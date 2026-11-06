//! Activities run here: a WebView2 that is a child of the window the call is in, kept over the
//! call's stage. It loads one of our own pages from a folder mapped to discord.com, which holds the
//! activity in an iframe like Discord's client does and passes the iframe's postMessages across;
//! the protocol itself is spoken in Rust (`ui/activity/bridge.rs`). The page has no way to reach
//! anything but its one frame, and the frame has no way off its own origin. Video embeds use the
//! same host with a page that holds just the iframe (`webwin/frame.rs`).

use std::collections::VecDeque;

use serde_json::{Value, json};

use super::*;

const PAGE: &str = include_str!("../../assets/activity_host.html");
const EMBED: &str = include_str!("../../assets/embed_host.html");
const AUTOPLAY: &str = "--autoplay-policy=no-user-gesture-required";
const HOST: &str = "discord.com";
const INBOX: usize = 512;
const MESSAGE: usize = 1 << 20;

pub struct Cfg {
    pub src: String,
    pub hello: Value,
    pub embed: bool,
}

#[derive(Clone, Copy, PartialEq, Default)]
struct Want {
    parent: isize,
    r: [i32; 4],
    show: bool,
}

pub enum Status {
    Starting,
    Up,
    Failed,
}

struct Inner {
    ctl: RefCell<Option<ICoreWebView2Controller>>,
    want: Cell<Want>,
    done: Cell<Want>,
    inbox: RefCell<VecDeque<Value>>,
    out: RefCell<Vec<String>>,
    up: Cell<bool>,
    failed: RefCell<Option<String>>,
    page: String,
    origin: String,
    embed: bool,
    ctx: egui::Context,
}

pub struct Host {
    s: Rc<Inner>,
    dirs: [PathBuf; 2],
}

pub fn origin_of(url: &str) -> Option<String> {
    let o = reqwest::Url::parse(url).ok()?.origin();
    o.is_tuple().then(|| o.ascii_serialization())
}

fn json_in_html(v: &Value) -> String {
    v.to_string().replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026")
}

fn loopback(origin: &str) -> bool {
    origin.starts_with("http://127.0.0.1")
}

fn top_allowed(url: &str, page: &str) -> bool {
    url == page || url == "about:blank"
}

fn frame_allowed(url: &str, origin: &str) -> bool {
    if url == "about:blank" || url == "about:srcdoc" {
        return true;
    }
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    u.scheme() == "https" || (u.scheme() == "http" && loopback(origin) && u.host_str() == Some("127.0.0.1"))
}

impl Host {
    pub fn open(ctx: &egui::Context, parent: isize, cfg: Cfg) -> Host {
        let root = crate::cache::dir().join("webview2");
        sweep(&root);
        let id = format!("{}-{}", std::process::id(), COUNT.fetch_add(1, Relaxed));
        let (data, site) = (root.join(&id), root.join(format!("{id}-page")));
        let origin = origin_of(&cfg.src).unwrap_or_default();
        let page = format!("https://{HOST}/index.html");
        let s = Rc::new(Inner {
            ctl: RefCell::new(None),
            want: Cell::new(Want { parent, ..Want::default() }),
            done: Cell::new(Want::default()),
            inbox: RefCell::new(VecDeque::new()),
            out: RefCell::new(Vec::new()),
            up: Cell::new(false),
            failed: RefCell::new(None),
            page,
            origin: origin.clone(),
            embed: cfg.embed,
            ctx: ctx.clone(),
        });
        let host = Host { s: s.clone(), dirs: [data.clone(), site.clone()] };
        LIVE.lock().unwrap().extend([data.clone(), site.clone()]);
        let html = (if cfg.embed { EMBED } else { PAGE }).replace("__ORIGIN__", &origin).replace("__CFG__", &json_in_html(&json!({ "src": cfg.src, "origin": origin, "hello": cfg.hello })));
        let written = std::fs::create_dir_all(&site).and_then(|_| std::fs::write(site.join("index.html"), html));
        if origin.is_empty() || written.is_err() {
            *s.failed.borrow_mut() = Some("the activity's address is not usable".into());
            return host;
        }
        let weak = Rc::downgrade(&s);
        create(parent as HWND, &data, if cfg.embed { AUTOPLAY } else { "" }, move |r| {
            let Some(s) = weak.upgrade() else { return };
            if let Err(e) = r.and_then(|ctl| start(&s, ctl, &site)) {
                *s.failed.borrow_mut() = Some(e.message());
            }
        });
        host
    }

    pub fn status(&self) -> Status {
        match &*self.s.failed.borrow() {
            Some(_) => Status::Failed,
            None if self.s.up.get() => Status::Up,
            None => Status::Starting,
        }
    }

    pub fn place(&self, parent: isize, r: [i32; 4], show: bool) {
        self.s.want.set(Want { parent, r, show });
        apply(&self.s);
    }

    pub fn placed(&self) -> ([i32; 4], bool) {
        let d = self.s.done.get();
        (d.r, d.show)
    }

    pub fn blur(&self) {
        let w = self.s.done.get().parent;
        if w != 0 {
            unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(w as HWND) };
        }
    }

    pub fn poll(&self) -> Vec<Value> {
        self.s.inbox.borrow_mut().drain(..).collect()
    }

    pub fn post(&self, frame: &Value) {
        let msg = json!({ "t": "post", "data": frame }).to_string();
        if self.s.up.get() {
            send(&self.s, &msg);
        } else {
            self.s.out.borrow_mut().push(msg);
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if let Some(ctl) = self.s.ctl.borrow_mut().take() {
            let _ = unsafe { ctl.Close() };
        }
        let dirs = self.dirs.clone();
        let _ = std::thread::Builder::new().name("webview2-clean".into()).spawn(move || {
            for d in &dirs {
                for _ in 0..10 {
                    if std::fs::remove_dir_all(d).is_ok() || !d.exists() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(300));
                }
            }
            LIVE.lock().unwrap().retain(|p| !dirs.contains(p));
        });
    }
}

fn send(s: &Inner, msg: &str) {
    if let Some(wv) = s.ctl.borrow().as_ref().and_then(|c| unsafe { c.CoreWebView2() }.ok()) {
        let _ = unsafe { wv.PostWebMessageAsJson(&HSTRING::from(msg)) };
    }
}

fn apply(s: &Inner) {
    let (want, done) = (s.want.get(), s.done.get());
    if want == done {
        return;
    }
    let ctl = s.ctl.borrow();
    let Some(ctl) = ctl.as_ref() else { return };
    unsafe {
        if want.parent != done.parent && want.parent != 0 {
            let _ = ctl.SetParentWindow(Hwnd(want.parent as HWND));
        }
        let [x, y, w, h] = want.r;
        if want.r != done.r || want.parent != done.parent {
            let _ = ctl.SetBounds(WinRect { left: x, top: y, right: x + w, bottom: y + h });
            let _ = ctl.NotifyParentWindowPositionChanged();
        }
        if want.show != done.show {
            let _ = ctl.SetIsVisible(want.show);
        }
    }
    s.done.set(want);
}

fn args_uri(args: &ICoreWebView2NavigationStartingEventArgs) -> String {
    let mut u = PWSTR::null();
    unsafe { args.Uri(&mut u) }.map(|_| take_pwstr(u)).unwrap_or_default()
}

fn start(s: &Rc<Inner>, ctl: ICoreWebView2Controller, site: &Path) -> windows::core::Result<()> {
    let wv = unsafe { ctl.CoreWebView2()? };
    unsafe {
        if let Ok(c) = ctl.cast::<ICoreWebView2Controller2>() {
            let _ = c.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR { A: 255, R: 0, G: 0, B: 0 });
        }
        let st = wv.Settings()?;
        st.SetAreDevToolsEnabled(false)?;
        st.SetIsStatusBarEnabled(false)?;
        st.SetAreDefaultContextMenusEnabled(false)?;
        st.SetAreDefaultScriptDialogsEnabled(false)?;
        st.SetIsZoomControlEnabled(false)?;
        st.SetIsBuiltInErrorPageEnabled(false)?;
        if let Ok(st) = st.cast::<ICoreWebView2Settings3>() {
            let _ = st.SetAreBrowserAcceleratorKeysEnabled(false);
        }
        if let Ok(st) = st.cast::<ICoreWebView2Settings4>() {
            let _ = st.SetIsGeneralAutofillEnabled(false);
            let _ = st.SetIsPasswordAutosaveEnabled(false);
        }
        wv.cast::<ICoreWebView2_3>()?.SetVirtualHostNameToFolderMapping(
            &HSTRING::from(HOST),
            &HSTRING::from(site.as_os_str()),
            COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY,
        )?;
    }
    let mut t = 0i64;
    let weak = Rc::downgrade(s);
    unsafe {
        let w = weak.clone();
        wv.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let (Some(s), Some(args)) = (w.upgrade(), args)
                    && !top_allowed(&args_uri(&args), &s.page)
                {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &mut t,
        )?;
        let w = weak.clone();
        wv.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let (Some(s), Some(args)) = (w.upgrade(), args)
                    && !frame_allowed(&args_uri(&args), &s.origin)
                {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &mut t,
        )?;
        let w = weak.clone();
        wv.add_FrameNavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let (Some(s), Some(args)) = (w.upgrade(), args) else { return Ok(()) };
                let mut ok = Default::default();
                args.IsSuccess(&mut ok)?;
                if !ok.as_bool() {
                    push(&s, json!({ "t": "frame_error" }));
                }
                Ok(())
            })),
            &mut t,
        )?;
        let popups = s.embed;
        wv.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
                    let mut u = PWSTR::null();
                    if popups && args.Uri(&mut u).is_ok() {
                        let u = take_pwstr(u);
                        if u.starts_with("https://") {
                            browser(&u);
                        }
                    }
                }
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                Ok(())
            })),
            &mut t,
        )?;
        wv.cast::<ICoreWebView2_4>()?.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &mut t,
        )?;
        let w = weak.clone();
        wv.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let (Some(s), Some(args)) = (w.upgrade(), args) else { return Ok(()) };
                let mut j = PWSTR::null();
                args.WebMessageAsJson(&mut j)?;
                let text = take_pwstr(j);
                if text.len() <= MESSAGE
                    && let Ok(v) = serde_json::from_str::<Value>(&text)
                {
                    push(&s, v);
                }
                Ok(())
            })),
            &mut t,
        )?;
        let w = weak.clone();
        wv.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let (Some(s), Some(args)) = (w.upgrade(), args) else { return Ok(()) };
                let mut ok = Default::default();
                args.IsSuccess(&mut ok)?;
                if !ok.as_bool() {
                    *s.failed.borrow_mut() = Some("the page could not be loaded".into());
                    return Ok(());
                }
                s.up.set(true);
                let queued: Vec<String> = s.out.borrow_mut().drain(..).collect();
                for m in queued {
                    send(&s, &m);
                }
                s.ctx.request_repaint();
                Ok(())
            })),
            &mut t,
        )?;
        wv.Navigate(&HSTRING::from(s.page.as_str()))?;
    }
    *s.ctl.borrow_mut() = Some(ctl);
    apply(s);
    Ok(())
}

fn push(s: &Inner, v: Value) {
    let mut q = s.inbox.borrow_mut();
    if q.len() < INBOX {
        q.push_back(v);
        s.ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const O: &str = "https://1234.discordsays.com";

    #[test]
    fn the_page_cannot_be_left() {
        let page = "https://discord.com/index.html";
        assert!(top_allowed(page, page));
        for u in ["https://1234.discordsays.com/", "https://evil.com/", "file:///c:/x", "javascript:1", "https://discord.com/index.html#x"] {
            assert!(!top_allowed(u, page), "{u}");
        }
    }

    #[test]
    fn frames_stay_on_https() {
        for u in ["https://1234.discordsays.com/a", "https://www.youtube.com/embed/x", "about:blank"] {
            assert!(frame_allowed(u, O), "{u}");
        }
        for u in ["http://1234.discordsays.com/", "file:///c:/x", "ms-msdt:/id", "data:text/html,x", "javascript:1", "http://127.0.0.1:1/"] {
            assert!(!frame_allowed(u, O), "{u}");
        }
        assert!(frame_allowed("http://127.0.0.1:5/a", "http://127.0.0.1:5"));
        assert!(!frame_allowed("http://localhost:5/a", "http://127.0.0.1:5"));
    }

    #[test]
    fn config_cannot_close_its_script() {
        let s = json_in_html(&json!({ "src": "https://a/</script><script>alert(1)" }));
        assert!(!s.contains('<') && !s.contains('>'));
    }

    #[test]
    fn origins_are_scheme_host_port() {
        assert_eq!(origin_of("https://1.discordsays.com/?a=1").as_deref(), Some("https://1.discordsays.com"));
        assert_eq!(origin_of("http://127.0.0.1:9/x").as_deref(), Some("http://127.0.0.1:9"));
        assert_eq!(origin_of("data:text/html,x"), None);
    }
}

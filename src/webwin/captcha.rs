//! The login captcha: hCaptcha in the small window, served from a virtual host named discord.com so
//! the sitekey sees the origin it is bound to. The page is the only top-level page, other windows
//! are dropped, and the account's token isn't there; the answer comes back as a web message.

use super::*;

const HOST: &str = "discord.com";

fn hcaptcha() -> String {
    std::env::var("RUSTCORD_HCAPTCHA").unwrap_or_else(|_| "https://js.hcaptcha.com".into())
}

fn page(sitekey: &str, rqdata: Option<&str>) -> String {
    let data = rqdata.map(|d| format!("hcaptcha.setData(id, {{ rqdata: {} }});", serde_json::json!(d))).unwrap_or_default();
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>Discord</title>
<style>html,body{{background:#1e1f22;margin:0;height:100%}}body{{display:grid;place-items:center}}</style>
<div id="c"></div>
<script src="{api}/1/api.js?render=explicit&onload=ready" async defer></script>
<script>
function ready() {{
  var id = hcaptcha.render("c", {{ sitekey: {sitekey}, theme: "dark", callback: function (t) {{ window.chrome.webview.postMessage(t); }} }});
  {data}
}}
</script>"#,
        api = hcaptcha(),
        sitekey = serde_json::json!(sitekey),
    )
}

fn top_allowed(url: &str) -> bool {
    url == "about:blank" || url == format!("https://{HOST}/index.html")
}

fn frame_allowed(url: &str) -> bool {
    if url == "about:blank" || url == "about:srcdoc" {
        return true;
    }
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    let own = reqwest::Url::parse(&hcaptcha()).is_ok_and(|b| b.origin() == u.origin());
    own || (u.scheme() == "https" && host_in(&u, "hcaptcha.com"))
}

fn uri(args: &ICoreWebView2NavigationStartingEventArgs) -> String {
    let mut u = PWSTR::null();
    unsafe { args.Uri(&mut u) }.map(|_| take_pwstr(u)).unwrap_or_default()
}

pub fn solve(sitekey: &str, rqdata: Option<&str>) -> Option<String> {
    if BUSY.swap(true, Relaxed) {
        return None;
    }
    let parent = crate::winsys::WINDOW.load(Relaxed);
    let (tx, rx) = mpsc::channel();
    let html = page(sitekey, rqdata);
    let started = std::thread::Builder::new().name("captcha".into()).spawn(move || run(parent, move |hwnd, data| setup(hwnd, data, &html, tx)));
    if started.is_err() {
        BUSY.store(false, Relaxed);
        return None;
    }
    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(t) => return Some(t),
            Err(mpsc::RecvTimeoutError::Timeout) if BUSY.load(Relaxed) => {}
            _ => return rx.try_recv().ok(),
        }
    }
}

fn setup(hwnd: HWND, data: &Path, html: &str, tx: mpsc::Sender<String>) -> windows::core::Result<()> {
    let site = data.join("page");
    std::fs::create_dir_all(&site)?;
    std::fs::write(site.join("index.html"), html)?;
    let (done, rx) = mpsc::channel();
    create(hwnd, data, "", move |r| {
        let _ = done.send(r);
    });
    let ctl = webview2_com::wait_with_pump(rx).map_err(|_| windows::core::Error::from(E_POINTER))??;
    let wv = unsafe { ctl.CoreWebView2()? };
    unsafe {
        if let Ok(c) = ctl.cast::<ICoreWebView2Controller2>() {
            let _ = c.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR { A: 255, R: 0x1e, G: 0x1f, B: 0x22 });
        }
        let st = wv.Settings()?;
        st.SetAreDevToolsEnabled(false)?;
        st.SetIsStatusBarEnabled(false)?;
        st.SetAreDefaultContextMenusEnabled(false)?;
        wv.cast::<ICoreWebView2_3>()?.SetVirtualHostNameToFolderMapping(&HSTRING::from(HOST), &HSTRING::from(site.as_os_str()), COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY)?;
        ctl.SetIsVisible(true)?;
    }
    fit(&ctl, hwnd);
    let mut pid = 0u32;
    let _ = unsafe { wv.BrowserProcessId(&mut pid) };
    BROWSER.store(pid, Relaxed);
    let mut t = 0i64;
    unsafe {
        wv.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args
                    && !top_allowed(&uri(&args))
                {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args
                    && !frame_allowed(&uri(&args))
                {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
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
        wv.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
                Ok(())
            })),
            &mut t,
        )?;
        wv.add_WebMessageReceived(
            &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut s = PWSTR::null();
                args.TryGetWebMessageAsString(&mut s)?;
                let token = take_pwstr(s);
                if !token.is_empty() {
                    let _ = tx.send(token);
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                }
                Ok(())
            })),
            &mut t,
        )?;
        ctl.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let (mut kind, mut key) = (Default::default(), 0u32);
                    args.KeyEventKind(&mut kind)?;
                    args.VirtualKey(&mut key)?;
                    if kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && key == 0x1b {
                        args.SetHandled(true)?;
                        PostMessageW(hwnd, WM_CLOSE, 0, 0);
                    }
                }
                Ok(())
            })),
            &mut t,
        )?;
    }
    let v = Rc::new(View { ctl, wv });
    VIEW.with(|c| *c.borrow_mut() = Some(v.clone()));
    navigate(&v, &format!("https://{HOST}/index.html"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_captcha_page_and_hcaptcha_frames_load() {
        assert!(top_allowed("https://discord.com/index.html"));
        assert!(!top_allowed("https://discord.com/login"));
        assert!(!top_allowed("https://hcaptcha.com/"));
        assert!(frame_allowed("https://newassets.hcaptcha.com/captcha/v1/x/static/hcaptcha.html"));
        assert!(frame_allowed("about:blank"));
        assert!(!frame_allowed("https://evilhcaptcha.com/"));
        assert!(!frame_allowed("https://hcaptcha.com.evil.com/"));
    }
}

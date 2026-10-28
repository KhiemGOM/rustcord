//! Vencord, hosted: its plugins' own code running in QuickJS against Vencord's APIs, which
//! Fastcord provides natively. Everything crosses the boundary as JSON: the app calls into
//! `__host` with what happened, plugins call `__fc` for what they want done, and those asks
//! queue up for the app to carry out after the call.

pub mod css;
mod sources;
mod stores;
mod transpile;

pub use sources::vencord_dir;
pub use stores::{View, message};

use rquickjs::{CatchResultExt, Context, Ctx, Function, Object, Runtime};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use sources::Sources;

// the shims: Vencord's modules that reach into Discord's client, written against Fastcord
// instead, by the path a plugin's import resolves to
const SHIMS: &[(&str, &str, &str)] = &[
    ("fc:host", "host.jsx", include_str!("js/host.jsx")),
    ("react", "react.js", include_str!("js/react.js")),
    ("src/utils/types", "types.js", include_str!("js/types.js")),
    ("src/api/Settings", "settings.js", include_str!("js/settings.js")),
    ("src/api/Commands", "commands.js", include_str!("js/commands.js")),
    ("src/api/MessageEvents", "events.js", include_str!("js/events.js")),
    ("src/api/Notifications", "notifications.js", include_str!("js/notifications.js")),
    ("src/api/ContextMenu", "menu.js", include_str!("js/menu.js")),
    ("src/api/ChatButtons", "chatbar.jsx", include_str!("js/chatbar.jsx")),
    ("src/api/MessagePopover", "popover.js", include_str!("js/popover.js")),
    ("src/api/DataStore", "datastore.js", include_str!("js/datastore.js")),
    ("src/api/Styles", "styles.js", include_str!("js/styles.js")),
    ("src/api/Badges", "badges.js", include_str!("js/badges.js")),
    ("src/api/MessageAccessories", "decorations.js", include_str!("js/decorations.js")),
    ("src/api/MessageDecorations", "decorations.js", include_str!("js/decorations.js")),
    ("src/api/MemberListDecorators", "decorations.js", include_str!("js/decorations.js")),
    ("src/utils/Logger", "logger.js", include_str!("js/logger.js")),
    ("src/utils/patches", "patches.js", include_str!("js/patches.js")),
    ("src/utils/discord", "discord.js", include_str!("js/discord.js")),
    ("src/utils/clipboard", "clipboard.js", include_str!("js/clipboard.js")),
    ("src/components/ErrorBoundary", "errorboundary.js", include_str!("js/errorboundary.js")),
    ("src/components/Switch", "switch.js", include_str!("js/switch.js")),
    ("src/components/Button", "button.js", include_str!("js/button.js")),
    ("src/components/BaseText", "basetext.js", include_str!("js/basetext.js")),
    ("src/utils/modal", "modal.js", include_str!("js/modal.js")),
    ("src/webpack", "webpack.js", include_str!("js/webpack.js")),
    ("src/webpack/common", "common.jsx", include_str!("js/common.jsx")),
];

fn shim(path: &str) -> Option<(&'static str, &'static str)> {
    SHIMS.iter().find(|(p, ..)| *p == path).map(|(_, f, src)| (*f, *src))
}

thread_local! {
    static VIEW: Cell<*const ()> = const { Cell::new(std::ptr::null()) };
}

struct State {
    sources: Sources,
    js: HashMap<String, String>,
    out: Vec<Value>,
}

pub struct Host {
    rt: Runtime,
    ctx: Context,
    st: Rc<RefCell<State>>,
    deadline: Rc<Cell<Option<Instant>>>,
}

/// The module at `id`, transpiled once, as the body of a function of (exports, require, module).
fn compile(st: &RefCell<State>, id: &str) -> Result<String, String> {
    if let Some(c) = st.borrow().js.get(id) {
        return Ok(c.clone());
    }
    let st = &mut *st.borrow_mut();
    let (path, src) = match shim(id) {
        Some((file, src)) => (format!("shim/{file}"), src),
        None => (id.to_string(), st.sources.get(id).unwrap_or_default()),
    };
    let c = format!("(function (exports, __require, module) {{
{}
}})", transpile::transpile(&path, src)?);
    st.js.insert(id.to_string(), c.clone());
    Ok(c)
}

impl Host {
    /// `extra` is one more userplugins folder, Fastcord's own; `settings` is in Vencord's
    /// settings.json shape and `data` is the DataStore. `None` with no Vencord to host.
    pub fn new(extra: &std::path::Path, settings: &Value, data: &Value) -> Option<Host> {
        Self::new_with(extra, &sources::vencord_dir(), settings, data)
    }

    pub fn new_with(extra: &std::path::Path, vencord: &std::path::Path, settings: &Value, data: &Value) -> Option<Host> {
        let sources = Sources::load(extra, vencord);
        if sources.plugins().is_empty() {
            return None;
        }
        let rt = Runtime::new().ok()?;
        rt.set_memory_limit(512 << 20);
        let deadline: Rc<Cell<Option<Instant>>> = Rc::default();
        let d = deadline.clone();
        rt.set_interrupt_handler(Some(Box::new(move || d.get().is_some_and(|t| Instant::now() > t))));
        let ctx = Context::full(&rt).ok()?;
        let st = Rc::new(RefCell::new(State { sources, js: HashMap::new(), out: Vec::new() }));
        let started = ctx.with(|ctx| {
            (|| -> rquickjs::Result<()> {
                let fc = Object::new(ctx.clone())?;
                fc.set(
                    "log",
                    Function::new(ctx.clone(), |level: i32, msg: String| match level {
                        3 => eprintln!("[vencord] error: {msg}"),
                        2 => eprintln!("[vencord] warn: {msg}"),
                        _ => eprintln!("[vencord] {msg}"),
                    })?,
                )?;
                let s = st.clone();
                fc.set("resolve", Function::new(ctx.clone(), move |from: String, spec: String| s.borrow().sources.resolve(&from, &spec, |p| shim(p).is_some()))?)?;
                let s = st.clone();
                fc.set(
                    "source",
                    Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: String| compile(&s, &id).map_err(|e| rquickjs::Exception::throw_message(&ctx, &e)))?,
                )?;
                fc.set(
                    "q",
                    Function::new(ctx.clone(), |kind: String, a: String, b: String| -> String {
                        VIEW.with(|v| {
                            let p = v.get() as *const View<'static>;
                            // SAFETY: only set for the length of a `call`, from a `View` that outlives it
                            if p.is_null() { Value::Null } else { stores::query(unsafe { &*p }, &kind, &a, &b) }
                        })
                        .to_string()
                    })?,
                )?;
                let s = st.clone();
                fc.set(
                    "out",
                    Function::new(ctx.clone(), move |json: String| {
                        if let Ok(v) = serde_json::from_str(&json) {
                            s.borrow_mut().out.push(v);
                        }
                    })?,
                )?;
                ctx.globals().set("__fc", fc)?;
                ctx.eval::<(), _>(include_str!("js/prelude.js"))?;
                ctx.eval::<(), _>(format!("globalThis.__fcSettings = {settings}; globalThis.__fcData = {data}; __load(\"fc:host\");"))?;
                Ok(())
            })()
            .catch(&ctx)
            .map_err(|e| e.to_string())
        });
        if let Err(e) = started {
            eprintln!("[vencord] couldn't start: {e}");
            return None;
        }
        Some(Host { rt, ctx, st, deadline })
    }

    /// Calls `__host[name](arg)` and returns what it gave back, with the store readable for
    /// the length of it and every promise the call set off run as far as it can go.
    pub fn call(&self, view: &View, name: &str, arg: Value) -> Value {
        VIEW.with(|v| v.set(view as *const View as *const ()));
        self.deadline.set(Some(Instant::now() + Duration::from_secs(3)));
        let out = self.ctx.with(|ctx| {
            let r = (|| -> rquickjs::Result<String> {
                let host: Object = ctx.globals().get("__host")?;
                let f: Function = host.get(name)?;
                f.call((arg.to_string(),))
            })()
            .catch(&ctx);
            match r {
                Ok(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
                Err(e) => {
                    eprintln!("[vencord] {name}: {e}");
                    Value::Null
                }
            }
        });
        while self.rt.is_job_pending() {
            if self.rt.execute_pending_job().is_err() {
                eprintln!("[vencord] a promise job threw");
            }
        }
        self.deadline.set(None);
        VIEW.with(|v| v.set(std::ptr::null()));
        out
    }

    /// The installed Vencord's commit and GitHub repo.
    pub fn version(&self) -> (Option<String>, Option<String>) {
        let st = self.st.borrow();
        (st.sources.hash.clone(), st.sources.remote.clone())
    }

    /// Every plugin's entry file, official then userplugins.
    pub fn plugins(&self) -> Vec<String> {
        self.st.borrow().sources.plugins()
    }

    /// What the plugins asked for since last time: requests, toasts, messages to send...
    pub fn take(&self) -> Vec<Value> {
        std::mem::take(&mut self.st.borrow_mut().out)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    const PLUGIN: &str = r#"
import definePlugin, { OptionType } from "@utils/types";
import { definePluginSettings } from "@api/Settings";
import { ChatBarButton } from "@api/ChatButtons";
import { Menu, UserStore } from "@webpack/common";
import { findByPropsLazy } from "@webpack";

const missing = findByPropsLazy("doesNotExist");
const settings = definePluginSettings({
    suffix: { type: OptionType.STRING, description: "Added to every message", default: "!" },
    loud: { type: OptionType.BOOLEAN, description: "Shout", default: false },
});
let seen = 0;

export default definePlugin({
    name: "FcTest",
    description: "Exercises the host",
    authors: [{ name: "fastcord", id: 0n }],
    settings,
    commands: [{ name: "fctest", description: "says hi", options: [], execute: (args, ctx) => ({ content: `hi from ${ctx.channel?.name ?? "nowhere"}` }) }],
    flux: { MESSAGE_CREATE({ message }) { seen++; if (message.content === "ping") setTimeout(() => __out({ op: "toast", text: `pong ${seen}` }), 0); } },
    contextMenus: { message: (children, { message }) => children.push(<Menu.MenuItem id="fc-test" label={`Quote ${message.author.username}`} action={() => __out({ op: "toast", text: "quoted" })} />) },
    chatBarButton: { icon: () => null, render: () => <ChatBarButton tooltip="Test" onClick={() => settings.store.loud = true}><svg viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z" /></svg></ChatBarButton> },
    onBeforeMessageSend(channelId, msg) { msg.content += settings.store.suffix; if (settings.store.loud) msg.content = msg.content.toUpperCase(); },
    start() { missing.whatever(); },
});
"#;

    #[test]
    fn runs_a_plugin() {
        let dir = std::env::temp_dir().join(format!("fc_vencord_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("fcTest"));
        std::fs::write(dir.join("fcTest/index.tsx"), PLUGIN).unwrap();
        let host = super::Host::new_with(&dir, &dir.join("none"), &json!({ "plugins": { "FcTest": { "enabled": true } } }), &json!({})).expect("host");
        let mut store = crate::state::Store::default();
        store.me = 1;
        store.users.insert(2, crate::state::User { id: 2, username: "alice".into(), ..Default::default() });
        store.channels.insert(10, crate::state::Channel { id: 10, name: "general".into(), ..Default::default() });
        store.msgs.entry(10).or_default().list.push(crate::state::Message { id: 100, channel: 10, author: 2, content: "hey".into(), ..Default::default() });
        let view = super::View { store: &store, channel: 10, guild: 0, voice: 0 };
        let paths = host.plugins();
        assert_eq!(paths, vec!["src/userplugins/fcTest/index.tsx"]);
        host.call(&view, "start", json!({ "paths": paths }));
        let list = host.call(&view, "list", json!({ "paths": paths }));
        assert_eq!(list[0]["name"], "FcTest", "{list}");
        assert_eq!(list[0]["started"], true, "{list}");
        assert_eq!(list[0]["options"][0]["value"], "!", "{list}");

        host.call(&view, "preSend", json!({ "token": 7, "channel": "10", "content": "hello" }));
        let out = host.take();
        assert!(out.iter().any(|o| o["op"] == "sent" && o["token"] == 7 && o["content"] == "hello!"), "{out:?}");

        let menu = host.call(&view, "menu", json!({ "nav": "message", "message": super::stores::message(&store, &store.msgs[&10].list[0]), "channel": "10" }));
        assert_eq!(menu[0]["label"], "Quote alice", "{menu}");
        host.call(&view, "act", json!({ "id": menu[0]["action"] }));
        assert!(host.take().iter().any(|o| o["text"] == "quoted"));

        let bar = host.call(&view, "chatBar", json!({ "channel": "10", "empty": true }));
        assert!(bar[0]["icon"].as_str().is_some_and(|s| s.contains("<path") && s.contains("#ffffff")), "{bar}");
        host.call(&view, "act", json!({ "id": bar[0]["click"] }));
        host.call(&view, "preSend", json!({ "token": 8, "channel": "10", "content": "hello" }));
        let out = host.take();
        assert!(out.iter().any(|o| o["op"] == "settings" && o["value"]["plugins"]["FcTest"]["loud"] == true), "{out:?}");
        assert!(out.iter().any(|o| o["token"] == 8 && o["content"] == "HELLO!"), "{out:?}");

        let cmds = host.call(&view, "commands", json!({}));
        assert_eq!(cmds[0]["name"], "fctest");
        host.call(&view, "runCommand", json!({ "name": "fctest", "args": [], "channel": "10" }));
        assert!(host.take().iter().any(|o| o["op"] == "send" && o["content"] == "hi from general"));

        host.call(&view, "dispatch", json!({ "t": "MESSAGE_CREATE", "d": { "id": "101", "channel_id": "10", "content": "ping", "author": { "id": "2" } } }));
        host.call(&view, "tick", json!({}));
        assert!(host.take().iter().any(|o| o["text"] == "pong 1"));

        host.call(&view, "setEnabled", json!({ "name": "FcTest", "on": false }));
        host.call(&view, "preSend", json!({ "token": 9, "channel": "10", "content": "plain" }));
        assert!(host.take().iter().any(|o| o["token"] == 9 && o["content"] == "plain"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    const UI_PLUGIN: &str = r#"
import definePlugin, { OptionType } from "@utils/types";
import { definePluginSettings } from "@api/Settings";
import { Button, Forms, Text, TextInput, useEffect, useState } from "@webpack/common";
import { ModalContent, ModalRoot, openModal } from "@utils/modal";

function Counter({ message }) {
    const [n, setN] = useState(0);
    const [ready, setReady] = useState(false);
    useEffect(() => { setReady(true); }, []);
    return <div><Text>{`count ${n} ${ready ? "ready" : "waiting"} on ${message.content}`}</Text><Button onClick={() => setN(n + 1)}>Add</Button></div>;
}

const settings = definePluginSettings({
    word: { type: OptionType.COMPONENT, component: ({ setValue }) => <Forms.FormSection title="Word"><TextInput value="" onChange={v => setValue(v)} /></Forms.FormSection> },
});

export default definePlugin({
    name: "FcUi",
    description: "Plugin UI",
    authors: [],
    settings,
    renderMessageAccessory: props => props.message.content === "hey" ? <Counter message={props.message} /> : null,
    userProfileBadge: { description: "Tester", iconSrc: "https://example.com/b.png", shouldShow: ({ userId }) => userId === "2" },
    commands: [{ name: "fcmodal", description: "", execute: () => { openModal(props => <ModalRoot><ModalContent><Text>in a modal</Text><Button onClick={props.onClose}>Close</Button></ModalContent></ModalRoot>); } }],
});
"#;

    fn find<'a>(tree: &'a serde_json::Value, pred: &dyn Fn(&serde_json::Value) -> bool) -> Option<&'a serde_json::Value> {
        if pred(tree) {
            return Some(tree);
        }
        let kids = tree.as_array().or_else(|| tree["c"].as_array())?;
        kids.iter().find_map(|k| find(k, pred))
    }

    fn text(tree: &serde_json::Value) -> String {
        match tree {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(a) => a.iter().map(text).collect(),
            v => v["c"].as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default(),
        }
    }

    #[test]
    fn draws_plugin_ui() {
        let dir = std::env::temp_dir().join(format!("fc_vencord_ui_{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("fcUi"));
        std::fs::write(dir.join("fcUi/index.tsx"), UI_PLUGIN).unwrap();
        let host = super::Host::new_with(&dir, &dir.join("none"), &json!({ "plugins": { "FcUi": { "enabled": true } } }), &json!({})).expect("host");
        let mut store = crate::state::Store::default();
        store.me = 1;
        store.users.insert(2, crate::state::User { id: 2, username: "alice".into(), ..Default::default() });
        store.channels.insert(10, crate::state::Channel { id: 10, name: "general".into(), ..Default::default() });
        store.msgs.entry(10).or_default().list.push(crate::state::Message { id: 100, channel: 10, author: 2, content: "hey".into(), ..Default::default() });
        let view = super::View { store: &store, channel: 10, guild: 0, voice: 0 };
        host.call(&view, "start", json!({ "paths": host.plugins() }));

        let msg = super::stores::message(&store, &store.msgs[&10].list[0]);
        host.call(&view, "accessories", json!({ "message": msg, "channel": "10" }));
        // the effect ran after the first render and asked for another
        assert!(host.take().iter().any(|o| o["op"] == "dirty" && o["root"] == "acc:100"));
        let tree = host.call(&view, "root", json!({ "root": "acc:100" }));
        assert!(text(&tree).contains("count 0 ready on hey"), "{tree}");
        let button = find(&tree, &|n| n["t"] == "fc-button").expect("button");
        let tree = host.call(&view, "event", json!({ "root": "acc:100", "id": button["h"]["onClick"], "args": [] }));
        assert!(text(&tree).contains("count 1 ready"), "{tree}");

        host.call(&view, "runCommand", json!({ "name": "fcmodal", "args": [], "channel": "10" }));
        let opened = host.take();
        let modal = opened.iter().find(|o| o["op"] == "modal").map(|o| o["root"].as_str().unwrap().to_string()).expect("modal opened");
        let tree = host.call(&view, "root", json!({ "root": modal }));
        assert!(find(&tree, &|n| n["t"] == "fc-modal").is_some() && text(&tree).contains("in a modal"), "{tree}");
        let close = find(&tree, &|n| n["t"] == "fc-button").expect("close");
        host.call(&view, "event", json!({ "root": modal, "id": close["h"]["onClick"], "args": [] }));
        assert!(host.take().iter().any(|o| o["op"] == "modalClose"));

        let ui = host.call(&view, "settingUi", json!({ "plugin": "FcUi", "key": "word" }));
        let input = find(&ui, &|n| n["t"] == "fc-input").expect("input");
        host.call(&view, "event", json!({ "root": "setting:FcUi:word", "id": input["h"]["onChange"], "args": ["typed"] }));
        assert!(host.take().iter().any(|o| o["op"] == "settings" && o["value"]["plugins"]["FcUi"]["word"] == "typed"));

        let badges = host.call(&view, "badges", json!({ "user": "2" }));
        assert_eq!(badges[0]["tooltip"], "Tester", "{badges}");
        assert_eq!(host.call(&view, "badges", json!({ "user": "3" })), json!([]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every installed Vencord plugin loads, and lists. Needs Vencord installed, so it only runs
    /// when asked for.
    #[test]
    #[ignore]
    fn installed_plugins_load() {
        let settings: serde_json::Value = std::fs::read(super::sources::vencord_dir().join("settings/settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}));
        let host = super::Host::new(std::path::Path::new("nowhere"), &settings, &json!({})).expect("host");
        let store = crate::state::Store::default();
        let view = super::View { store: &store, channel: 0, guild: 0, voice: 0 };
        let paths = host.plugins();
        let t = std::time::Instant::now();
        host.call(&view, "start", json!({ "paths": paths }));
        let list = host.call(&view, "list", json!({ "paths": paths }));
        let took = t.elapsed();
        let list = list.as_array().unwrap();
        let bad: Vec<String> = list.iter().filter(|p| !p["error"].is_null()).map(|p| format!("{}: {}", p["name"], p["error"].as_str().unwrap_or("").lines().next().unwrap_or(""))).collect();
        let started: Vec<&str> = list.iter().filter(|p| p["started"] == true).filter_map(|p| p["name"].as_str()).collect();
        println!("{} plugins in {took:?}; started {started:?}", list.len());
        println!("no patches: {:?}", list.iter().filter(|p| p["patches"] == 0).filter_map(|p| p["name"].as_str()).collect::<Vec<_>>());
        assert!(bad.is_empty(), "{} failed:
{}", bad.len(), bad.join("
"));
    }
}

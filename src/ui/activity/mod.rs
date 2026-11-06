//! Activities: the shelf behind Start An Activity, launching and joining one in the call you're in,
//! and hosting it in a web view over the call's stage (`crate::webwin::child`), with the Embedded App
//! SDK's protocol (`bridge`) between the page and the client.

use std::time::{Duration, Instant};

use egui::Context;
use serde_json::{Value, json};

use super::App;
use crate::net::Tag;
use crate::state::{Id, Map, Set, sid};
use crate::webwin::child::{Cfg, Host, Status, origin_of};

pub mod bridge;
mod shelf;
pub mod view;

const LAUNCH_WAIT: Duration = Duration::from_secs(20);

pub struct Entry {
    pub app: Id,
    pub label: &'static str,
}

#[derive(Default, Clone)]
pub struct Info {
    pub name: String,
    pub desc: String,
    pub icon: Option<String>,
    pub banner: Option<String>,
    pub promoted: bool,
}

struct Start {
    app: Id,
    channel: Id,
    at: Instant,
    instance: Option<String>,
    ticket: Option<Result<String, String>>,
}

pub struct Run {
    pub app: Id,
    pub channel: Id,
    pub instance: String,
    pub host: Host,
    sock: bridge::Socket,
    pub failed: bool,
}

pub struct Consent {
    pub name: String,
    pub icon: Option<String>,
    pub scopes: Vec<String>,
}

struct Auth {
    cmd: &'static str,
    nonce: String,
    scopes: Vec<String>,
    state: Option<String>,
    quiet: bool,
    token: Option<String>,
}

#[derive(Default)]
pub struct Activity {
    pub shelf: Option<shelf::Shelf>,
    pub entries: Map<Id, Vec<Entry>>,
    pub apps: Map<Id, Info>,
    asked: Set<Id>,
    pub run: Option<Run>,
    starting: Option<Start>,
    pub consent: Option<Consent>,
    auth: Option<Auth>,
    link: Option<String>,
    pub surface: isize,
    pub in_popout: bool,
    pub placed: bool,
    after_join: Option<(Id, Id)>,
}

fn origin_template() -> String {
    std::env::var("RUSTCORD_ACTIVITY").unwrap_or_else(|_| "https://{app}.discordsays.com".into())
}

fn cdn(path: String) -> String {
    format!("{}/{path}", crate::api::cdn())
}

fn query(pairs: &[(&str, &str)]) -> String {
    let mut u = reqwest::Url::parse("http://q/").expect("static url");
    u.query_pairs_mut().extend_pairs(pairs);
    u.query().unwrap_or_default().to_string()
}

fn random_id() -> String {
    use std::hash::BuildHasher;
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let h = |salt: u64| std::collections::hash_map::RandomState::new().hash_one((salt, n, std::time::SystemTime::now()));
    format!("{:016x}{:016x}", h(1), h(2))
}

pub fn user_json(app: &App, u: Id) -> Value {
    let Some(user) = app.store.user(u) else { return json!({ "id": u.to_string() }) };
    json!({
        "id": u.to_string(),
        "username": user.username,
        "discriminator": "0",
        "global_name": user.global_name,
        "avatar": user.avatar,
        "bot": user.bot,
        "flags": user.flags,
        "premium_type": 0,
    })
}

impl Activity {
    pub fn runs_in(&self, ch: Id) -> bool {
        self.run.as_ref().is_some_and(|r| r.channel == ch)
    }

    pub fn ready(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.sock.is_open())
    }

    pub fn authed(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.sock.is_authed())
    }

    pub fn busy(&self) -> bool {
        self.run.is_some() || self.starting.is_some()
    }

    pub fn info(&self, app: Id) -> Option<&Info> {
        self.apps.get(&app)
    }

    pub fn linked(&mut self, opened: bool) -> Option<(String, bool)> {
        self.link.take().map(|n| (n, opened))
    }
}

pub fn overlays(app: &mut App, ctx: &Context) {
    shelf::show(app, ctx);
    view::consent(app, ctx);
}

pub fn parse_app(v: &Value) -> Option<(Id, Info)> {
    let id = sid(&v["id"]);
    if id == 0 {
        return None;
    }
    let icon = v["icon"].as_str().map(|h| cdn(format!("app-icons/{id}/{h}.png?size=240")));
    let info = Info {
        name: v["name"].as_str().unwrap_or("Activity").to_string(),
        desc: v["description"].as_str().unwrap_or_default().to_string(),
        icon,
        banner: None,
        promoted: v["flags"].as_u64().is_some_and(|f| f & (1 << 21) != 0),
    };
    Some((id, info))
}

pub fn fetch_apps(app: &mut App, ids: impl IntoIterator<Item = Id>) {
    let fresh: Vec<Id> = ids.into_iter().filter(|i| !app.activity.apps.contains_key(i) && app.activity.asked.insert(*i)).collect();
    if fresh.is_empty() {
        return;
    }
    let q: Vec<String> = fresh.iter().map(|i| format!("application_ids={i}")).collect();
    app.rest("GET", format!("/applications/public?{}", q.join("&")), None, Tag::Activity("apps".into()));
}

pub fn guild_of(app: &App) -> Id {
    app.voice.chan.and_then(|c| app.store.channels.get(&c)).map_or(0, |c| c.guild)
}

pub fn open_shelf(app: &mut App) {
    let g = guild_of(app);
    app.activity.shelf = Some(shelf::Shelf::default());
    let path = if g != 0 { format!("/activities/shelf?guild_id={g}") } else { "/activities/shelf".into() };
    app.rest("GET", path, None, Tag::Activity(format!("shelf:{g}")));
}

/// Opens an activity in the call we're in, or joins the one that's already running there.
pub fn launch(app: &mut App, id: Id) {
    let Some(ch) = app.voice.chan else { return };
    if app.activity.run.is_some() || app.activity.starting.is_some() {
        return;
    }
    let guild = guild_of(app);
    let mut body = json!({ "session_id": app.store.session });
    if guild != 0 {
        body["guild_id"] = json!(guild.to_string());
    }
    app.activity.shelf = None;
    app.activity.starting = Some(Start { app: id, channel: ch, at: Instant::now(), instance: None, ticket: None });
    fetch_apps(app, [id]);
    app.rest("POST", format!("/activities/{ch}/{id}"), Some(body), Tag::Activity("launch".into()));
}

/// Join from the channel list: straight in when we're in that call, else the call first.
pub fn join_from_list(app: &mut App, ch: Id, id: Id) {
    if app.voice.chan == Some(ch) {
        return launch(app, id);
    }
    app.join_voice(ch, true);
    app.activity.after_join = Some((ch, id));
}

pub fn leave(app: &mut App) {
    app.activity.starting = None;
    app.activity.auth = None;
    app.activity.consent = None;
    if app.activity.link.take().is_some() {
        app.leaving = None;
    }
    let Some(run) = app.activity.run.take() else { return };
    let path = format!("/applications/{}/activities/{}/instances/{}/leave", run.app, run.channel, run.instance);
    app.rest("POST", path, Some(json!({ "session_id": app.store.session })), Tag::Quiet);
}

pub fn on_rest(app: &mut App, key: &str, v: Value) {
    let (head, rest) = key.split_once(':').unwrap_or((key, ""));
    match head {
        "shelf" => {
            let g: Id = rest.parse().unwrap_or(0);
            let assets = &v["assets"];
            let mut list = Vec::new();
            let mut all: Vec<&Value> = v["activities"].as_array().into_iter().flatten().collect();
            all.sort_by_key(|a| a["shelf_rank"].as_i64().unwrap_or(i64::MAX));
            for a in all {
                let id = sid(&a["application_id"]);
                let label = match a["client_platform_config"]["desktop"]["label_type"].as_u64() {
                    Some(1) => "New",
                    Some(2) => "Updated",
                    _ => "",
                };
                list.push(Entry { app: id, label });
            }
            for a in v["applications"].as_array().into_iter().flatten() {
                if let Some((id, mut info)) = parse_app(a) {
                    info.banner = assets[id.to_string()]
                        .as_array()
                        .and_then(|l| l.iter().find(|x| x["name"].as_str().is_some_and(|n| n.contains("banner"))).or(l.first()))
                        .and_then(|x| x["id"].as_str())
                        .map(|h| cdn(format!("app-assets/{id}/{h}.webp?size=600")));
                    app.activity.apps.insert(id, info);
                }
            }
            app.activity.entries.insert(g, list);
        }
        "apps" => {
            for a in v.as_array().into_iter().flatten() {
                if let Some((id, info)) = parse_app(a) {
                    app.activity.apps.entry(id).or_insert(info);
                }
            }
        }
        "launch" => {
            if let Some(s) = &mut app.activity.starting
                && let Some(id) = v["launch_id"].as_str()
            {
                s.instance = Some(id.to_string());
            }
        }
        "ticket" => {
            if let Some(s) = &mut app.activity.starting {
                s.ticket = Some(v["ticket"].as_str().map(str::to_string).ok_or_else(|| "no ticket".to_string()));
            }
        }
        "auth_info" => auth_info(app, &v),
        "auth_grant" => auth_grant(app, &v),
        "me" => {
            let Some(a) = app.activity.auth.take() else { return };
            let Some(run) = &mut app.activity.run else { return };
            let ok = sid(&v["application"]["id"]) == run.app && sid(&v["user"]["id"]) == app.store.me;
            let out = if ok {
                let mut v = v;
                v["access_token"] = json!(a.token);
                run.sock.authenticated(&a.nonce, v)
            } else {
                run.sock.refused("AUTHENTICATE", &a.nonce, bridge::INVALID_TOKEN, "Token does not match this activity and user")
            };
            run.host.post(&out);
        }
        _ => {}
    }
}

pub fn failed(app: &mut App, key: &str, e: String) {
    let head = key.split(':').next().unwrap_or(key);
    match head {
        "shelf" => {
            if let Some(s) = &mut app.activity.shelf {
                s.failed = true;
            }
        }
        "launch" | "ticket" => {
            app.activity.starting = None;
            app.toast(format!("Couldn't start the activity: {e}"));
        }
        "auth_info" | "auth_grant" | "me" => {
            let Some(a) = app.activity.auth.take() else { return };
            app.activity.consent = None;
            let Some(run) = &mut app.activity.run else { return };
            let out = run.sock.refused(a.cmd, &a.nonce, if head == "me" { bridge::INVALID_TOKEN } else { bridge::OAUTH2_ERROR }, &format!("OAuth2 Error: {e}"));
            run.host.post(&out);
        }
        _ => {}
    }
}

fn oauth_query(a: &Auth, app: Id) -> String {
    let scope = a.scopes.join(" ");
    let id = app.to_string();
    let mut p = vec![("client_id", id.as_str()), ("response_type", "code"), ("scope", scope.as_str())];
    if let Some(s) = &a.state {
        p.push(("state", s));
    }
    query(&p)
}

fn auth_info(app: &mut App, v: &Value) {
    let Some(a) = &app.activity.auth else { return };
    let Some(run) = &app.activity.run else { return };
    if a.quiet && v["authorized"].as_bool() == Some(true) {
        return grant(app);
    }
    let name = v["application"]["name"].as_str().unwrap_or("This activity").to_string();
    let icon = v["application"]["icon"].as_str().map(|h| cdn(format!("app-icons/{}/{h}.png?size=128", run.app)));
    app.activity.consent = Some(Consent { name, icon, scopes: a.scopes.clone() });
}

fn grant(app: &mut App) {
    let (Some(run), Some(a)) = (&app.activity.run, &app.activity.auth) else { return };
    let q = oauth_query(a, run.app);
    let ch = run.channel;
    let g = guild_of(app);
    let kind = app.store.channels.get(&ch).map_or(0, |c| c.kind);
    let body = json!({
        "permissions": "0",
        "authorize": true,
        "location_context": { "guild_id": if g != 0 { g.to_string() } else { "10000".into() }, "channel_id": ch.to_string(), "channel_type": kind },
    });
    app.rest("POST", format!("/oauth2/authorize?{q}"), Some(body), Tag::Activity("auth_grant".into()));
}

fn auth_grant(app: &mut App, v: &Value) {
    let Some(a) = app.activity.auth.take() else { return };
    let Some(run) = &mut app.activity.run else { return };
    let code = v["location"].as_str().and_then(|l| reqwest::Url::parse(l).ok()).and_then(|u| u.query_pairs().find(|(k, _)| k == "code").map(|(_, c)| c.into_owned()));
    let out = match code {
        Some(c) => run.sock.authorized(&a.nonce, &c),
        None => run.sock.refused(a.cmd, &a.nonce, bridge::OAUTH2_ERROR, "OAuth2 Error: Unable to find auth code"),
    };
    run.host.post(&out);
}

pub fn consent_answer(app: &mut App, allow: bool) {
    if app.activity.consent.take().is_none() {
        return;
    }
    if allow {
        return grant(app);
    }
    let Some(a) = app.activity.auth.take() else { return };
    if let Some(run) = &mut app.activity.run {
        let out = run.sock.refused(a.cmd, &a.nonce, bridge::OAUTH2_ERROR, "OAuth2 Error: access_denied: The resource owner or authorization server denied the request");
        run.host.post(&out);
    }
}

fn env(app: &App, run: &Run) -> bridge::Env {
    let chan = app.store.channels.get(&run.channel);
    let voice_states: Vec<Value> = voice_states(app, run.channel);
    let channel = json!({
        "id": run.channel.to_string(),
        "type": chan.map_or(2, |c| c.kind),
        "guild_id": chan.filter(|c| c.guild != 0).map(|c| c.guild.to_string()),
        "name": chan.map(|c| c.name.clone()),
        "topic": chan.and_then(|c| c.topic.clone()),
        "position": chan.map_or(0, |c| c.position),
        "voice_states": voice_states,
        "messages": [],
    });
    bridge::Env {
        app_name: app.activity.apps.get(&run.app).map(|i| i.name.clone()).unwrap_or_default(),
        channel,
        channel_id: run.channel,
        participants: participants(app, run),
        cdn: crate::api::cdn().split("://").nth(1).unwrap_or("cdn.discordapp.com").to_string(),
        api: format!("//{}", crate::api::base().split("://").nth(1).unwrap_or("discord.com/api/v9").trim_end_matches("/v9")),
        locale: if app.settings.locale.is_empty() { "en-US".into() } else { app.settings.locale.clone() },
    }
}

fn participants(app: &App, run: &Run) -> Vec<Value> {
    let guild = app.store.channels.get(&run.channel).map_or(0, |c| c.guild);
    let users = app.store.embedded.get(&run.channel).and_then(|l| l.iter().find(|i| i.id == run.instance)).map(|i| i.users.clone()).unwrap_or_default();
    users
        .into_iter()
        .map(|u| {
            let mut j = user_json(app, u);
            if guild != 0 {
                j["nickname"] = json!(app.store.members.get(&guild).and_then(|m| m.get(&u)).and_then(|m| m.nick.clone()));
            }
            j
        })
        .collect()
}

fn voice_states(app: &App, ch: Id) -> Vec<Value> {
    app.voice
        .members(&app.store, ch)
        .into_iter()
        .map(|u| {
            let m = app.voice.muted.get(&u).copied().unwrap_or_default();
            json!({
                "user": user_json(app, u),
                "nick": app.store.name_in(app.store.channels.get(&ch).map_or(0, |c| c.guild), u),
                "mute": m[1],
                "voice_state": { "mute": m[1], "deaf": m[3], "self_mute": m[0], "self_deaf": m[2], "suppress": false },
                "volume": 100,
                "pan": { "left": 1.0, "right": 1.0 },
            })
        })
        .collect()
}

fn handle(app: &mut App, ctx: &Context, msg: &Value) {
    let Some(run) = app.activity.run.as_mut() else { return };
    match msg["t"].as_str() {
        Some("load") => run.sock = run.sock.fresh(),
        Some("frame_error") => run.failed = true,
        Some("msg") => {
            let origin = msg["origin"].as_str().unwrap_or_default().to_string();
            let Some(run) = app.activity.run.as_ref() else { return };
            let e = env(app, run);
            let acts = app.activity.run.as_mut().map(|r| r.sock.on_message(&origin, &msg["data"], &e)).unwrap_or_default();
            for a in acts {
                act(app, ctx, a);
            }
        }
        _ => {}
    }
}

fn act(app: &mut App, _ctx: &Context, a: bridge::Act) {
    use bridge::Act::*;
    match a {
        Send(v) => {
            if let Some(r) = &app.activity.run {
                r.host.post(&v);
            }
        }
        Close(code, message) => {
            if let Some(r) = &app.activity.run {
                r.host.post(&json!([2, { "code": code, "message": message }]));
            }
        }
        Authorize { nonce, scopes, state, quiet } => {
            let Some(run) = &app.activity.run else { return };
            let a = Auth { cmd: "AUTHORIZE", nonce, scopes, state, quiet, token: None };
            let q = oauth_query(&a, run.app);
            app.activity.auth = Some(a);
            app.rest("GET", format!("/oauth2/authorize?{q}"), None, Tag::Activity("auth_info".into()));
        }
        Authenticate { nonce, token } => {
            app.activity.auth = Some(Auth { cmd: "AUTHENTICATE", nonce, scopes: Vec::new(), state: None, quiet: false, token: Some(token.clone()) });
            app.net.send(crate::net::Cmd::Bearer { path: "/oauth2/@me".into(), token, tag: Tag::Activity("me".into()) });
        }
        Link { nonce, url } => {
            let ok = reqwest::Url::parse(&url).is_ok_and(|u| u.scheme() == "https" || u.scheme() == "http");
            let Some(run) = &app.activity.run else { return };
            if !ok || app.activity.link.is_some() {
                run.host.post(&run.sock.answer("OPEN_EXTERNAL_LINK", &nonce, json!({ "opened": false })));
                return;
            }
            app.activity.link = Some(nonce);
            app.leaving = Some((url, false));
        }
    }
}

pub fn link_done(app: &mut App, opened: bool) {
    let Some((nonce, opened)) = app.activity.linked(opened) else { return };
    if let Some(run) = &app.activity.run {
        run.host.post(&run.sock.answer("OPEN_EXTERNAL_LINK", &nonce, json!({ "opened": opened })));
    }
}

/// Every frame, whatever is on screen: starting and stopping, and the page's messages.
pub fn logic(app: &mut App, ctx: &Context) {
    app.activity.surface = 0;
    if app.activity.run.as_ref().is_some_and(|r| app.voice.chan != Some(r.channel)) {
        leave(app);
    }
    if !app.activity.placed
        && let Some(r) = &app.activity.run
    {
        r.host.place(0, [0, 0, 0, 0], false);
    }
    app.activity.placed = false;
    if !app.store.embedded.is_empty() {
        let unknown: Vec<Id> = app.store.embedded.values().flatten().map(|i| i.app).filter(|a| !app.activity.apps.contains_key(a)).collect();
        fetch_apps(app, unknown);
    }
    if let Some((ch, id)) = app.activity.after_join
        && app.voice.chan == Some(ch)
        && app.voice.state == crate::voice::State::Connected
    {
        app.activity.after_join = None;
        launch(app, id);
    }
    starting(app, ctx);
    let Some(run) = &app.activity.run else { return };
    let msgs = run.host.poll();
    if let Status::Failed = run.host.status() {
        app.toast("The activity couldn't be loaded.");
        leave(app);
        return;
    }
    for m in &msgs {
        handle(app, ctx, m);
    }
    sync(app);
}

fn sync(app: &mut App) {
    let Some(run) = &app.activity.run else { return };
    if !run.sock.wants_view() {
        return;
    }
    let me = app.store.me;
    let view = bridge::View {
        me: user_json(app, me),
        participants: participants(app, run),
        voice: voice_states(app, run.channel),
        speaking: app.voice.members(&app.store, run.channel).into_iter().filter(|u| app.voice.is_speaking(*u)).collect(),
        channel_id: run.channel,
    };
    let out = app.activity.run.as_mut().map(|r| r.sock.sync(&view)).unwrap_or_default();
    if let Some(run) = &app.activity.run {
        for f in out {
            run.host.post(&f);
        }
    }
}

fn starting(app: &mut App, ctx: &Context) {
    let Some(st) = &app.activity.starting else { return };
    if st.at.elapsed() > LAUNCH_WAIT {
        app.activity.starting = None;
        app.toast("The activity didn't start.");
        return;
    }
    let me = app.store.me;
    let seen = app.store.embedded.get(&st.channel).and_then(|l| l.iter().find(|i| i.app == st.app && i.users.contains(&me))).map(|i| i.id.clone());
    let (id, channel, ticketed) = (st.app, st.channel, st.ticket.clone());
    let Some(instance) = seen.or_else(|| st.instance.clone()) else { return };
    match ticketed {
        None => {
            if let Some(s) = &mut app.activity.starting
                && s.instance.is_none()
            {
                s.instance = Some(instance);
            }
            let asked = ctx.data(|d| d.get_temp::<Id>(egui::Id::new("activity_ticket")));
            if asked != Some(id) {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("activity_ticket"), id));
                app.rest("POST", format!("/applications/{id}/proxy-tickets"), Some(json!({ "use_stateless_ticket": true, "channel_id": channel.to_string() })), Tag::Activity("ticket".into()));
            }
        }
        Some(t) => {
            app.activity.starting = None;
            ctx.data_mut(|d| d.remove_temp::<Id>(egui::Id::new("activity_ticket")));
            open(app, ctx, id, channel, instance, t.ok());
        }
    }
}

fn open(app: &mut App, ctx: &Context, id: Id, channel: Id, instance: String, ticket: Option<String>) {
    let base = origin_template().replace("{app}", &id.to_string());
    let frame_id = random_id();
    let guild = app.store.channels.get(&channel).map_or(0, |c| c.guild);
    let mut params = json!({
        "instance_id": instance,
        "location_id": channel.to_string(),
        "launch_id": instance,
        "channel_id": channel.to_string(),
        "frame_id": frame_id,
        "platform": "desktop",
    });
    if guild != 0 {
        params["guild_id"] = json!(guild.to_string());
    }
    if let Some(t) = ticket {
        params["discord_proxy_ticket"] = json!(t);
    }
    let pairs: Vec<(String, String)> = params.as_object().into_iter().flatten().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect();
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let src = format!("{}/?{}", base.trim_end_matches('/'), query(&refs));
    let Some(origin) = origin_of(&src) else {
        return app.toast("The activity's address isn't usable.");
    };
    let parent = crate::winsys::WINDOW.load(std::sync::atomic::Ordering::Relaxed);
    let host = Host::open(ctx, parent, Cfg { src, hello: params, embed: false });
    app.activity.run = Some(Run { app: id, channel, instance, host, sock: bridge::Socket::new(id, frame_id, origin), failed: false });
}

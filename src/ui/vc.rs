//! The message plugins that need more than a branch in chat.rs: their fetches and caches,
//! and the little icons they add next to a name.

use egui::{Color32, CornerRadius, Frame, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::App;
use super::icons::{self, Icon};
use crate::net::Tag;
use crate::plugins::{flag, num, on, text};
use crate::state::{Embed, Id, Message, Status, Store, snowflake_ms};
use crate::theme::*;

pub enum Req {
    Msg(Id, Id),
    User(Id),
    Profile(Id),
    Reactors(Id, Id, String),
    Dearrow(String),
    File(String),
}

#[derive(Default)]
pub struct St {
    pub hidden: HashSet<Id>,
    pub msgs: HashMap<Id, Message>,
    pub reactors: HashMap<String, Vec<Id>>,
    pub dearrow: HashMap<String, Option<(Option<String>, Option<String>)>>,
    pub dearrow_flipped: HashSet<String>,
    pub raw: Option<(String, String)>,
    asked: HashSet<String>,
    queue: Vec<Req>,
    last: Option<Instant>,
    toast: Option<String>,
}

static ST: LazyLock<Mutex<St>> = LazyLock::new(Default::default);
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(crate::api::http);

pub fn st() -> MutexGuard<'static, St> {
    ST.lock().unwrap()
}

pub fn ask(key: String, req: Req) {
    let mut s = st();
    if s.asked.insert(key) {
        s.queue.push(req);
    }
}

pub fn push(req: Req) {
    st().queue.push(req);
}

fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The discord requests go one per 300ms, the others as they come.
pub fn flush(app: &mut App, ctx: &egui::Context) {
    let (reqs, more, toast) = {
        let mut s = st();
        let mut out = Vec::new();
        let mut i = 0;
        while i < s.queue.len() {
            if matches!(s.queue[i], Req::Dearrow(_) | Req::File(_)) {
                out.push(s.queue.remove(i));
            } else {
                i += 1;
            }
        }
        if !s.queue.is_empty() && s.last.is_none_or(|t| t.elapsed() > Duration::from_millis(300)) {
            s.last = Some(Instant::now());
            out.push(s.queue.remove(0));
        }
        (out, !s.queue.is_empty(), s.toast.take())
    };
    if more {
        ctx.request_repaint_after(Duration::from_millis(300));
    }
    if let Some(t) = toast {
        app.toast(t);
    }
    for r in reqs {
        match r {
            Req::Msg(c, m) => app.rest("GET", format!("/channels/{c}/messages?around={m}&limit=1"), None, Tag::Vc(format!("msg:{m}"))),
            Req::User(u) => app.rest("GET", format!("/users/{u}"), None, Tag::Vc("user".into())),
            Req::Profile(u) => app.rest("GET", format!("/users/{u}/profile?with_mutual_guilds=false"), None, Tag::Profile(u)),
            Req::Reactors(c, m, emoji) => {
                app.rest("GET", format!("/channels/{c}/messages/{m}/reactions/{}?limit=4", enc(&emoji)), None, Tag::Vc(format!("react:{m}:{emoji}")))
            }
            Req::Dearrow(id) => {
                let ctx = ctx.clone();
                app.net.rt.spawn(async move {
                    let found = dearrow_fetch(&id).await;
                    st().dearrow.insert(id, found);
                    ctx.request_repaint();
                });
            }
            Req::File(url) => {
                let ctx = ctx.clone();
                app.net.rt.spawn(async move {
                    let msg = match HTTP.get(&url).send().await.and_then(|r| r.error_for_status()) {
                        Ok(r) => match r.bytes().await {
                            Ok(b) if b.len() <= 4_000_000 => {
                                ctx.copy_text(String::from_utf8_lossy(&b).into_owned());
                                "Copied file contents".to_string()
                            }
                            Ok(_) => "File is too large to copy".to_string(),
                            Err(e) => e.to_string(),
                        },
                        Err(e) => e.to_string(),
                    };
                    st().toast = Some(msg);
                    ctx.request_repaint();
                });
            }
        }
    }
}

async fn dearrow_fetch(id: &str) -> Option<(Option<String>, Option<String>)> {
    let v: Value = HTTP.get(format!("https://sponsor.ajay.app/api/branding?videoID={id}")).send().await.ok()?.json().await.ok()?;
    let title = v["titles"][0]["title"].as_str().filter(|_| v["titles"][0]["votes"].as_i64().unwrap_or(-1) >= 0).map(|t| t.replace(" >", " ").trim_start_matches('>').to_string());
    let t = &v["thumbnails"][0];
    let thumb = t["timestamp"]
        .as_f64()
        .filter(|_| t["votes"].as_i64().unwrap_or(-1) >= 0 && t["original"].as_bool() != Some(true))
        .map(|ts| format!("https://dearrow-thumb.ajay.app/api/v1/getThumbnail?videoID={id}&time={ts}"));
    (title.is_some() || thumb.is_some()).then_some((title, thumb))
}

pub fn loaded(app: &mut App, key: &str, v: Value) {
    let mut p = key.splitn(3, ':');
    match (p.next(), p.next(), p.next()) {
        (Some("user"), ..) => {
            app.store.apply("USER_UPDATE", &v);
        }
        (Some("msg"), ..) => {
            if let Some(m) = v.as_array().and_then(|a| a.first()) {
                let m = app.store.parse_message(m);
                st().msgs.insert(m.id, m);
            }
        }
        (Some("react"), Some(m), Some(emoji)) => {
            let ids = v
                .as_array()
                .into_iter()
                .flatten()
                .map(|u| {
                    app.store.apply("USER_UPDATE", u);
                    crate::state::sid(&u["id"])
                })
                .collect();
            st().reactors.insert(format!("{m}:{emoji}"), ids);
        }
        _ => {}
    }
}

pub fn ask_user(id: Id) {
    if on("ValidUser") {
        ask(format!("user:{id}"), Req::User(id));
    }
}

fn xxh64(b: &[u8]) -> u64 {
    const P1: u64 = 11400714785074694791;
    const P2: u64 = 14029467366897019727;
    const P3: u64 = 1609587929392839161;
    const P4: u64 = 9650029242287828579;
    const P5: u64 = 2870177450012600261;
    let round = |acc: u64, v: u64| acc.wrapping_add(v.wrapping_mul(P2)).rotate_left(31).wrapping_mul(P1);
    let merge = |h: u64, v: u64| (h ^ round(0, v)).wrapping_mul(P1).wrapping_add(P4);
    let r64 = |s: &[u8]| u64::from_le_bytes(s[..8].try_into().unwrap());
    let mut p = b;
    let mut h;
    if b.len() >= 32 {
        let mut v = [P1.wrapping_add(P2), P2, 0, 0u64.wrapping_sub(P1)];
        while p.len() >= 32 {
            for (i, x) in v.iter_mut().enumerate() {
                *x = round(*x, r64(&p[i * 8..]));
            }
            p = &p[32..];
        }
        h = v[0].rotate_left(1).wrapping_add(v[1].rotate_left(7)).wrapping_add(v[2].rotate_left(12)).wrapping_add(v[3].rotate_left(18));
        for x in v {
            h = merge(h, x);
        }
    } else {
        h = P5;
    }
    h = h.wrapping_add(b.len() as u64);
    while p.len() >= 8 {
        h = (h ^ round(0, r64(p))).rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
        p = &p[8..];
    }
    if p.len() >= 4 {
        h = (h ^ (u32::from_le_bytes(p[..4].try_into().unwrap()) as u64).wrapping_mul(P1)).rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
        p = &p[4..];
    }
    for &c in p {
        h = (h ^ (c as u64).wrapping_mul(P5)).rotate_left(11).wrapping_mul(P1);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(P2);
    h ^= h >> 29;
    h = h.wrapping_mul(P3);
    h ^ (h >> 32)
}

/// IrcColors: hsl(hash of the id mod 360, 100%, lightness).
pub fn irc_color(id: Id) -> Color32 {
    let hue = (xxh64(id.to_string().as_bytes()) % 360) as f32;
    let l = (num("IrcColors", "lightness") as f32 / 100.0).clamp(0.0, 1.0);
    let c = 1.0 - (2.0 * l - 1.0).abs();
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (hue / 60.0) as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let ch = |v: f32| ((v + m) * 255.0).round() as u8;
    Color32::from_rgb(ch(r), ch(g), ch(b))
}

pub fn irc(store: &Store, guild: Id, who: Id) -> Option<Color32> {
    if !on("IrcColors") || (guild != 0 && flag("IrcColors", "applyColorOnlyInDms")) {
        return None;
    }
    if flag("IrcColors", "applyColorOnlyToUsersWithoutColor") && store.role_color(guild, who).is_some_and(|c| c != 0) {
        return None;
    }
    Some(irc_color(who))
}

/// ShowMeYourName: the name to draw and the muted one after it.
pub fn smyn(store: &Store, guild: Id, who: Id, replies: bool) -> (String, Option<String>) {
    let nick = store.name_in(guild, who);
    if !on("ShowMeYourName") || (replies && !flag("ShowMeYourName", "inReplies")) {
        return (nick, None);
    }
    let Some(u) = store.user(who) else { return (nick, None) };
    let user = if flag("ShowMeYourName", "displayNames") { u.display().to_string() } else { u.username.clone() };
    if user.eq_ignore_ascii_case(&nick) {
        return (nick, None);
    }
    match text("ShowMeYourName", "mode").as_str() {
        "nick-user" => (nick, Some(user)),
        "user" => (user, None),
        _ => (user, Some(nick)),
    }
}

/// RoleColorEverywhere's coloured message text.
pub fn role_text(store: &Store, guild: Id, who: Id) -> Option<Color32> {
    if !flag("RoleColorEverywhere", "colorChatMessages") {
        return None;
    }
    let c = role_color(store.role_color(guild, who).filter(|c| *c != 0)?);
    Some(pal().text_normal.lerp_to_gamma(c, (num("RoleColorEverywhere", "messageSaturation") / 100.0).clamp(0.0, 1.0) as f32))
}

/// BlurNSFW: the same picture at a few pixels wide, stretched.
pub fn blurred(url: &str) -> Option<String> {
    if !on("BlurNSFW") || url.contains("/stickers/") {
        return None;
    }
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let keep: Vec<&str> = query.split('&').filter(|q| !q.is_empty() && !q.starts_with("width=") && !q.starts_with("height=")).collect();
    let w = (120.0 / num("BlurNSFW", "blurAmount").max(1.0)).round().clamp(4.0, 40.0) as u32;
    Some(format!("{base}?{}{}width={w}", keep.join("&"), if keep.is_empty() { "" } else { "&" }))
}

pub fn is_hidden(id: Id) -> bool {
    on("HideMedia") && st().hidden.contains(&id)
}

pub fn toggle_hidden(id: Id) {
    let mut s = st();
    if !s.hidden.remove(&id) {
        s.hidden.insert(id);
    }
}

pub fn unindent(code: &str) -> String {
    let min = code.lines().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start_matches(' ').len()).min().unwrap_or(0);
    if min == 0 {
        return code.to_string();
    }
    code.split('\n').map(|l| l.strip_prefix(&" ".repeat(min)).unwrap_or(l)).collect::<Vec<_>>().join("\n")
}

pub fn youtube_id(e: &Embed) -> Option<String> {
    if e.provider.as_deref() != Some("YouTube") {
        return None;
    }
    let from = |url: &str, sep: &str| url.split(sep).nth(1).and_then(|r| r.get(..11)).map(str::to_string);
    e.video.as_ref().and_then(|v| from(&v.url, "youtube.com/embed/")).or_else(|| e.url.as_deref().and_then(|u| from(u, "watch?v=").or_else(|| from(u, "youtu.be/"))))
}

/// Dearrow: the embed with its title and thumbnail swapped, while the toggle is on.
pub fn dearrow(e: &Embed) -> Option<(Embed, String)> {
    if !on("Dearrow") {
        return None;
    }
    let id = youtube_id(e)?;
    let found = st().dearrow.get(&id).cloned();
    let Some(Some((title, thumb))) = found else {
        if found.is_none() {
            ask(format!("dearrow:{id}"), Req::Dearrow(id));
        }
        return None;
    };
    let mut x = e.clone();
    let on_now = flag("Dearrow", "dearrowByDefault") != st().dearrow_flipped.contains(&id);
    if on_now {
        let which = text("Dearrow", "replaceElements");
        if let Some(t) = title.filter(|_| which != "2") {
            x.title = Some(t);
        }
        if let Some(t) = thumb.filter(|_| which != "1") {
            for m in [&mut x.thumb, &mut x.image].into_iter().flatten() {
                m.url = t.clone();
            }
        }
    }
    Some((x, id))
}

pub fn dearrow_button(ui: &mut Ui, rect: Rect, id: &str) {
    let b = Rect::from_min_size(pos2(rect.right() - 26.0, rect.top() + 6.0), vec2(20.0, 20.0));
    let r = ui.interact(b, egui::Id::new(("dearrow", id)), Sense::click());
    let on_now = flag("Dearrow", "dearrowByDefault") != st().dearrow_flipped.contains(id);
    let a = if on_now { 1.0 } else { 0.5 };
    let p = ui.painter();
    p.circle_filled(b.center(), 10.0, Color32::from_rgb(0x12, 0x13, 0xbd).gamma_multiply(a));
    p.circle_filled(b.center(), 7.5, Color32::from_rgb(0x88, 0xc9, 0xf9).gamma_multiply(a));
    p.circle_filled(b.center(), 4.0, Color32::from_rgb(0x0a, 0x62, 0xa5).gamma_multiply(a));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.on_hover_text(if on_now { "This embed has been dearrowed, click to restore" } else { "Click to dearrow" }).clicked() {
        let mut s = st();
        if !s.dearrow_flipped.remove(id) {
            s.dearrow_flipped.insert(id.to_string());
        }
    }
}

pub fn is_text_file(a: &crate::state::Attachment) -> bool {
    let n = a.filename.to_ascii_lowercase();
    a.ctype.starts_with("text/") || ["txt", "md", "json", "log", "rs", "py", "js", "ts", "toml", "yaml", "yml", "csv", "ini", "cfg", "xml", "html", "css", "c", "cpp", "h", "java", "go", "sh", "bat"].iter().any(|e| n.ends_with(&format!(".{e}")))
}

/// A small round button on a card; true when clicked.
pub fn card_button(ui: &mut Ui, at: Rect, id: egui::Id, icon: Icon, tip: &str) -> bool {
    let r = ui.interact(at, id, Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(at, CornerRadius::same(4), pal().control_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::draw(ui.painter(), icon, Rect::from_center_size(at.center(), vec2(18.0, 18.0)), pal().header_secondary);
    r.on_hover_text(tip).clicked()
}

fn delta_text(ms: i64, millis: bool) -> String {
    let parts = [(ms / 86_400_000, "day"), (ms / 3_600_000 % 24, "hour"), (ms / 60_000 % 60, "minute"), (ms / 1000 % 60, "second"), (ms % 1000, "millisecond")];
    let words: Vec<String> = parts.iter().filter(|(n, u)| *n > 0 && (millis || *u != "millisecond")).map(|(n, u)| format!("{n} {u}{}", if *n > 1 { "s" } else { "" })).collect();
    match words.len() {
        0 => "0 seconds".into(),
        1 => words[0].clone(),
        n => format!("{} and {}", words[..n - 1].join(" "), words[n - 1]),
    }
}

/// MessageLatency: the three bars' colours and what they say, if the message was slow enough.
fn latency(store: &Store, m: &Message) -> Option<([Color32; 3], String)> {
    let nonce = m.nonce.as_ref()?.parse::<u64>().ok()?;
    if store.user(m.author).is_some_and(|u| u.bot) || (flag("MessageLatency", "ignoreSelf") && m.author == store.me) {
        return None;
    }
    let millis = flag("MessageLatency", "showMillis");
    let mut delta = m.ts() - snowflake_ms(nonce);
    if !millis {
        delta = (delta as f64 / 1000.0).round() as i64 * 1000;
    }
    const KOTLIN: i64 = 1_471_228_928;
    let kotlin = -delta >= KOTLIN - 86_400_000;
    if kotlin {
        delta += KOTLIN;
    }
    let abs = delta.abs();
    let ahead = abs != delta;
    let limit = (num("MessageLatency", "latency") * 1000.0) as i64;
    if abs < limit && !kotlin {
        return None;
    }
    let (m_, d, w, g) = (pal().text_muted, RED, YELLOW, GREEN);
    let fill = if kotlin {
        [g, g, m_]
    } else if delta >= 120_000 || ahead {
        [m_; 3]
    } else if delta >= limit * 2 {
        [d, m_, m_]
    } else {
        [w, w, m_]
    };
    let when = (abs >= limit).then(|| delta_text(abs, millis));
    let tip = match when {
        None => "User is suspected to be on an old Discord Android client".to_string(),
        Some(t) => format!("{}{}", if ahead { format!("This user's clock is {t} ahead.") } else { format!("This message was sent with a delay of {t}.") }, if kotlin { " User is suspected to be on an old Discord Android client." } else { "" }),
    };
    Some((fill, tip))
}

fn platform_icon(p: &egui::Painter, kind: usize, c: Rect, col: Color32) {
    let s = Stroke::new(1.5, col);
    match kind {
        0 => {
            p.rect_stroke(Rect::from_center_size(c.center() - vec2(0.0, 1.0), vec2(12.0, 8.0)), CornerRadius::same(1), s, egui::StrokeKind::Inside);
            p.hline(c.center().x - 3.0..=c.center().x + 3.0, c.center().y + 5.0, s);
        }
        1 => {
            p.rect_stroke(Rect::from_center_size(c.center(), vec2(7.0, 12.0)), CornerRadius::same(2), s, egui::StrokeKind::Inside);
            p.circle_filled(c.center() + vec2(0.0, 4.0), 0.9, col);
        }
        _ => {
            p.circle_stroke(c.center(), 6.0, s);
            p.vline(c.center().x, c.center().y - 6.0..=c.center().y + 6.0, Stroke::new(1.0, col));
            p.hline(c.center().x - 6.0..=c.center().x + 6.0, c.center().y, Stroke::new(1.0, col));
        }
    }
}

fn pronouns(profiles: &HashMap<Id, Value>, who: Id) -> Option<String> {
    let v = profiles.get(&who)?;
    let raw = [&v["guild_member_profile"]["pronouns"], &v["user_profile"]["pronouns"]].into_iter().find_map(|p| p.as_str().filter(|s| !s.is_empty()))?;
    let lower = raw.to_lowercase();
    if text("UserMessagesPronouns", "pronounsFormat") != "CAPITALIZED" {
        return Some(lower);
    }
    let mut prev = ' ';
    Some(lower.chars().map(|c| { let up = if prev.is_alphanumeric() { c } else { c.to_ascii_uppercase() }; prev = c; up }).collect())
}

/// What the plugins put after a name in the author line; returns where the timestamp starts.
pub fn badges(ui: &mut Ui, store: &Store, guild: Id, profiles: &HashMap<Id, Value>, m: &Message, mut x: f32, cy: f32) -> f32 {
    let mut slot = |ui: &mut Ui, kind: &str, w: f32, tip: &str, draw: &dyn Fn(&egui::Painter, Rect)| {
        let r = Rect::from_min_size(pos2(x, cy - 8.0), vec2(w, 16.0));
        draw(ui.painter(), r);
        ui.interact(r, ui.id().with(("vc", kind, m.id)), Sense::hover()).on_hover_text(tip);
        x += w + 4.0;
    };
    if on("ForceOwnerCrown") && store.guilds.get(&guild).is_some_and(|g| g.owner == m.author) {
        slot(ui, "crown", 16.0, "Server Owner", &|p, r| icons::draw(p, Icon::Crown, r, Color32::from_rgb(0xf0, 0xb2, 0x32)));
    }
    if flag("PlatformIndicators", "messages")
        && let Some(cs) = store.client_status.get(&m.author)
    {
        for (i, (s, name)) in cs.iter().zip(["Desktop", "Mobile", "Web"]).enumerate() {
            let Some(s) = s.filter(|s| *s != Status::Offline) else { continue };
            let col = if i == 1 && !flag("PlatformIndicators", "colorMobileIndicator") { pal().text_muted } else { status_color(s) };
            slot(ui, &format!("plat{i}"), 16.0, name, &|p, r| platform_icon(p, i, r, col));
        }
    }
    if flag("UserVoiceShow", "showInMessages")
        && let Some(ch) = store.guilds.get(&guild).and_then(|g| g.voice.get(&m.author))
    {
        let name = store.channels.get(ch).map_or("a voice channel".to_string(), |c| c.name.clone());
        slot(ui, "voice", 16.0, &format!("In {name}"), &|p, r| icons::draw(p, Icon::Speaker, r, pal().header_secondary));
    }
    if on("UserMessagesPronouns") && (m.author != store.me || flag("UserMessagesPronouns", "showSelf")) {
        match pronouns(profiles, m.author) {
            Some(t) => {
                let g = ui.painter().layout_no_wrap(format!("• {t}"), medium(12.0), pal().chat_muted);
                let w = g.size().x;
                slot(ui, "pronouns", w, &t, &|p, r| {
                    p.galley(pos2(r.left(), cy - g.size().y / 2.0), g.clone(), pal().chat_muted);
                });
            }
            None if !profiles.contains_key(&m.author) => ask(format!("profile:{}", m.author), Req::Profile(m.author)),
            None => {}
        }
    }
    if on("MessageLatency")
        && let Some((fill, tip)) = latency(store, m)
    {
        slot(ui, "latency", 12.0, &tip, &|p, r| {
            for (i, c) in fill.iter().enumerate() {
                let h = 3.0 + 3.0 * i as f32;
                p.rect_filled(Rect::from_min_max(pos2(r.left() + i as f32 * 4.0, r.center().y + 4.0 - h), pos2(r.left() + i as f32 * 4.0 + 3.0, r.center().y + 4.0)), CornerRadius::same(1), *c);
            }
        });
    }
    x
}

/// The ids of those who reacted, as far as fetched.
pub fn reactors(ch: Id, msg: Id, emoji: &crate::state::ReactEmoji, count: u32) -> Vec<Id> {
    let e = emoji.api_name();
    let s = st();
    if let Some(v) = s.reactors.get(&format!("{msg}:{e}")).filter(|v| v.len() == (count as usize).min(4)) {
        return v.iter().take(3).copied().collect();
    }
    drop(s);
    ask(format!("react:{msg}:{e}:{count}"), Req::Reactors(ch, msg, e));
    Vec::new()
}

pub fn quick_fill(list: &mut Vec<crate::state::ReactEmoji>, frequent: &[crate::state::ReactEmoji]) {
    if !on("MoreQuickReactions") {
        return;
    }
    let n = num("MoreQuickReactions", "reactionCount").clamp(0.0, 42.0) as usize;
    list.truncate(n);
    let fallback = ["✅", "❤️", "🔥", "😂", "👍", "😭", "💀", "👀", "🙏", "😊", "🎉", "😍", "🤔", "😢", "😡", "👎", "💯", "🥺", "😎", "🤣", "✨", "🙌", "😅", "💔", "🤝", "👏", "😉", "🥰", "😁", "😱"];
    for e in frequent.iter().cloned().chain(fallback.iter().map(|n| crate::state::ReactEmoji { id: 0, name: n.to_string(), animated: false })) {
        if list.len() >= n {
            break;
        }
        if !list.contains(&e) {
            list.push(e);
        }
    }
}

pub fn raw_modal(app: &mut App, ctx: &egui::Context) {
    let Some((title, body)) = st().raw.clone() else { return };
    let mut close = false;
    let modal = egui::Modal::new("view_raw".into())
        .frame(Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, pal().border_subtle)).inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() * 0.7).clamp(320.0, 760.0));
            ui.horizontal(|ui| {
                ui.label(RichText::new(&title).font(semi(20.0)).color(pal().header_primary));
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= super::icon_button(ui, Icon::Close, 24.0, "Close").clicked();
                });
            });
            ui.add_space(12.0);
            Frame::new().fill(pal().bg_code).corner_radius(4).inner_margin(8).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.6).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&body).font(mono(13.0)).color(pal().text_normal)).selectable(true));
                });
            });
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if super::button(ui, "Copy", BLURPLE, 120.0).clicked() {
                    ctx.copy_text(body.clone());
                    app.toast("Copied");
                }
                close |= super::grey_button(ui, "Close", 120.0).clicked();
            });
        });
    if close || modal.should_close() {
        st().raw = None;
    }
}

pub fn show_raw(m: &Message) {
    let body = serde_json::to_string_pretty(m).unwrap_or_default();
    st().raw = Some((format!("Message {}", m.id), body));
}

/// ImageZoom: while the button is down on the picture, a lens magnifies under the pointer.
pub fn lens(ui: &Ui, rect: Rect, url: &str) {
    let id = egui::Id::new("viewer_lens");
    let (mut zoom, mut size): (f32, f32) = ui.data(|d| d.get_temp(id)).unwrap_or((2.0, 150.0));
    let Some(p) = ui.input(|i| i.pointer.hover_pos().filter(|p| rect.contains(*p) && i.pointer.primary_down())) else { return };
    let (shift, scroll) = ui.input(|i| (i.modifiers.shift, i.smooth_scroll_delta.y + i.smooth_scroll_delta.x));
    let dir = if flag("ImageZoom", "invertScroll") { 1.0 } else { -1.0 };
    if shift {
        size = (size + scroll * dir * 0.5).clamp(50.0, 500.0);
    } else {
        zoom = (zoom + scroll * dir * 0.005).clamp(1.0, 10.0);
    }
    ui.data_mut(|d| d.insert_temp(id, (zoom, size)));
    let square = flag("ImageZoom", "square");
    let at = Rect::from_center_size(p, vec2(size, size));
    let f = (p - rect.min) / rect.size();
    let uv = Rect::from_center_size(pos2(f.x, f.y), vec2(size / zoom / rect.width(), size / zoom / rect.height()));
    let round = if square { CornerRadius::ZERO } else { CornerRadius::same((size / 2.0).min(255.0) as u8) };
    let mut img = egui::Image::from_uri(crate::gif::uri(ui.ctx(), url, true)).uv(uv).corner_radius(round).show_loading_spinner(false);
    if flag("ImageZoom", "nearestNeighbour") {
        img = img.texture_options(egui::TextureOptions::NEAREST);
    }
    ui.painter().rect_filled(at, round, Color32::BLACK);
    img.paint_at(ui, at);
    ui.painter().rect_stroke(at, round, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Outside);
}

#[cfg(test)]
mod tests {
    use super::xxh64;

    #[test]
    fn xxh64_vectors() {
        assert_eq!(xxh64(b""), 0xEF46DB3751D8E999);
        assert_eq!(xxh64(b"a"), 0xD24EC4F1A98C6E5B);
        assert_eq!(xxh64(b"Nobody inspects the spammish repetition"), 0xFBCEA83C8A378BF1);
    }
}

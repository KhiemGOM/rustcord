//! Vencord's send-side plugins: the rewrites a message goes through on its way out, auto
//! translation, reply pings and the quick reply keys, and the state the composer's plugin
//! buttons share. Anything that needs the web runs on the net runtime and answers through `INBOX`.

use egui::{Key, Modifiers};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use super::App;
use crate::net::Cmd;
use crate::plugins;
use crate::state::{BLOCKED, Id};

#[derive(Default)]
pub struct State {
    pub silent: bool,
    pub sticker: Option<(Id, String, u8)>,
    pub preview: bool,
    pub stamp: Option<Stamp>,
    pub ctx: Option<egui::Context>,
    pub cloning: Option<super::expr::Cloning>,
    held: HashMap<String, Cmd>,
    last_ch: Id,
}

pub struct Stamp {
    pub date: String,
    pub time: String,
    pub fmt: usize,
}

pub enum Task {
    Draft(Id),
    Send(String),
    Message(Id),
}

pub enum Done {
    Translated(Task, Result<String, String>),
    File(Result<PathBuf, String>),
    Say(Id, Result<String, String>),
    Toast(String),
}

static INBOX: Mutex<Vec<Done>> = Mutex::new(Vec::new());

pub fn spawn(app: &App, fut: impl Future<Output = Done> + Send + 'static) {
    let ctx = app.plug.ctx.clone();
    app.net.rt.spawn(async move {
        let d = fut.await;
        INBOX.lock().unwrap().push(d);
        if let Some(c) = ctx {
            c.request_repaint();
        }
    });
}

pub fn readonly() -> bool {
    std::env::var_os("FASTCORD_READONLY").is_some()
}

pub fn api(app: &App) -> Option<crate::api::Api> {
    app.token.clone().filter(|_| !readonly()).map(|t| crate::api::Api::new(crate::api::http(), t))
}

pub fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn tick(app: &mut App, ctx: &egui::Context) {
    app.plug.ctx = Some(ctx.clone());
    let cur = app.cur().unwrap_or(0);
    if app.plug.last_ch != cur {
        app.plug.last_ch = cur;
        if !plugins::flag("SilentMessageToggle", "persistState") {
            app.plug.silent = false;
        }
    }
    let done = std::mem::take(&mut *INBOX.lock().unwrap());
    for d in done {
        match d {
            Done::Toast(s) => app.toast(s),
            Done::File(Ok(path)) => {
                app.files.push(super::composer::Upload::new(path));
                app.focus_composer = true;
            }
            Done::File(Err(e)) => app.toast(e),
            Done::Say(ch, Ok(text)) => {
                app.drafts.insert(ch, text);
                app.post(ch, false);
            }
            Done::Say(_, Err(e)) => app.toast(e),
            Done::Translated(Task::Draft(ch), res) => match res {
                Ok(t) => {
                    app.drafts.insert(ch, t);
                    app.caret_end = true;
                }
                Err(e) => app.toast(e),
            },
            Done::Translated(Task::Message(id), res) => {
                let t = res.unwrap_or_else(|e| e);
                ctx.data_mut(|d| d.insert_temp(egui::Id::new(("translation", id)), t));
                app.heights.clear();
            }
            Done::Translated(Task::Send(nonce), res) => {
                let Some(Cmd::Send { channel, mut content, nonce, reply, files, stickers, tts, silent }) = app.plug.held.remove(&nonce) else { continue };
                match res {
                    Ok(t) => {
                        if let Some(m) = app.store.msgs.get_mut(&channel).and_then(|e| e.list.iter_mut().find(|m| m.nonce.as_deref() == Some(&nonce))) {
                            m.content = t.clone();
                        }
                        content = t;
                    }
                    Err(e) => app.toast(e),
                }
                app.net.send(Cmd::Send { channel, content, nonce, reply, files, stickers, tts, silent });
            }
        }
    }
}

/// Every send goes through here so auto translate can hold one back until its translation lands.
pub fn dispatch(app: &mut App, cmd: Cmd) {
    if plugins::flag("Translate", "autoTranslate")
        && let Cmd::Send { content, nonce, .. } = &cmd
        && !content.trim().is_empty()
    {
        let (text, nonce) = (content.clone(), nonce.clone());
        app.plug.held.insert(nonce.clone(), cmd);
        translate(app, text, "sent", Task::Send(nonce));
        return;
    }
    app.net.send(cmd);
}

pub fn translate(app: &App, text: String, kind: &str, task: Task) {
    let (sl, tl) = (plugins::text("Translate", &format!("{kind}Input")), plugins::text("Translate", &format!("{kind}Output")));
    let url = format!("https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t&dj=1&q={}", enc(&sl), enc(&tl), enc(&text));
    spawn(app, async move {
        let res = async {
            let v: Value = crate::api::http().get(url).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
            let out: String = v["sentences"].as_array().into_iter().flatten().filter_map(|s| s["trans"].as_str()).collect();
            if out.is_empty() { Err("Couldn't translate that".to_string()) } else { Ok(out) }
        };
        Done::Translated(task, res.await)
    });
}

/// The rewrites a message goes through, in the order Vencord's listeners run them.
pub fn rewrite(app: &App, guild: Id, text: String) -> String {
    let mut t = text;
    if plugins::on("TextReplace") {
        t = text_replace(t);
    }
    if plugins::flag("SendTimestamps", "replaceMessageContents") {
        t = timestamps(&t);
    }
    if plugins::on("ClearURLs") {
        t = clear_urls(&t);
    }
    if plugins::flag("FakeNitro", "enableEmojiBypass") {
        t = fake_emojis(app, guild, &t);
    }
    t
}

fn text_replace(mut text: String) -> String {
    let rules = plugins::get("TextReplace", "stringRules");
    for r in rules.as_array().into_iter().flatten() {
        let (find, with, only) = (r["find"].as_str().unwrap_or(""), r["replace"].as_str().unwrap_or(""), r["onlyIfIncludes"].as_str().unwrap_or(""));
        if find.is_empty() || (!only.is_empty() && !text.contains(only)) {
            continue;
        }
        text = text.replace(find, &with.replace("\\n", "\n"));
    }
    text
}

/// `` `3:51 PM` `` becomes a short time timestamp for the next time that happens.
fn timestamps(text: &str) -> String {
    use chrono::{Duration, Local, TimeZone};
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('`') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let parsed = after.find('`').and_then(|j| {
            let body = &after[..j];
            let (clock, ampm) = match body.len().checked_sub(2).map(|k| body.split_at(k)) {
                Some((c, ap)) if ap.eq_ignore_ascii_case("am") || ap.eq_ignore_ascii_case("pm") => (c.trim_end(), Some(ap.eq_ignore_ascii_case("pm"))),
                _ => (body, None),
            };
            let (h, m) = clock.split_once(':')?;
            if !(1..=2).contains(&h.len()) || m.len() != 2 || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit()) {
                return None;
            }
            let (mut h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
            match ampm {
                Some(pm) if (1..=12).contains(&h) => h = h % 12 + if pm { 12 } else { 0 },
                Some(_) => return None,
                None => {}
            }
            let today = Local::now().date_naive();
            let at = if h == 24 && m == 0 { (today + Duration::days(1)).and_hms_opt(0, 0, 0)? } else { today.and_hms_opt(h, m, 0)? };
            let mut ts = Local.from_local_datetime(&at).single()?.timestamp();
            if ts < Local::now().timestamp() {
                ts += 86400;
            }
            Some((format!("<t:{ts}:t>"), j))
        });
        match parsed {
            Some((stamp, j)) => {
                out.push_str(&stamp);
                rest = &after[j + 1..];
            }
            None => {
                out.push('`');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

const TRACKING: &[&str] = &[
    "utm_*", "ga_*", "fbclid", "gclid", "gclsrc", "dclid", "gbraid", "wbraid", "msclkid", "yclid", "twclid", "igshid", "igsh", "mc_cid", "mc_eid", "_hsenc", "_hsmi",
    "mkt_tok", "oly_enc_id", "oly_anon_id", "vero_id", "vero_conv", "wickedid", "ocid", "_openstat", "s_cid", "ref_src", "ref_url", "__s", "rb_clickid", "spm", "scm",
];

const SITES: &[(&str, &[&str])] = &[
    ("amazon.", &["tag", "ref", "ref_", "pf_rd_*", "pd_rd_*", "_encoding", "psc", "linkcode", "linkid", "ascsubtag", "creative", "creativeasin", "camp", "th", "qid", "sr", "sprefix", "crid", "content-id"]),
    ("youtube.com", &["si", "feature", "pp", "kw"]),
    ("youtu.be", &["si", "feature", "pp"]),
    ("twitter.com", &["s", "t", "src"]),
    ("x.com", &["s", "t", "src"]),
    ("tiktok.com", &["_t", "_r", "is_from_webapp", "sender_device", "sender_web_id", "share_*", "source", "u_code", "tt_from", "timestamp", "user_id", "sec_user_id", "social_share_type", "checksum", "refer", "enter_*"]),
    ("reddit.com", &["ref", "ref_source", "share_id", "rdt", "correlation_id", "$deep_link"]),
    ("spotify.com", &["si", "nd", "dlsi", "go", "context"]),
    ("google.", &["ei", "ved", "uact", "gs_lcp", "gs_lp", "sclient", "sca_esv", "sxsrf", "gws_rd", "oq", "aqs"]),
    ("facebook.com", &["__cft__*", "__tn__", "refsrc", "ref", "hc_ref", "fref", "eid", "mibextid", "sfnsn"]),
    ("instagram.com", &["igshid", "igsh"]),
    ("linkedin.com", &["trk", "trackingid", "lipi", "refid", "originalsubdomain", "midtoken", "midsig"]),
    ("ebay.", &["_trkparms", "_trksid", "hash", "mkcid", "mkevt", "mkrid", "siteid", "campid", "customid", "toolid", "var"]),
    ("aliexpress.", &["algo_*", "btsid", "ws_ab_test", "gps-id", "aff_*", "sk", "pvid", "pdp_*"]),
    ("bilibili.com", &["spm_id_from", "vd_source", "from_spmid", "share_*", "buvid", "unique_k", "from_source", "bbid"]),
    ("steampowered.com", &["snr", "curator_clanid"]),
    ("medium.com", &["source", "sk"]),
    ("twitch.tv", &["tt_content", "tt_medium", "sr"]),
];

fn glob(pat: &str, s: &str) -> bool {
    match pat.strip_suffix('*') {
        Some(p) => s.starts_with(p),
        None => s == pat,
    }
}

fn clean_url(url: &str) -> String {
    let (rest, frag) = url.find('#').map_or((url, ""), |i| url.split_at(i));
    let Some((base, query)) = rest.split_once('?') else { return url.to_string() };
    let host = base.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("").to_lowercase();
    let site = SITES.iter().filter(|(h, _)| host.contains(h)).flat_map(|(_, p)| p.iter().copied());
    let drop: Vec<&str> = TRACKING.iter().copied().chain(site).collect();
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| {
            let key = kv.split('=').next().unwrap_or("").to_lowercase();
            !drop.iter().any(|p| glob(p, &key))
        })
        .collect();
    let q = if kept.is_empty() { String::new() } else { format!("?{}", kept.join("&")) };
    format!("{base}{q}{frag}")
}

fn clear_urls(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("http") {
        let tail = &rest[i..];
        if !(tail.starts_with("http://") || tail.starts_with("https://")) {
            out.push_str(&rest[..i + 4]);
            rest = &rest[i + 4..];
            continue;
        }
        let end = tail.find(|c: char| c.is_whitespace() || c == '<').unwrap_or(tail.len());
        let url = tail[..end].trim_end_matches(['.', ',', ':', ';', '"', '\'', '>', ')', '|', ']']);
        out.push_str(&rest[..i]);
        out.push_str(&clean_url(url));
        rest = &tail[url.len()..];
    }
    out.push_str(rest);
    out
}

fn link(name: &str, url: &str) -> String {
    if plugins::flag("FakeNitro", "useHyperLinks") {
        format!("[{}]({url})", plugins::text("FakeNitro", "hyperLinkText").replace("{{NAME}}", name))
    } else {
        url.to_string()
    }
}

fn space_if(c: Option<char>) -> &'static str {
    if c.is_some_and(|c| !c.is_whitespace()) { " " } else { "" }
}

/// Custom emoji this account can't send become links to their image, as FakeNitro does.
fn fake_emojis(app: &App, guild: Id, text: &str) -> String {
    let size = plugins::text("FakeNitro", "emojiSize");
    let usable: Vec<Id> = app.store.emojis_for(guild).into_iter().filter(|(_, _, ok)| *ok).map(|(_, e, _)| e.id).collect();
    super::composer::outside_code(text, |part| {
        let mut out = String::new();
        let mut i = 0;
        while i < part.len() {
            let rest = &part[i..];
            let Some(n) = super::composer::custom_len(rest) else {
                let c = rest.chars().next().unwrap_or(' ');
                out.push(c);
                i += c.len_utf8();
                continue;
            };
            let raw = &rest[..n];
            let animated = raw.starts_with("<a:");
            let inner = raw.trim_start_matches("<a:").trim_start_matches("<:").trim_end_matches('>');
            match inner.split_once(':').and_then(|(name, id)| Some((name, id.parse::<Id>().ok()?))) {
                Some((name, id)) if !usable.contains(&id) => {
                    let url = format!("{}/emojis/{id}.{}?size={size}&name={name}&lossless=true", crate::api::cdn(), if animated { "gif" } else { "webp" });
                    out.push_str(space_if(part[..i].chars().last()));
                    out.push_str(&link(name, &url));
                    out.push_str(space_if(part[i + n..].chars().next()));
                }
                _ => out.push_str(raw),
            }
            i += n;
        }
        out
    })
}

pub fn sticker_link(id: Id, name: &str, fmt: u8) -> String {
    let url = format!("https://media.discordapp.net/stickers/{id}.{}?size={}&name={name}&lossless=true", if fmt == 4 { "gif" } else { "png" }, plugins::text("FakeNitro", "stickerSize"));
    link(name, &url)
}

pub fn bypass_emoji() -> bool {
    plugins::flag("FakeNitro", "enableEmojiBypass")
}

pub fn bypass_sticker() -> bool {
    plugins::flag("FakeNitro", "enableStickerBypass")
}

/// A random name for an upload, or the fixed one, with the extension kept.
pub fn anon_name(name: &str) -> String {
    let ext = name.rfind('.').map_or("", |i| &name[i..]);
    let stem = match plugins::text("AnonymiseFileNames", "method").as_str() {
        "1" => plugins::text("AnonymiseFileNames", "consistent"),
        "2" => chrono::Utc::now().timestamp_millis().to_string(),
        _ => {
            use std::hash::{BuildHasher, Hasher};
            let mut seed = std::collections::hash_map::RandomState::new().build_hasher().finish();
            (0..plugins::num("AnonymiseFileNames", "randomisedLength") as usize)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"[(seed >> 33) as usize % 62] as char
                })
                .collect()
        }
    };
    format!("{stem}{ext}")
}

/// Whether a reply to this message pings its author, as NoReplyMention and shift decide.
pub fn ping(app: &App, ch: Id, msg: Id, shift: bool) -> bool {
    let Some(m) = app.store.find_msg(ch, msg).filter(|m| m.author != app.store.me) else { return false };
    if !plugins::on("NoReplyMention") {
        return !shift;
    }
    let guild = app.store.channels.get(&ch).map_or(0, |c| c.guild);
    let roles = plugins::ids("NoReplyMention", "roleList");
    let listed = plugins::ids("NoReplyMention", "userList").contains(&m.author)
        || app.store.members.get(&guild).and_then(|g| g.get(&m.author)).is_some_and(|mem| mem.roles.iter().any(|r| roles.contains(r)));
    let exempt = if plugins::text("NoReplyMention", "shouldPingListed") == "true" { listed } else { !listed };
    if plugins::flag("NoReplyMention", "inverseShiftReply") { shift != exempt } else { !shift && exempt }
}

/// Ctrl+up/down walks the reply target, ctrl+shift+up/down your own messages to edit.
pub fn quick_reply(app: &mut App, ui: &egui::Ui, ch: Id) {
    let (reply, edit) = ui.input_mut(|i| {
        let key = |i: &mut egui::InputState, mods| {
            if super::take_key(i, mods, Key::ArrowUp) {
                Some(true)
            } else if super::take_key(i, mods, Key::ArrowDown) {
                Some(false)
            } else {
                None
            }
        };
        (key(i, Modifiers::COMMAND), key(i, Modifiers::COMMAND | Modifiers::SHIFT))
    });
    let Some(up) = reply.or(edit) else { return };
    let skip_blocked = plugins::flag("QuickReply", "ignoreBlockedAndIgnored");
    let me = app.store.me;
    let list: Vec<(Id, String)> = app
        .store
        .msgs
        .get(&ch)
        .map(|e| {
            e.list
                .iter()
                .filter(|m| !m.pending && matches!(m.kind, 0 | 19) && (reply.is_some() || m.author == me))
                .filter(|m| !skip_blocked || app.store.relationships.get(&m.author) != Some(&BLOCKED))
                .map(|m| (m.id, m.content.clone()))
                .collect()
        })
        .unwrap_or_default();
    let cur = if reply.is_some() { app.reply } else { app.editing.as_ref().map(|(id, _)| *id) };
    let at = cur.and_then(|c| list.iter().position(|(id, _)| *id == c));
    let next = match at {
        None => list.last(),
        Some(i) if up => i.checked_sub(1).and_then(|i| list.get(i)),
        Some(i) => list.get(i + 1),
    };
    if reply.is_some() {
        app.reply = next.map(|(id, _)| *id);
        if let Some((id, _)) = next {
            app.reply_ping = match plugins::text("QuickReply", "shouldMention").as_str() {
                "1" => app.store.find_msg(ch, *id).is_some_and(|m| m.author != me),
                "0" => false,
                _ => ping(app, ch, *id, false),
            };
        }
    } else {
        app.editing = next.cloned();
    }
    if let Some((id, _)) = next {
        app.jump_to(ch, *id);
    }
    app.focus_composer = reply.is_some();
}

pub fn translation(ctx: &egui::Context, id: Id) -> Option<String> {
    ctx.data(|d| d.get_temp(egui::Id::new(("translation", id))))
}

/// AddAttachments: pick files and put them on a message you already sent.
pub fn add_files(app: &mut App, ch: Id, id: Id) {
    let Some(m) = app.store.find_msg(ch, id) else { return };
    let room = 10usize.saturating_sub(m.attachments.len());
    if room == 0 {
        return app.toast("You cannot add more attachments to this message.");
    }
    let have: Vec<Value> = m
        .attachments
        .iter()
        .filter_map(|a| a.url.split("/attachments/").nth(1)?.split('/').nth(1).map(|i| json!({ "id": i })))
        .collect();
    let Some(files) = rfd::FileDialog::new().pick_files() else { return };
    if files.len() > room {
        return app.toast(format!("You can only add {room} more attachment{}", if room == 1 { "" } else { "s" }));
    }
    let Some(api) = api(app) else { return };
    app.toast("Uploading, this can take a while...");
    spawn(app, async move {
        Done::Toast(match api.add_attachments(ch, id, have, &files).await {
            Ok(_) => "Attachments added successfully!".to_string(),
            Err(e) => e,
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tracking_params() {
        assert_eq!(clear_urls("see https://example.com/a?utm_source=x&id=3&fbclid=abc#top, ok"), "see https://example.com/a?id=3#top, ok");
        assert_eq!(clear_urls("https://youtu.be/xyz?si=123"), "https://youtu.be/xyz");
        assert_eq!(clear_urls("[a](https://www.amazon.com/dp/B0?tag=zz&th=1)"), "[a](https://www.amazon.com/dp/B0)");
        assert_eq!(clear_urls("no links http here"), "no links http here");
        assert_eq!(clear_urls("https://example.com/?q=1"), "https://example.com/?q=1");
    }

    #[test]
    fn timestamps_only_for_clock_times() {
        assert_eq!(timestamps("`hello` `25:99` `0:13PM`"), "`hello` `25:99` `0:13PM`");
        assert!(timestamps("at `3:51 PM` ok").starts_with("at <t:"));
        assert!(timestamps("`17:59`").ends_with(":t>"));
    }
}

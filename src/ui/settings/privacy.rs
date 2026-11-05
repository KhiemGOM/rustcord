use egui::{Align, Align2, Color32, Layout, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::wire::{self, Patch};
use super::{Look, Page, action, caption, divider, dropdown, link, loading, note, radio, refetch, remote, section, sub, toggle};
use crate::net::Tag;
use crate::state::{Id, sid};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{App, avatar, text};

pub const TEXT: u64 = 6;
pub const NOTIF: u64 = 7;
pub const PRIVACY: u64 = 8;
pub const STATUS: u64 = 11;
pub const CLIPS: u64 = 19;

fn b64(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// The account's settings blob, asked for once per visit.
pub fn blob(app: &mut App, ui: &mut Ui) -> Option<Vec<u8>> {
    use base64::Engine;
    let v = loading(ui, remote(app, "proto", "/users/@me/settings-proto/1"))?;
    Some(base64::engine::general_purpose::STANDARD.decode(v["settings"].as_str().unwrap_or("")).unwrap_or_default())
}

/// Apply `patch` here at once and send it; the server's answer replaces our copy.
pub fn write(app: &mut App, patch: Patch) {
    use base64::Engine;
    let bytes = patch.bytes();
    if let Some(Some(Ok(cur))) = app.remote.get_mut("proto") {
        let old = base64::engine::general_purpose::STANDARD.decode(cur["settings"].as_str().unwrap_or("")).unwrap_or_default();
        cur["settings"] = json!(b64(&wire::merge(&old, &bytes)));
    }
    app.rest("PATCH", "/users/@me/settings-proto/1".into(), Some(json!({ "settings": b64(&bytes) })), Tag::Remote("proto_set"));
}

/// The settings blob as the page last saw it, for the e2e to read back.
pub fn current(app: &App) -> Vec<u8> {
    use base64::Engine;
    match app.remote.get("proto") {
        Some(Some(Ok(v))) => base64::engine::general_purpose::STANDARD.decode(v["settings"].as_str().unwrap_or("")).unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub fn proto_done(app: &mut App, res: Result<Value, String>) {
    match res {
        Ok(v) if v["settings"].is_string() => {
            app.remote.insert("proto", Some(Ok(v)));
        }
        Ok(_) => {}
        Err(e) => {
            app.toast(e);
            refetch(app, "proto");
        }
    }
}

fn flag(b: &[u8], g: u64, f: u64, default: bool) -> bool {
    wire::flag(b, g, f).unwrap_or(default)
}

fn guilds(app: &App) -> Vec<(Id, String, u32)> {
    app.store.guild_order.iter().filter_map(|g| app.store.guilds.get(g).map(|x| (*g, x.name.clone(), x.member_count))).collect()
}

fn consent_toggle(app: &mut App, ui: &mut Ui, key: &'static str, label: &str, desc: &str) {
    let Some(Some(Ok(c))) = app.remote.get("consent") else { return };
    let mut on = c[key]["consented"].as_bool().unwrap_or(false);
    if toggle(ui, &mut on, label, desc) {
        if let Some(Some(Ok(cur))) = app.remote.get_mut("consent") {
            cur[key]["consented"] = json!(on);
        }
        let body = if on { json!({ "grant": [key], "revoke": [] }) } else { json!({ "grant": [], "revoke": [key] }) };
        app.rest("POST", "/users/@me/consent".into(), Some(body), Tag::Remote("action"));
    }
}

fn learn(ui: &mut Ui, url: &str) {
    if link(ui, "Learn more").clicked() {
        let _ = open::that_detached(url);
    }
}

pub fn data(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "How Discord Uses My Data");
    super::lead(ui);
    ui.label(RichText::new("Use data to make Discord work").font(medium(16.0)).color(pal().header_primary));
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let t = |ui: &mut Ui, s: &str| ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
        t(ui, "Discord needs to store and process some data in order to provide you the basic Discord service, such as your messages, what servers you’re in and your Direct Messages. By using Discord, you allow Discord to provide this basic service. You can stop this by");
        if link(ui, "Disabling").clicked() {
            super::modals::open(app, super::modals::Edit::Disable);
        }
        t(ui, "or");
        if link(ui, "Deleting").clicked() {
            super::modals::open(app, super::modals::Edit::Delete);
        }
        t(ui, "your account.");
    });
    super::end(ui, 28.0);
    loading(ui, remote(app, "consent", "/users/@me/consent"));
    let Some(b) = blob(app, ui) else { return false };
    consent_toggle(app, ui, "usage_statistics", "Use data to improve Discord", "Allow Discord to use and process my information to understand and improve Discord’s services.");
    consent_toggle(app, ui, "personalization", "Use data to personalize my Discord experience", "Allows Discord to use information, such as who you talk to and what games you play, to personalize Discord for you.");
    let mut act = !flag(&b, PRIVACY, 18, false);
    if toggle(ui, &mut act, "Use my Discord activity to personalize Sponsored Content", "Allows Discord to personalize Sponsored Content, like Quests, using your Discord activity, such as the games you play. If you opt out you may still see Quests, but they won’t be personalized.") {
        write(app, Patch::new().flag(PRIVACY, 18, !act));
    }
    let mut third = !flag(&b, PRIVACY, 30, false);
    if toggle(ui, &mut third, "Use third-party data to personalize Sponsored Content", "Allows Discord to personalize Sponsored Content, like Quests, using data Discord receives from advertisers and third-party data providers. If you opt out, you may still see Quests, but they won’t be personalized using data from third parties.") {
        write(app, Patch::new().flag(PRIVACY, 30, !third));
    }
    let mut clips = flag(&b, CLIPS, 1, true);
    if toggle(ui, &mut clips, "Allow my voice to be recorded in Clips", "Allows your voice to be included when someone in the same voice channel uses Clips.") {
        write(app, Patch::new().flag(CLIPS, 1, clips));
    }

    section(ui, "Request my data");
    super::lead(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let t = |ui: &mut Ui, s: &str| ui.label(RichText::new(s).font(reg(14.0)).color(pal().header_secondary));
        t(ui, "If you need a copy of your personal data from your account's entire history, you can get it here.");
        learn(ui, "https://support.discord.com/hc/en-us/articles/360004027692");
        t(ui, "about how getting a copy of your data works.");
    });
    super::end(ui, 16.0);
    let status = remote(app, "harvest", "/users/@me/harvest").cloned();
    let pending = match &status {
        Some(Ok(v)) => v.as_array().and_then(|l| l.first()).filter(|h| h["completed_at"].is_null() && h["status"].as_u64().is_none_or(|s| s < 4)).cloned(),
        _ => None,
    };
    if let Some(h) = &pending {
        note(ui, &format!("You asked for your data{}. Discord will email you when it's ready.", h["created_at"].as_str().map(|t| format!(" on {}", &t[..t.len().min(10)])).unwrap_or_default()));
    } else if action(ui, "Request all of my Data", "It can take up to 30 days to arrive in your email.", "Request Data", Look::Secondary) {
        app.rest("POST", "/users/@me/harvest".into(), Some(json!({})), Tag::Remote("action"));
        refetch(app, "harvest");
        app.toast("Data request sent. Discord will email you when it's ready.");
    }

    section(ui, "Profile Privacy");
    note(ui, "Control who can see your profile info — like your bio and connected accounts.");
    learn(ui, "https://support.discord.com/hc/en-us/articles/38859942749463-Profile-Privacy-Setting-on-Discord");
    super::end(ui, 24.0);
    sub(ui, "Share my full profile with");
    let mut vis = wire::num(&b, PRIVACY, 32).filter(|v| *v != 0).unwrap_or(3);
    let opts = [
        (3u64, "Friends & All Servers", "Your full profile is visible to friends and any server you join."),
        (2, "Friends & Small Servers Only", "Your full profile is visible to friends and any server you join with 200 or fewer members. Everyone else sees a limited version."),
        (1, "Friends Only", "Your full profile is visible to friends. Everyone else sees a limited version."),
    ];
    if radio(ui, &mut vis, &opts) {
        write(app, Patch::new().num(PRIVACY, 32, vis));
    }
    let mut upd = flag(&b, NOTIF, 24, true);
    if toggle(ui, &mut upd, "Share when I update my profile", "Allow friends to receive a push notification when you update your profile.") {
        write(app, Patch::new().flag(NOTIF, 24, upd));
    }

    section(ui, "Related Settings");
    related(app, ui, "Activity Privacy", &format!("Control how your game and app activity is shared—what's visible and who sees it. You're sharing activity with {}.", sharing_summary(app, &b)), Page::Activity);
    false
}

fn sharing_summary(app: &App, b: &[u8]) -> String {
    let list = guilds(app);
    let off = wire::ids(b, PRIVACY, 13);
    let on: Vec<&(Id, String, u32)> = list.iter().filter(|g| !off.contains(&(g.0 as u64))).collect();
    match on.len() {
        0 => "no servers".into(),
        1 => on[0].1.clone(),
        n => format!("{} and {} other servers", on[0].1, n - 1),
    }
}

/// A row that opens another settings page.
pub fn related(app: &mut App, ui: &mut Ui, head: &str, desc: &str, to: Page) {
    super::lead(ui);
    let w = ui.available_width();
    let g = ui.painter().layout(desc.into(), reg(14.0), pal().header_secondary, w - 64.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0 + g.size().y), Sense::click());
    ui.painter().text(r.left_top(), Align2::LEFT_TOP, head, medium(16.0), pal().header_primary);
    ui.painter().galley(r.left_top() + vec2(0.0, 24.0), g, pal().header_secondary);
    icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(24.0, 24.0)), pal().header_primary);
    super::mark(ui, &format!("related:{head}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    super::end(ui, 24.0);
    if resp.clicked() {
        app.settings_page = Some(to);
    }
}

// messaging ---------------------------------------------------------------------------------

const SHOW: u64 = 1;
const BLUR: u64 = 2;
const BLOCK: u64 = 3;

fn redaction(n: u64) -> &'static str {
    match n {
        SHOW => "Show",
        BLUR => "Blur",
        _ => "Block",
    }
}

pub fn messaging(app: &mut App, ui: &mut Ui) -> bool {
    section(ui, "Content Filters");
    let Some(b) = blob(app, ui) else { return false };
    sub(ui, "Sensitive content filters");
    note(ui, "Choose how you want to see image-based media detected by Discord’s sensitive content filters.");
    learn(ui, "https://support.discord.com/hc/en-us/articles/18210995019671");
    super::end(ui, 16.0);
    super::lead(ui);
    ui.horizontal(|ui| {
        for (i, t) in ["Mature Sexual Media", "Graphic Media"].iter().enumerate() {
            let sel = app.sform.media_tab == i as u8;
            let g = ui.painter().layout_no_wrap(t.to_string(), medium(16.0), pal().header_primary);
            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, 32.0), Sense::click());
            if sel {
                ui.painter().rect_filled(r, 8, Color32::from_rgba_unmultiplied(151, 151, 159, 51));
            }
            super::mark(ui, &format!("tab:{t}"), r);
            ui.painter().galley(pos2(r.left() + 16.0, r.center().y - g.size().y / 2.0), g, if sel || resp.hovered() { pal().header_primary } else { pal().header_secondary });
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                app.sform.media_tab = i as u8;
            }
        }
    });
    super::end(ui, 16.0);
    let (field, defaults, blurb) = if app.sform.media_tab == 0 {
        (29, [SHOW, BLUR, SHOW], "Helps detect image-based media that may contain sexually explicit or suggestive material involving adults.")
    } else {
        (32, [BLUR, BLOCK, BLUR], "Helps detect image-based media that may contain violent or potentially disturbing visual content.")
    };
    let cur: Vec<u64> = (0..3).map(|i| wire::inner(&b, TEXT, field, i + 1).filter(|v| *v != 0).unwrap_or(defaults[i as usize])).collect();
    let mut next = cur.clone();
    for (i, label) in ["Direct messages from friends", "Direct messages from others", "Messages in server channels"].iter().enumerate() {
        super::lead(ui);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, *label, medium(16.0), pal().header_primary);
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(Layout::right_to_left(Align::Center)));
        let resp = dropdown(&mut c, 140.0, None, redaction(cur[i]), |ui| {
            for v in [SHOW, BLUR, BLOCK] {
                if ui.selectable_label(cur[i] == v, redaction(v)).clicked() {
                    next[i] = v;
                }
            }
        });
        super::mark(ui, &format!("filter:{label}"), resp.rect);
        super::end(ui, 16.0);
    }
    note(ui, blurb);
    if next != cur {
        write(app, Patch::new().sub(TEXT, field, &[(1, next[0]), (2, next[1]), (3, next[2])]));
    }
    divider(ui);
    let mut cmds = flag(&b, TEXT, 23, false);
    if toggle(ui, &mut cmds, "Allow access to age-restricted commands from apps in DMs", "Allows people 18+ to access commands marked as age-restricted in DMs. Applies to all apps.") {
        write(app, Patch::new().flag(TEXT, 23, cmds));
    }
    let mut ios = flag(&b, TEXT, 20, false);
    if toggle(ui, &mut ios, "Allow access to age-restricted servers on iOS", "Access age-restricted servers (18+) on iOS devices, after joining them on desktop.") {
        write(app, Patch::new().flag(TEXT, 20, ios));
    }
    super::end(ui, 16.0);
    related(app, ui, "Appearance", "Show/hide media in chat, spoiler content", Page::Appearance);

    section(ui, "Spam Filters");
    caption(ui, "Automatically filter suspected spam messages", "Discord can filter out some messages that contain spam. These messages go to your Spam inbox.");
    super::end(ui, 16.0);
    let mut spam = wire::num(&b, TEXT, 27).filter(|v| *v != 0).unwrap_or(2);
    let opts = [(3u64, "Filter all spam", ""), (2, "Filter messages from non-friends (Recommended)", ""), (1, "Don't filter spam", "")];
    if radio(ui, &mut spam, &opts) {
        write(app, Patch::new().num(TEXT, 27, spam));
    }

    section(ui, "Direct Message (DM) Permissions");
    dm_permissions(app, ui, &b);

    section(ui, "Friend Request Permissions");
    note(ui, "Control who can send you friend requests and how they appear.");
    sub(ui, "Allow Friend Requests from...");
    let flags = wire::wrapped(&b, PRIVACY, 11).unwrap_or(14);
    let mut next = flags;
    for (bit, label, desc) in [(8u64, "Everyone", ""), (2, "Friend of friends", ""), (4, "Server members", "Server members can only send you friend requests from servers where you also allow Direct Messages.")] {
        let mut on = flags & bit != 0;
        if toggle(ui, &mut on, label, desc) {
            next = if bit == 8 && on { 14 } else if on { flags | bit } else { flags & !bit };
        }
    }
    if next != flags {
        write(app, Patch::new().sub(PRIVACY, 11, &[(1, next)]));
    }
    super::end(ui, 16.0);
    let mut personal = !flag(&b, PRIVACY, 33, false);
    if toggle(ui, &mut personal, "Show personalized messages", "Show personalized messages on incoming friend requests. If you accept, the message will still appear in your DMs.") {
        write(app, Patch::new().flag(PRIVACY, 33, !personal));
    }

    section(ui, "Messaging in Connected Games");
    note(ui, "These are settings for games that use Discord to power their social experiences.");
    super::lead(ui);
    ui.label(RichText::new("No games Connected").font(medium(16.0)).color(pal().header_primary));
    ui.horizontal(|ui| {
        ui.label(RichText::new("Want to connect a game to your account?").font(reg(14.0)).color(pal().header_secondary));
        learn(ui, "https://support.discord.com/hc/en-us/articles/27893392334359");
    });
    super::end(ui, 16.0);

    section(ui, "Related Settings");
    related(app, ui, "Authorized Apps", "Manage your games in Authorized Apps", Page::Apps);

    section(ui, "Ignore & Block");
    super::lead(ui);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("You’re in control. To compare your options for reducing unwanted interactions,").font(reg(14.0)).color(pal().header_secondary));
        learn_as(ui, "explore our feature guide", "https://support.discord.com/hc/en-us/articles/28084948873623");
    });
    super::end(ui, 24.0);
    people(app, ui);
    false
}

fn learn_as(ui: &mut Ui, label: &str, url: &str) {
    if link(ui, label).clicked() {
        let _ = open::that_detached(url);
    }
}

fn dm_permissions(app: &mut App, ui: &mut Ui, b: &[u8]) {
    let list = guilds(app);
    let at = app.sform.dm_guild.filter(|g| list.iter().any(|x| x.0 == *g));
    let shown = at.and_then(|g| list.iter().find(|x| x.0 == g)).map_or("All servers".to_string(), |g| g.1.clone());
    super::lead(ui);
    let mut pick = None;
    let resp = dropdown(ui, ui.available_width(), None, &shown, |ui| {
        if ui.selectable_label(at.is_none(), "All servers").clicked() {
            pick = Some(None);
        }
        for (g, n, _) in &list {
            if ui.selectable_label(at == Some(*g), n).clicked() {
                pick = Some(Some(*g));
            }
        }
    });
    super::mark(ui, "dm_server", resp.rect);
    super::end(ui, 24.0);
    if let Some(p) = pick {
        app.sform.dm_guild = p;
    }
    let (dm_ids, rq_ids) = (wire::ids(b, PRIVACY, 3), wire::ids(b, PRIVACY, 16));
    let all: Vec<u64> = list.iter().map(|g| g.0 as u64).collect();
    let (mut dm, mut rq) = match at {
        Some(g) => (!dm_ids.contains(&(g as u64)), !rq_ids.contains(&(g as u64))),
        None => (!all.iter().any(|g| dm_ids.contains(g)), !all.iter().any(|g| rq_ids.contains(g))),
    };
    let mut patch = Patch::new();
    let mut changed = false;
    let edit = |ids: &[u64], on: bool| -> Vec<u64> {
        match at {
            Some(g) => ids.iter().copied().filter(|x| *x != g as u64).chain((!on).then_some(g as u64)).collect(),
            None if on => ids.iter().copied().filter(|x| !all.contains(x)).collect(),
            None => all.clone(),
        }
    };
    if toggle(ui, &mut dm, "Direct Messages", "Allow DMs from other server members") {
        patch = patch.ids(PRIVACY, 3, &edit(&dm_ids, dm));
        if at.is_none() {
            patch = patch.sub(PRIVACY, 27, &[(1, !dm as u64)]);
        }
        changed = true;
    }
    if toggle(ui, &mut rq, "Message requests", "Filter messages from server members you may not know.") {
        patch = patch.ids(PRIVACY, 16, &edit(&rq_ids, rq));
        if at.is_none() {
            patch = patch.sub(PRIVACY, 17, &[(1, !rq as u64)]);
        }
        changed = true;
    }
    if changed {
        write(app, patch);
    }
}

fn people(app: &mut App, ui: &mut Ui) {
    let Some(v) = loading(ui, remote(app, "rels", "/users/@me/relationships")) else { return };
    let rows = v.as_array().cloned().unwrap_or_default();
    let blocked: Vec<&Value> = rows.iter().filter(|r| r["type"] == 2).collect();
    let ignored: Vec<&Value> = rows.iter().filter(|r| r["user_ignored"].as_bool() == Some(true)).collect();
    let mut act: Option<(String, bool)> = None;
    for (title, list, verb, key, ignore) in [("Blocked accounts", &blocked, "Unblock", 0u8, false), ("Ignored accounts", &ignored, "Unignore", 1, true)] {
        if list.is_empty() {
            continue;
        }
        super::lead(ui);
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(medium(16.0)).color(pal().header_primary));
            ui.label(RichText::new(format!("{} accounts", list.len())).font(reg(14.0)).color(pal().header_secondary));
        });
        ui.add_space(12.0);
        let open = app.sform.people_open[key as usize];
        for r in list.iter().take(if open { usize::MAX } else { 5 }) {
            let u = &r["user"];
            let name = u["global_name"].as_str().or(u["username"].as_str()).unwrap_or("");
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
            let av = Rect::from_center_size(pos2(row.left() + 20.0, row.center().y), vec2(32.0, 32.0));
            let uid = sid(&u["id"]);
            match u["avatar"].as_str() {
                Some(h) => avatar(ui, av, &format!("{}/avatars/{uid}/{h}.png?size=64", crate::api::cdn())),
                None => {
                    ui.painter().circle_filled(av.center(), 16.0, BLURPLE);
                }
            }
            text::paint(ui.painter(), pos2(row.left() + 48.0, row.center().y), Align2::LEFT_CENTER, name, medium(16.0), pal().header_primary, 300.0);
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::Center)));
            let b = super::secondary(&mut c, verb);
            super::mark(ui, &format!("{verb}:{}", u["username"].as_str().unwrap_or("")), b.rect);
            if b.clicked() {
                act = Some((u["id"].as_str().unwrap_or("").to_string(), ignore));
            }
        }
        if list.len() > 5 {
            let more = if open { "See less".to_string() } else { format!("See {} more", list.len() - 5) };
            if super::secondary(ui, &more).clicked() {
                app.sform.people_open[key as usize] = !open;
            }
        }
        ui.add_space(24.0);
    }
    if let Some((id, ignore)) = act {
        let path = if ignore { format!("/users/@me/relationships/{id}/ignore") } else { format!("/users/@me/relationships/{id}") };
        app.rest("DELETE", path, None, Tag::Remote("action"));
        if !ignore && let Ok(uid) = id.parse::<Id>() {
            app.store.relationships.remove(&uid);
        }
        refetch(app, "rels");
    }
}

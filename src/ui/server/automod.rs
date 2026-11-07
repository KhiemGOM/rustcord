use egui::{Align2, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use std::collections::HashSet;

use super::{App, faint, rule, title};
use crate::net::Tag;
use crate::state::{Id, sid};
use crate::theme::*;
use crate::ui::icons::Icon;
use crate::ui::settings;

#[derive(Default)]
pub struct AutoMod {
    form: Option<Form>,
}

#[derive(Clone, Default)]
struct Form {
    id: Id,
    trigger: u8,
    name: String,
    words: String,
    regex: String,
    allow: String,
    presets: [bool; 3],
    limit: u32,
    raid: bool,
    block: bool,
    block_msg: String,
    alert: Id,
    timeout: u64,
    block_member: bool,
    roles: HashSet<Id>,
    chans: HashSet<Id>,
    enabled: bool,
    checked: String,
    edited: f64,
}

const KINDS: [(u8, &str, &str, &str); 5] = [
    (4, "Block Commonly Flagged Words", "Block messages containing words that are commonly flagged as profanity, sexual content or slurs.", "Commonly Flagged Words"),
    (1, "Block Custom Words", "Block messages containing these words from being posted.", "Custom Words"),
    (3, "Block Suspected Spam Content", "Block messages with spam-like content from being posted.", "Suspected Spam"),
    (5, "Block Mention Spam", "Block messages with an excessive # of unique mentions.", "Mention Spam"),
    (6, "Block Members With Flagged Words In Their Profile", "Block members with certain words in their username, display name, nickname or bio from interacting.", "Member Profile"),
];

const PRESETS: [(&str, &str); 3] = [("Profanity", "Swear words and curse words"), ("Sexual Content", "Words that refer to sexually explicit behavior or activity"), ("Slurs", "Words that insult or harass an individual or group")];

const TIMEOUTS: [(u64, &str); 6] = [(60, "60 seconds"), (300, "5 mins"), (600, "10 mins"), (3600, "1 hour"), (86400, "1 day"), (604800, "1 week")];

fn lines(s: &str) -> Vec<String> {
    s.split(['\n', ',']).map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

impl AutoMod {
    pub fn deep(&self) -> bool {
        self.form.is_some()
    }
}

impl Form {
    fn new(trigger: u8) -> Form {
        let name = KINDS.iter().find(|k| k.0 == trigger).map_or("", |k| k.3).to_string();
        Form { trigger, name, presets: [true; 3], limit: 5, raid: true, block: trigger != 6, enabled: true, timeout: 0, block_member: trigger == 6, ..Default::default() }
    }

    fn of(r: &Value) -> Form {
        let m = &r["trigger_metadata"];
        let list = |k: &str| m[k].as_array().into_iter().flatten().filter_map(|s| s.as_str()).collect::<Vec<_>>().join("\n");
        let presets: Vec<u64> = m["presets"].as_array().into_iter().flatten().filter_map(|p| p.as_u64()).collect();
        let mut f = Form {
            id: sid(&r["id"]),
            trigger: r["trigger_type"].as_u64().unwrap_or(1) as u8,
            name: r["name"].as_str().unwrap_or_default().into(),
            words: list("keyword_filter"),
            regex: list("regex_patterns"),
            allow: list("allow_list"),
            presets: [presets.contains(&1), presets.contains(&2), presets.contains(&3)],
            limit: m["mention_total_limit"].as_u64().unwrap_or(5) as u32,
            raid: m["mention_raid_protection_enabled"].as_bool().unwrap_or(false),
            enabled: r["enabled"].as_bool().unwrap_or(true),
            roles: r["exempt_roles"].as_array().into_iter().flatten().map(sid).collect(),
            chans: r["exempt_channels"].as_array().into_iter().flatten().map(sid).collect(),
            ..Default::default()
        };
        for a in r["actions"].as_array().into_iter().flatten() {
            match a["type"].as_u64() {
                Some(1) => {
                    f.block = true;
                    f.block_msg = a["metadata"]["custom_message"].as_str().unwrap_or_default().into();
                }
                Some(2) => f.alert = sid(&a["metadata"]["channel_id"]),
                Some(3) => f.timeout = a["metadata"]["duration_seconds"].as_u64().unwrap_or(0),
                Some(4) => f.block_member = true,
                _ => {}
            }
        }
        f
    }

    fn body(&self) -> Value {
        let mut meta = json!({});
        match self.trigger {
            1 | 6 => {
                meta["keyword_filter"] = json!(lines(&self.words));
                meta["regex_patterns"] = json!(lines(&self.regex));
                meta["allow_list"] = json!(lines(&self.allow));
            }
            4 => {
                let p: Vec<u8> = (0..3).filter(|i| self.presets[*i]).map(|i| i as u8 + 1).collect();
                meta["presets"] = json!(p);
                meta["allow_list"] = json!(lines(&self.allow));
            }
            5 => {
                meta["mention_total_limit"] = json!(self.limit);
                meta["mention_raid_protection_enabled"] = json!(self.raid);
            }
            _ => {}
        }
        let mut actions = Vec::new();
        if self.block {
            actions.push(json!({ "type": 1, "metadata": if self.block_msg.trim().is_empty() { json!({}) } else { json!({ "custom_message": self.block_msg.trim() }) } }));
        }
        if self.alert != 0 {
            actions.push(json!({ "type": 2, "metadata": { "channel_id": self.alert.to_string() } }));
        }
        if self.timeout != 0 && self.trigger != 6 {
            actions.push(json!({ "type": 3, "metadata": { "duration_seconds": self.timeout } }));
        }
        if self.block_member {
            actions.push(json!({ "type": 4 }));
        }
        json!({
            "name": self.name.trim(),
            "event_type": if self.trigger == 6 { 2 } else { 1 },
            "trigger_type": self.trigger,
            "trigger_metadata": meta,
            "actions": actions,
            "enabled": self.enabled,
            "exempt_roles": self.roles.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
            "exempt_channels": self.chans.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
        })
    }

    fn valid(&self) -> bool {
        let content = match self.trigger {
            1 | 6 => !lines(&self.words).is_empty() || !lines(&self.regex).is_empty(),
            4 => self.presets.iter().any(|p| *p),
            _ => true,
        };
        let acts = self.block || self.alert != 0 || self.timeout != 0 || self.block_member;
        !self.name.trim().is_empty() && content && acts
    }
}

fn summary(r: &Value) -> String {
    let m = &r["trigger_metadata"];
    let mut parts = Vec::new();
    match r["trigger_type"].as_u64() {
        Some(1) | Some(6) => {
            let n = m["keyword_filter"].as_array().map_or(0, |a| a.len()) + m["regex_patterns"].as_array().map_or(0, |a| a.len());
            parts.push(format!("{n} word{}", if n == 1 { "" } else { "s" }));
        }
        Some(4) => parts.push("Commonly flagged words".to_string()),
        Some(5) => parts.push(format!("More than {} mentions", m["mention_total_limit"].as_u64().unwrap_or(0))),
        _ => parts.push("Suspected spam".to_string()),
    }
    for a in r["actions"].as_array().into_iter().flatten() {
        parts.push(
            match a["type"].as_u64() {
                Some(1) => "Block message",
                Some(2) => "Send alert",
                Some(3) => "Timeout",
                _ => "Block member interactions",
            }
            .into(),
        );
    }
    parts.join(" \u{2022} ")
}

pub fn page(app: &mut App, ui: &mut Ui, gid: Id) {
    if app.server.as_ref().is_some_and(|s| s.pages.automod.form.is_some()) {
        return edit(app, ui, gid);
    }
    title(ui, "AutoMod", "Give your mods a break while keeping your server safe! Set up filters to moderate content and automate a custom response when they\u{2019}re found, and AutoMod will make it happen.");
    if settings::link(ui, "Learn More").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/4421269296535"));
    }
    ui.add_space(24.0);
    let rules: Vec<Value> = app.server_data("automod", format!("/guilds/{gid}/auto-moderation/rules")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut open = None;
    let mut toggle = None;
    let mut create = None;
    for (t, head, desc, _) in KINDS {
        let existing: Vec<&Value> = rules.iter().filter(|r| r["trigger_type"].as_u64() == Some(t as u64)).collect();
        let cap = match t {
            1 => 6,
            _ => 1,
        };
        ui.label(RichText::new(head).font(semi(16.0)).color(pal().header_primary));
        ui.label(RichText::new(desc).font(reg(14.0)).color(pal().header_secondary));
        ui.add_space(8.0);
        for r in &existing {
            let (rc, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
            ui.painter().rect(rc, CornerRadius::same(8), crate::ui::motion::mix(pal().bg_secondary, pal().bg_mod_hover, crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
            crate::ui::text::paint(ui.painter(), pos2(rc.left() + 16.0, rc.center().y - 10.0), Align2::LEFT_CENTER, r["name"].as_str().unwrap_or_default(), semi(16.0), pal().header_primary, rc.width() - 160.0);
            crate::ui::text::paint(ui.painter(), pos2(rc.left() + 16.0, rc.center().y + 12.0), Align2::LEFT_CENTER, &summary(r), reg(12.0), pal().header_secondary, rc.width() - 160.0);
            let on = r["enabled"].as_bool().unwrap_or(false);
            let sw = Rect::from_center_size(pos2(rc.right() - 40.0, rc.center().y), vec2(48.0, 24.0));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(sw));
            if settings::switch(&mut c, on).clicked() {
                toggle = Some((sid(&r["id"]), !on));
            }
            if resp.clicked() && !ui.rect_contains_pointer(sw) {
                open = Some((*r).clone());
            }
            ui.add_space(8.0);
        }
        if existing.len() < cap && settings::secondary(ui, if existing.is_empty() { "Set Up" } else { "Create Rule" }).clicked() {
            create = Some(t);
        }
        ui.add_space(24.0);
    }
    if let Some((id, on)) = toggle {
        app.rest("PATCH", format!("/guilds/{gid}/auto-moderation/rules/{id}"), Some(json!({ "enabled": on })), Tag::rollback(crate::ui::failed::stale("automod")));
        if let Some(Value::Array(a)) = app.server.as_mut().and_then(|s| s.data.get_mut("automod")) {
            for r in a.iter_mut().filter(|r| sid(&r["id"]) == id) {
                r["enabled"] = json!(on);
            }
        }
    }
    if let Some(s) = &mut app.server {
        if let Some(r) = open {
            s.pages.automod.form = Some(Form::of(&r));
        } else if let Some(t) = create {
            s.pages.automod.form = Some(Form::new(t));
        }
    }
}

fn field(ui: &mut Ui, label: &str, desc: &str) {
    ui.label(RichText::new(label).font(medium(16.0)).color(pal().header_primary));
    if !desc.is_empty() {
        ui.label(RichText::new(desc).font(reg(14.0)).color(pal().header_secondary));
    }
    ui.add_space(8.0);
}

fn area(ui: &mut Ui, text: &mut String, hint: &str) {
    Frame::new().fill(egui::Color32::from_black_alpha(31)).stroke(Stroke::new(1.0, faint(61))).corner_radius(8).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
        ui.add(egui::TextEdit::multiline(text).frame(Frame::NONE).desired_rows(3).font(reg(16.0)).text_color(pal().text_normal).hint_text(RichText::new(hint).color(pal().text_muted)).desired_width(ui.available_width()));
    });
}

fn picker(ui: &mut Ui, label: &str, items: &[(Id, String)], set: &mut HashSet<Id>) {
    let shown = if set.is_empty() { format!("Select {}", label.to_lowercase()) } else { format!("{} selected", set.len()) };
    settings::dropdown(ui, ui.available_width(), None, &shown, |ui| {
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (id, n) in items {
                let on = set.contains(id);
                if crate::ui::menu_item(ui, on.then_some(Icon::Check), n, false) && !set.remove(id) {
                    set.insert(*id);
                }
            }
        });
    });
}

fn edit(app: &mut App, ui: &mut Ui, gid: Id) {
    let Some(mut f) = app.server.as_mut().and_then(|s| s.pages.automod.form.take()) else { return };
    let kind = KINDS.iter().find(|k| k.0 == f.trigger).map_or("AutoMod", |k| k.1);
    let (mut close, mut save, mut delete) = (false, false, false);
    close |= ui.add(egui::Label::new(RichText::new("\u{2190} Back").font(semi(14.0)).color(pal().header_secondary)).sense(Sense::click())).clicked();
    ui.add_space(12.0);
    title(ui, kind, KINDS.iter().find(|k| k.0 == f.trigger).map_or("", |k| k.2));
    settings::h5(ui, "Rule name");
    settings::input(ui, &mut f.name, "Enter a rule name", false);
    rule(ui);
    match f.trigger {
        1 | 6 => {
            field(ui, "Block words", "Add words or phrases, separated by commas or new lines. Use * as a wildcard, like cat* or *cat.");
            area(ui, &mut f.words, "Words to block");
            ui.add_space(16.0);
            field(ui, "Regex patterns", "Optional. Block text matching these patterns, one per line.");
            let before = f.regex.clone();
            area(ui, &mut f.regex, "^spam.*$");
            let now = ui.input(|i| i.time);
            if f.regex != before {
                f.edited = now;
            }
            if f.regex.trim().is_empty() {
                f.checked.clear();
                if let Some(s) = &mut app.server {
                    s.errors.retain(|e| !e.0.starts_with("trigger_metadata.regex_patterns"));
                }
            } else if f.regex != f.checked && now - f.edited > 0.5 {
                f.checked = f.regex.clone();
                app.rest("POST", format!("/guilds/{gid}/auto-moderation/rules/validate"), Some(f.body()), Tag::Server("automod_regex"));
            } else if f.regex != f.checked {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(550));
            }
            for (path, why) in app.server.iter().flat_map(|s| s.errors.iter()).filter(|e| e.0.starts_with("trigger_metadata.regex_patterns.")) {
                ui.label(RichText::new(format!("{}: {why}", path.trim_start_matches("trigger_metadata.regex_patterns."))).font(reg(12.0)).color(pal().critical));
            }
            ui.add_space(16.0);
            field(ui, "Exempt words", "Words that should never be blocked by this rule.");
            area(ui, &mut f.allow, "Allowed words");
        }
        4 => {
            field(ui, "Wordsets", "Choose which kinds of words to block.");
            for (i, (n, d)) in PRESETS.iter().enumerate() {
                settings::toggle(ui, &mut f.presets[i], n, d);
                ui.add_space(12.0);
            }
            field(ui, "Exempt words", "Words that should never be blocked by this rule.");
            area(ui, &mut f.allow, "Allowed words");
        }
        5 => {
            field(ui, "Mention limit", "Block messages with more unique mentions of roles and users than this.");
            let mut v = f.limit as f32;
            settings::slider(ui, 400.0, &mut v, 1.0, 50.0);
            f.limit = v.round() as u32;
            ui.label(RichText::new(format!("{} mentions", f.limit)).font(medium(14.0)).color(pal().header_secondary));
            ui.add_space(16.0);
            settings::toggle(ui, &mut f.raid, "Mention raid protection", "Automatically detect mention raids and temporarily block the members taking part.");
        }
        _ => {
            ui.label(RichText::new("This rule needs no extra settings. Discord decides what looks like spam.").font(reg(14.0)).color(pal().header_secondary));
        }
    }
    rule(ui);
    ui.label(RichText::new("Response").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(8.0);
    if f.trigger != 6 {
        settings::toggle(ui, &mut f.block, "Block message", "Stop the message from being sent and show the member why.");
        if f.block {
            ui.add_space(8.0);
            settings::input(ui, &mut f.block_msg, "Custom message shown to the member (optional)", false);
        }
        ui.add_space(16.0);
    } else {
        settings::toggle(ui, &mut f.block_member, "Block member interactions", "Stop the member from interacting with others until a moderator lets them.");
        ui.add_space(16.0);
    }
    let chans: Vec<(Id, String)> = app.store.guilds.get(&gid).map(|g| g.channels.clone()).unwrap_or_default().into_iter().filter_map(|c| app.store.channels.get(&c)).filter(|c| matches!(c.kind, 0 | 5)).map(|c| (c.id, c.name.clone())).collect();
    let mut alert_on = f.alert != 0;
    if settings::toggle(ui, &mut alert_on, "Send alert", "Post a message in a channel whenever the rule catches something.") {
        f.alert = if alert_on { chans.first().map_or(0, |c| c.0) } else { 0 };
    }
    if f.alert != 0 {
        ui.add_space(8.0);
        let shown = chans.iter().find(|c| c.0 == f.alert).map(|c| format!("#{}", c.1)).unwrap_or_default();
        settings::dropdown(ui, ui.available_width(), Some(Icon::Hash), &shown, |ui| {
            crate::ui::menu_body(ui, |ui| {
                for (c, n) in &chans {
                    if crate::ui::menu_item(ui, (*c == f.alert).then_some(Icon::Check), n, false) {
                        f.alert = *c;
                    }
                }
            });
        });
    }
    ui.add_space(16.0);
    if f.trigger != 6 {
        let mut t_on = f.timeout != 0;
        if settings::toggle(ui, &mut t_on, "Timeout member", "Stop the member from sending messages or joining voice for a while.") {
            f.timeout = if t_on { 3600 } else { 0 };
        }
        if f.timeout != 0 {
            ui.add_space(8.0);
            let shown = TIMEOUTS.iter().find(|t| t.0 == f.timeout).map_or("1 hour", |t| t.1);
            settings::dropdown(ui, 240.0, None, shown, |ui| {
                for (s, l) in TIMEOUTS {
                    if crate::ui::menu_item(ui, (f.timeout == s).then_some(Icon::Check), l, false) {
                        f.timeout = s;
                    }
                }
            });
        }
        ui.add_space(8.0);
    }
    rule(ui);
    ui.label(RichText::new("Exemptions").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(8.0);
    field(ui, "Exempt roles", "Members with these roles are never affected by this rule.");
    let roles: Vec<(Id, String)> = app.store.guilds.get(&gid).map(|g| g.roles.values().filter(|r| r.id != gid).map(|r| (r.id, r.name.clone())).collect()).unwrap_or_default();
    picker(ui, "roles", &roles, &mut f.roles);
    ui.add_space(16.0);
    if f.trigger != 6 {
        field(ui, "Exempt channels", "This rule ignores messages in these channels.");
        picker(ui, "channels", &chans, &mut f.chans);
        ui.add_space(16.0);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.add_enabled_ui(f.valid(), |ui| settings::primary(ui, "Save Rule")).inner.clicked() {
            save = true;
        }
        if settings::secondary(ui, "Cancel").clicked() {
            close = true;
        }
        if f.id != 0 {
            delete = settings::danger(ui, "Delete Rule", false).clicked();
        }
    });
    if !f.valid() {
        ui.add_space(8.0);
        ui.label(RichText::new("Add a name, something to catch and at least one response to save this rule.").font(reg(12.0)).color(pal().text_muted));
    }
    if save {
        let (method, path) = if f.id == 0 { ("POST", format!("/guilds/{gid}/auto-moderation/rules")) } else { ("PATCH", format!("/guilds/{gid}/auto-moderation/rules/{}", f.id)) };
        app.rest(method, path, Some(f.body()), Tag::Server("automod_saved"));
        close = true;
    }
    if delete {
        let (id, name) = (f.id, f.name.clone());
        app.confirm = Some(crate::ui::Confirm::new("Delete Rule", format!("Are you sure you want to delete **{name}**? This cannot be undone."), "Delete", move |app: &mut App| {
            app.rest("DELETE", format!("/guilds/{gid}/auto-moderation/rules/{id}"), None, Tag::rollback(crate::ui::failed::stale("automod")));
            if let Some(s) = &mut app.server {
                s.pages.automod.form = None;
                if let Some(Value::Array(a)) = s.data.get_mut("automod") {
                    a.retain(|r| sid(&r["id"]) != id);
                }
            }
        }));
    }
    if !close && let Some(s) = &mut app.server {
        s.pages.automod.form = Some(f);
    }
}

pub fn saved(app: &mut App, v: Value) {
    let Some(s) = &mut app.server else { return };
    if v["id"].is_null() {
        return;
    }
    let id = sid(&v["id"]);
    if let Some(Value::Array(a)) = s.data.get_mut("automod") {
        match a.iter_mut().find(|r| sid(&r["id"]) == id) {
            Some(r) => *r = v,
            None => a.push(v),
        }
    }
}

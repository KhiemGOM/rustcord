//! Access: how people join (invite only, or apply and get approved), the age restriction and
//! the rules new members agree to, which share one membership-screening form.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::super::icons::{self, Icon};
use super::form;
use super::{faint, title};
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

const EXAMPLES: [&str; 4] = ["Be civil and respectful", "No spam or self-promotion", "No age-restricted or obscene content", "Help keep things safe"];
const KINDS: [(&str, &str); 3] = [("TEXT_INPUT", "Short answer"), ("PARAGRAPH", "Paragraph"), ("MULTIPLE_CHOICE", "Multiple choice")];

fn norm(ver: &Value, guild: &Value, features: &[String]) -> Value {
    let has = |f: &str| features.iter().any(|x| x == f);
    let fields = ver["form_fields"].as_array().cloned().unwrap_or_default();
    let rules: Vec<Value> = fields.iter().filter(|f| f["field_type"] == "TERMS").flat_map(|f| f["values"].as_array().cloned().unwrap_or_default()).collect();
    let questions: Vec<Value> = fields.iter().filter(|f| f["field_type"] != "TERMS").map(|f| json!({ "label": f["label"], "field_type": f["field_type"], "required": f["required"].as_bool().unwrap_or(true) })).collect();
    json!({
        "join": if has("DISCOVERABLE") { "discover" } else if has("MEMBER_VERIFICATION_MANUAL_APPROVAL") { "apply" } else { "invite" },
        "age": guild["owner_configured_content_level"].as_u64() == Some(3),
        "rules_on": has("MEMBER_VERIFICATION_GATE_ENABLED") && !has("MEMBER_VERIFICATION_MANUAL_APPROVAL"),
        "rules": rules,
        "questions": questions,
    })
}

pub fn saved(app: &mut App, form: &Value) {
    let Some(fields) = form["form_fields"].as_array() else { return };
    let Some(g) = app.server.as_ref().and_then(|s| app.store.guilds.get_mut(&s.guild)) else { return };
    g.features.retain(|f| f != "MEMBER_VERIFICATION_MANUAL_APPROVAL" && f != "MEMBER_VERIFICATION_GATE_ENABLED");
    if form["enabled"].as_bool() == Some(true) {
        let apply = fields.iter().any(|f| f["field_type"] != "TERMS");
        g.features.push(if apply { "MEMBER_VERIFICATION_MANUAL_APPROVAL" } else { "MEMBER_VERIFICATION_GATE_ENABLED" }.into());
    }
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    title(ui, "Access", "");
    let Some(g) = app.store.guilds.get(&gid).cloned() else { return };
    let Some(ver) = app.server_data("verification", format!("/guilds/{gid}/member-verification?with_guild=false")).cloned() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let Some(raw) = app.server_data("guild", format!("/guilds/{gid}")).cloned() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    let community = g.features.iter().any(|f| f == "COMMUNITY");
    let orig = norm(&ver, &raw, &g.features);
    let mut d = form::take(app, "access", &orig);
    ui.set_max_width(660.0);

    ui.label(RichText::new("How can people join your server?").font(semi(16.0)).color(pal().header_primary));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("Keep your server private, or open it up for more people to join.").font(reg(14.0)).color(pal().header_secondary));
        if super::super::settings::link(ui, "Learn More.").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/29729107418519"));
        }
    });
    ui.add_space(16.0);
    let join = d["join"].as_str().unwrap_or("invite").to_string();
    let mut pick = None;
    Frame::new().fill(Color32::from_black_alpha(20)).stroke(Stroke::new(1.0, faint(51))).corner_radius(8).inner_margin(Margin::same(8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let w = (ui.available_width() - 0.0) / 3.0;
            for (id, icon, name, body, on_off) in [
                ("invite", Icon::Lock, "Invite Only", "People can join your server directly with an invite", true),
                ("apply", Icon::Inbox, "Apply to Join", "People must submit an application and be approved to join", true),
                ("discover", Icon::Discover, "Discoverable", "Anyone can join your server directly through Server Discovery", community),
            ] {
                let (r, resp) = ui.allocate_exact_size(vec2(w, 118.0), Sense::click());
                let on = join == id;
                let a = if on_off { 1.0 } else { 0.45 };
                if on {
                    ui.painter().rect(r, CornerRadius::same(6), faint(31), Stroke::new(1.0, faint(51)), egui::StrokeKind::Inside);
                } else if resp.hovered() && on_off {
                    ui.painter().rect_filled(r, CornerRadius::same(6), faint(20));
                }
                icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(r.center().x, r.top() + 22.0), vec2(24.0, 24.0)), pal().header_primary.gamma_multiply(a));
                ui.painter().text(pos2(r.center().x, r.top() + 50.0), Align2::CENTER_CENTER, name, semi(16.0), pal().header_primary.gamma_multiply(a));
                let g = ui.painter().layout(body.into(), reg(12.0), pal().text_normal.gamma_multiply(a), w - 32.0);
                ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.top() + 66.0), g, Color32::WHITE);
                super::super::settings::mark(ui, &format!("join:{name}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(if on_off { egui::CursorIcon::PointingHand } else { egui::CursorIcon::NotAllowed });
                }
                if resp.clicked() && on_off && !on {
                    pick = Some(id);
                }
            }
        });
    });
    if let Some(p) = pick {
        if p == "discover" {
            if let Some(s) = &mut app.server {
                s.page = super::Page::Discovery;
            }
        } else {
            d["join"] = json!(p);
        }
    }
    let join = d["join"].as_str().unwrap_or("invite").to_string();

    form::line(ui);
    let mut age = d["age"].as_bool().unwrap_or(false);
    if super::super::settings::toggle(ui, &mut age, "Age-Restricted Server", "Users will need to confirm they are over the legal age to view the content in this server Learn more.") {
        d["age"] = json!(age);
    }

    if join == "apply" {
        form::line(ui);
        questions(ui, &mut d);
    } else {
        form::line(ui);
        let mut on = d["rules_on"].as_bool().unwrap_or(false);
        if super::super::settings::toggle(ui, &mut on, "Server Rules", "Members must agree to rules before they can chat or interact in the server.") {
            d["rules_on"] = json!(on);
        }
        ui.scope(|ui| {
            if !on {
                ui.set_opacity(0.4);
            }
            ui.add_enabled_ui(on, |ui| rules(ui, &mut d));
        });
    }

    form::put(app, "access", d);
    let ctx = ui.ctx().clone();
    let base = orig.clone();
    form::bar_with(app, &ctx, "access", &orig, move |app, c| {
        let d = form::take(app, "access", &base);
        if c.contains_key("age") {
            let level = if d["age"].as_bool().unwrap_or(false) { 3 } else { 0 };
            app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "owner_configured_content_level": level })), Tag::Server("guild"));
        }
        if c.keys().all(|k| k == "age") {
            return;
        }
        if base["join"] == "discover" && d["join"] != "discover" {
            let feats: Vec<&String> = app.store.guilds.get(&gid).into_iter().flat_map(|g| g.features.iter()).filter(|f| *f != "DISCOVERABLE").collect();
            app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "features": feats })), Tag::Server("guild"));
        }
        let rules: Vec<String> = d["rules"].as_array().into_iter().flatten().filter_map(|r| r.as_str()).map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect();
        let body = if d["join"] == "apply" {
            let qs: Vec<Value> = d["questions"].as_array().into_iter().flatten().filter(|q| q["label"].as_str().is_some_and(|l| !l.trim().is_empty())).map(|q| json!({ "field_type": q["field_type"], "label": q["label"], "required": q["required"], "values": null })).collect();
            if qs.is_empty() {
                app.toast("Add at least one question for people to answer.");
                return;
            }
            json!({ "form_fields": qs, "enabled": true })
        } else {
            let on = d["rules_on"].as_bool().unwrap_or(false);
            if on && rules.is_empty() {
                app.toast("Add at least one rule before turning on Server Rules.");
                return;
            }
            let fields = if rules.is_empty() { json!([]) } else { json!([{ "field_type": "TERMS", "label": "Read and agree to the server rules", "values": rules, "required": true }]) };
            json!({ "form_fields": fields, "enabled": on })
        };
        app.rest("PATCH", format!("/guilds/{gid}/member-verification"), Some(body), Tag::Server("verification"));
    });
}

fn rules(ui: &mut Ui, d: &mut Value) {
    ui.add_space(16.0);
    Frame::new().fill(Color32::from_black_alpha(20)).corner_radius(8).inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        form::caps(ui, "Rules");
        let mut list: Vec<String> = d["rules"].as_array().into_iter().flatten().map(|r| r.as_str().unwrap_or_default().to_string()).collect();
        let mut drop = None;
        for (i, r) in list.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let w = ui.available_width() - 36.0;
                ui.allocate_ui(vec2(w, 40.0), |ui| {
                    let resp = super::super::settings::input(ui, r, "Enter a rule", false);
                    super::super::settings::mark(ui, &format!("rule:{i}"), resp.rect);
                });
                let (x, xr) = ui.allocate_exact_size(vec2(28.0, 40.0), Sense::click());
                icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(x.center(), vec2(16.0, 16.0)), crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &xr)));
                if xr.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if xr.clicked() {
                    drop = Some(i);
                }
            });
            ui.add_space(8.0);
            r.truncate(300);
        }
        if let Some(i) = drop {
            list.remove(i);
        }
        if list.len() < 16 {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
            dashed(ui, r, crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
            icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pos2(r.center().x - 36.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
            ui.painter().text(pos2(r.center().x - 20.0, r.center().y), Align2::LEFT_CENTER, "Add a rule", medium(16.0), pal().header_primary);
            super::super::settings::mark(ui, "button:Add a rule", r);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                list.push(String::new());
            }
        }
        ui.add_space(24.0);
        form::caps(ui, "Example rules");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for ex in EXAMPLES {
                let g = ui.painter().layout_no_wrap(ex.into(), reg(14.0), pal().text_normal);
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 30.0, 36.0), Sense::click());
                ui.painter().rect(r, CornerRadius::same(18), crate::ui::motion::mix(faint(15), faint(31), crate::ui::motion::hot(ui.ctx(), &resp)), Stroke::new(1.0, faint(31)), egui::StrokeKind::Inside);
                ui.painter().galley(pos2(r.left() + 15.0, r.center().y - g.size().y / 2.0), g, pal().text_normal);
                super::super::settings::mark(ui, &format!("example:{ex}"), r);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() && list.len() < 16 && !list.iter().any(|r| r == ex) {
                    list.retain(|r| !r.trim().is_empty());
                    list.push(ex.to_string());
                }
            }
        });
        d["rules"] = json!(list);
    });
}

fn dashed(ui: &Ui, r: Rect, c: Color32) {
    let s = Stroke::new(1.0, c);
    for (a, b) in [(r.left_top(), r.right_top()), (r.left_bottom(), r.right_bottom()), (r.left_top(), r.left_bottom()), (r.right_top(), r.right_bottom())] {
        ui.painter().extend(egui::Shape::dashed_line(&[a, b], s, 6.0, 4.0));
    }
}

fn questions(ui: &mut Ui, d: &mut Value) {
    ui.label(RichText::new("Questions").font(semi(16.0)).color(pal().header_primary));
    ui.add_space(4.0);
    form::small(ui, "People who want to join answer these, and you approve or reject their application before they can chat.");
    ui.add_space(16.0);
    let mut qs: Vec<Value> = d["questions"].as_array().cloned().unwrap_or_default();
    let mut drop = None;
    for (i, q) in qs.iter_mut().enumerate() {
        form::card(ui, |ui| {
            let mut label = q["label"].as_str().unwrap_or_default().to_string();
            let resp = super::super::settings::input(ui, &mut label, "Write a question", false);
            super::super::settings::mark(ui, &format!("question:{i}"), resp.rect);
            label.truncate(300);
            q["label"] = json!(label);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let kind = q["field_type"].as_str().unwrap_or("TEXT_INPUT").to_string();
                let shown = KINDS.iter().find(|k| k.0 == kind).map_or("Short answer", |k| k.1);
                form::dropdown(ui, 220.0, shown, None, None, |ui| {
                    for (k, name) in KINDS {
                        if ui.selectable_label(k == kind, name).clicked() {
                            q["field_type"] = json!(k);
                            ui.close();
                        }
                    }
                });
                let mut req = q["required"].as_bool().unwrap_or(true);
                ui.label(RichText::new("Required").font(medium(14.0)).color(pal().text_normal));
                if super::super::settings::switch(ui, req).clicked() {
                    req = !req;
                    q["required"] = json!(req);
                }
                if form::button(ui, "Remove", false).clicked() {
                    drop = Some(i);
                }
            });
        });
        ui.add_space(8.0);
    }
    if let Some(i) = drop {
        qs.remove(i);
    }
    if qs.len() < 5 {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
        dashed(ui, r, crate::ui::motion::mix(pal().header_secondary, pal().header_primary, crate::ui::motion::hot(ui.ctx(), &resp)));
        icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(pos2(r.center().x - 56.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
        ui.painter().text(pos2(r.center().x - 40.0, r.center().y), Align2::LEFT_CENTER, "Add a question", medium(16.0), pal().header_primary);
        super::super::settings::mark(ui, "button:Add a question", r);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            qs.push(json!({ "label": "", "field_type": "TEXT_INPUT", "required": true }));
        }
    }
    d["questions"] = json!(qs);
}

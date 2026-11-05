//! Enable Community: the pitch page and the three-step setup over it, and once the server is
//! a Community the overview with its rules and updates channels and the way back out.

use egui::{Align2, Color32, CornerRadius, Frame, Id, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use serde_json::{Value, json};

use super::super::App;
use super::super::icons::{self, Icon};
use super::form;
use super::{faint, title};
use crate::net::Tag;
use crate::state::Id as Snow;
use crate::theme::*;

const STEPS: [&str; 3] = ["Safety checks", "Setting up the basics", "Finishing touches"];

fn is_community(app: &App, gid: Snow) -> bool {
    app.store.guilds.get(&gid).is_some_and(|g| g.features.iter().any(|f| f == "COMMUNITY"))
}

pub fn show(app: &mut App, ui: &mut Ui, gid: Snow) {
    form::fresh(ui);
    if is_community(app, gid) {
        overview(app, ui, gid);
    } else {
        pitch(app, ui, gid);
    }
}

fn centered(ui: &mut Ui, text: RichText) {
    ui.vertical_centered(|ui| ui.label(text));
}

fn pitch(app: &mut App, ui: &mut Ui, _gid: Snow) {
    ui.set_max_width(660.0);
    let (art, _) = ui.allocate_exact_size(vec2(660.0, 250.0), Sense::hover());
    for (i, (name, col)) in [("Pokemon GO Paris", Color32::from_rgb(0xe5, 0x3b, 0x3b)), ("Python", Color32::from_rgb(0x4b, 0x8b, 0xbe)), ("Learn Latin", Color32::from_rgb(0x7a, 0x6f, 0x66)), ("r/leagueoflegends", Color32::from_rgb(0xc8, 0x9b, 0x3c)), ("Sneaker Fans", Color32::from_rgb(0xf2, 0x7a, 0x1a)), ("Hogwarts School", Color32::from_rgb(0x2e, 0x4a, 0x86))].into_iter().enumerate() {
        let (x, y) = [(0.12, 0.12), (0.72, 0.12), (0.18, 0.5), (0.8, 0.5), (0.3, 0.84), (0.72, 0.84)][i];
        let c = pos2(art.left() + art.width() * x, art.top() + art.height() * y);
        ui.painter().rect_filled(Rect::from_center_size(pos2(c.x - 44.0, c.y), vec2(22.0, 22.0)), CornerRadius::same(5), col);
        ui.painter().text(pos2(c.x - 28.0, c.y), Align2::LEFT_CENTER, name, semi(12.0), if i % 2 == 0 { Color32::WHITE } else { pal().header_secondary });
    }
    let hub = Rect::from_center_size(art.center(), vec2(96.0, 96.0));
    ui.painter().rect_filled(hub, CornerRadius::same(24), BLURPLE);
    icons::draw(ui.painter(), Icon::BadgeCommunity, hub.shrink(24.0), Color32::WHITE);
    ui.add_space(24.0);
    centered(ui, RichText::new("Are you building a Community?").font(semi(24.0)).color(pal().header_primary));
    ui.add_space(8.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("Convert to a Community Server to access additional administrative tools that help you moderate and grow your server.").font(reg(16.0)).color(pal().text_normal));
        if super::super::settings::link(ui, "Learn more.").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/14078261239831"));
        }
        ui.add_space(16.0);
        if form::button(ui, "Enable Community", true).clicked() {
            app.server.as_mut().unwrap().sub = 1;
            app.server.as_mut().unwrap().draft.remove("community");
        }
    });
    ui.add_space(32.0);
    let (r, _) = ui.allocate_exact_size(vec2(660.0, 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, faint(51));
    ui.add_space(32.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("Community Servers are larger spaces where people with shared interests can come together.").font(reg(14.0)).color(pal().text_normal));
        ui.add_space(8.0);
        let (text, link) = ("Enabling Community does not make your server visible on Server Discovery.", "Learn more here.");
        let w = ui.painter().layout_no_wrap(text.into(), reg(14.0), pal().text_normal).size().x + ui.painter().layout_no_wrap(link.into(), reg(14.0), pal().link).size().x + 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            ui.label(RichText::new(text).font(reg(14.0)).color(pal().text_normal));
            if super::super::settings::link(ui, link).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://support.discord.com/hc/en-us/articles/360047132851"));
            }
        });
    });
    ui.add_space(40.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        for (head, body, ink) in [("Grow your community", "Apply to be in Server Discovery so more people can find your server directly on Discord.", Color32::from_rgb(0x23, 0xa5, 0x5a)), ("Keep members engaged", "Access tools like Server Insights that can better help you moderate and keep your server engaged.", Color32::from_rgb(0x58, 0x65, 0xf2)), ("Stay informed", "Get direct updates about new features built for communities from Discord.", RED)] {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(6), ink.gamma_multiply(0.85));
                ui.add_space(16.0);
                ui.label(RichText::new(head).font(semi(16.0)).color(pal().header_primary));
                ui.add_space(8.0);
                ui.label(RichText::new(body).font(reg(14.0)).line_height(Some(18.0)).color(pal().text_normal));
            });
        }
    });
}

fn overview(app: &mut App, ui: &mut Ui, gid: Snow) {
    title(ui, "Community Overview", "Community Servers are larger spaces where people with shared interests can come together.");
    let Some(raw) = app.server_data("guild", format!("/guilds/{gid}")).cloned() else {
        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
        return;
    };
    ui.set_max_width(660.0);
    let orig = json!({ "rules_channel_id": raw["rules_channel_id"], "public_updates_channel_id": raw["public_updates_channel_id"] });
    let mut d = form::take(app, "community_over", &orig);
    for (key, head, desc) in [("rules_channel_id", "Rules or Guidelines Channel", "Community Servers must have clearly posted server rules and/or guidelines for members. Please select the channel that hosts this."), ("public_updates_channel_id", "Community Updates Channel", "Community Servers are required to have a Community Updates channel where Discord can send important community server updates.")] {
        super::super::settings::h5(ui, head);
        form::small(ui, desc);
        ui.add_space(8.0);
        let cur = d[key].as_str().and_then(|s| s.parse::<Snow>().ok());
        if let Some(Some(c)) = form::channel(ui, app, gid, 360.0, cur, false, None) {
            d[key] = json!(c.to_string());
        }
        form::line(ui);
    }
    super::super::settings::h5(ui, "Server Insights");
    form::small(ui, "Server Insights is hosted on a separate website so that it\u{2019}s easier to share links and browse cross platform.");
    ui.add_space(8.0);
    if form::button(ui, "Open Server Insights", false).clicked() {
        super::super::settings::open_web(&format!("/developers/servers/{gid}/analytics"));
    }
    form::line(ui);
    super::super::settings::h5(ui, "Server Discovery");
    form::small(ui, "Apply to be in Server Discovery so more people can find your server directly on Discord.");
    ui.add_space(8.0);
    if form::button(ui, "Open Discovery Settings", false).clicked() {
        super::super::settings::open_web(&format!("/channels/{gid}"));
    }
    form::line(ui);
    super::super::settings::h5(ui, "Disable Community");
    form::small(ui, "This will remove specific features for Community Servers, like Server Discovery and Server Insights.");
    ui.add_space(8.0);
    if super::super::settings::danger(ui, "Disable Community", false).clicked() {
        let features: Vec<Value> = app.store.guilds.get(&gid).map(|g| g.features.iter().filter(|f| *f != "COMMUNITY" && *f != "DISCOVERABLE").map(|f| json!(f)).collect()).unwrap_or_default();
        app.confirm = Some(super::super::Confirm::new("Disable Community", "Are you sure you want to disable Community?", "Disable Community", move |app: &mut App| {
            app.rest("PATCH", format!("/guilds/{gid}"), Some(json!({ "features": features, "rules_channel_id": null, "public_updates_channel_id": null })), Tag::Server("guild"));
        }));
    }
    form::put(app, "community_over", d);
    let ctx = ui.ctx().clone();
    form::bar(app, &ctx, "community_over", "guild", &orig, format!("/guilds/{gid}"), Value::Object);
}

fn checkbox(ui: &mut Ui, on: &mut bool, label: &str) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let b = Rect::from_min_size(pos2(r.left(), r.center().y - 12.0), vec2(24.0, 24.0));
    if *on {
        ui.painter().rect_filled(b, CornerRadius::same(4), BLURPLE);
        icons::draw(ui.painter(), Icon::Check, b.shrink(4.0), Color32::WHITE);
    } else {
        ui.painter().rect_stroke(b, CornerRadius::same(4), Stroke::new(2.0, pal().header_secondary), egui::StrokeKind::Inside);
    }
    ui.painter().text(pos2(b.right() + 14.0, r.center().y), Align2::LEFT_CENTER, label, reg(16.0), pal().text_normal);
    super::super::settings::mark(ui, &format!("check:{label}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *on = !*on;
    }
}

fn block(ui: &mut Ui, head: &str, desc: &str) {
    ui.label(RichText::new(head).font(semi(16.0)).color(pal().header_primary));
    ui.add_space(4.0);
    ui.label(RichText::new(desc).font(reg(14.0)).line_height(Some(18.0)).color(pal().header_secondary));
    ui.add_space(8.0);
}

/// The setup over the page: a blurple panel with the steps, and the step's form beside it.
pub fn modal(app: &mut App, ctx: &egui::Context) {
    let Some(s) = app.server.as_ref() else { return };
    let (gid, step) = (s.guild, s.sub);
    if step == 0 || s.page != super::Page::Community {
        return;
    }
    let (level, filter, rules, updates, notif) = app.store.guilds.get(&gid).map_or((0, 0, 0, 0, 0), |g| (0, 0, g.rules, 0, g.default_notif));
    let raw = s.data.get("guild").cloned().unwrap_or(Value::Null);
    let init = json!({
        "email": raw["verification_level"].as_u64().unwrap_or(level) >= 1,
        "scan": raw["explicit_content_filter"].as_u64().unwrap_or(filter) >= 2,
        "rules": if rules != 0 { json!(rules.to_string()) } else { raw["rules_channel_id"].clone() },
        "updates": if updates != 0 { json!(updates.to_string()) } else { raw["public_updates_channel_id"].clone() },
        "notif": 1,
        "agree": false,
        "was": notif,
    });
    let mut d = form::take(app, "community", &init);
    let (mut next, mut back, mut close) = (false, false, false);
    let size = vec2(681.0, 628.0);
    let frame = Frame::new().fill(pal().bg_secondary).corner_radius(12).stroke(Stroke::new(1.0, faint(51)));
    let m = crate::ui::motion::Modal::new(Id::new("community_setup")).backdrop_color(Color32::from_black_alpha(184)).frame(frame).show(ctx, |ui| {
        let (all, _) = ui.allocate_exact_size(size, Sense::hover());
        let left = Rect::from_min_size(all.min, vec2(240.0, size.y));
        let mut m = egui::Mesh::default();
        let (t, b) = (Color32::from_rgb(0x4e, 0x28, 0xf2), Color32::from_rgb(0x63, 0x66, 0xe9));
        m.colored_vertex(left.left_top(), t);
        m.colored_vertex(left.right_top(), t);
        m.colored_vertex(left.right_bottom(), b);
        m.colored_vertex(left.left_bottom(), b);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        ui.painter().add(m);
        let mut ty = left.top() + 24.0;
        for line in ["Let's set up your", "Community", "Server."] {
            ty += ui.painter().text(pos2(left.left() + 24.0, ty), Align2::LEFT_TOP, line, bold(24.0), Color32::WHITE).height() + 4.0;
        }
        for (i, name) in STEPS.iter().enumerate() {
            let n = (i + 1) as u8;
            let y = left.top() + 290.0 + i as f32 * 40.0;
            let on = n == step.min(3);
            let c = pos2(left.left() + 36.0, y);
            ui.painter().circle_filled(c, 12.0, if on { Color32::WHITE } else { Color32::from_white_alpha(46) });
            ui.painter().text(c, Align2::CENTER_CENTER, n.to_string(), medium(12.0), if on { BLURPLE } else { Color32::WHITE });
            ui.painter().text(pos2(left.left() + 58.0, y), Align2::LEFT_CENTER, name, if on { bold(16.0) } else { medium(16.0) }, Color32::WHITE);
        }
        let right = Rect::from_min_max(pos2(left.right() + 32.0, all.top()), pos2(all.right() - 32.0, all.bottom()));
        let x = Rect::from_center_size(pos2(all.right() - 28.0, all.top() + 28.0), vec2(32.0, 32.0));
        let xr = ui.interact(x, ui.id().with("x"), Sense::click());
        icons::draw(ui.painter(), Icon::Close, x.shrink(6.0), if xr.hovered() { pal().header_primary } else { pal().header_secondary });
        close = xr.clicked();
        let badge = Rect::from_center_size(pos2(right.center().x, all.top() + 70.0), vec2(60.0, 60.0));
        ui.painter().rect_filled(badge, CornerRadius::same(16), Color32::from_gray(0x2b));
        icons::draw(ui.painter(), Icon::Shield, badge.shrink(14.0), Color32::WHITE);
        let mut body = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(pos2(right.left(), all.top() + 118.0), pos2(right.right(), all.bottom() - 72.0))));
        body.set_width(right.width());
        let (head, desc) = match step {
            1 => ("Keep your community safe", "To ensure user safety, Community Servers must have the following moderation settings enabled."),
            2 => ("Set up the basics", "Community Servers need a place for rules and a place for Discord to send updates."),
            3 => ("Finishing touches", "Almost there. Pick how members get notified by default and agree to the guidelines."),
            _ => ("Your server is now a Community Server!", "Thanks for abiding by our Community Guidelines and keeping your server safe!"),
        };
        body.vertical_centered(|ui| {
            ui.label(RichText::new(head).font(semi(24.0)).color(pal().header_primary));
            ui.add_space(8.0);
            ui.label(RichText::new(desc).font(reg(16.0)).color(pal().text_normal));
        });
        body.add_space(24.0);
        let mut ready = true;
        match step {
            1 => {
                let (mut email, mut scan) = (d["email"].as_bool().unwrap_or(false), d["scan"].as_bool().unwrap_or(false));
                block(&mut body, "Verified email required.", "To minimize spam, members of the server must have a verified email before they can send messages. This does not apply to members with roles.");
                checkbox(&mut body, &mut email, "Verified email required.");
                body.add_space(24.0);
                block(&mut body, "Explicit Media Content Filter", "Discord will automatically scan and delete media sent in this server that contain explicit content (excludes age-restricted channels).");
                checkbox(&mut body, &mut scan, "Scan media content from all members.");
                d["email"] = json!(email);
                d["scan"] = json!(scan);
                ready = email && scan;
            }
            2 => {
                for (key, head, desc) in [("rules", "Rules or guidelines channel", "Community Servers must have clearly posted server rules and/or guidelines for members. Please select the channel that hosts this."), ("updates", "Community Updates Channel", "Community Servers are required to have a Community Updates channel where Discord can send important community server updates.")] {
                    block(&mut body, head, desc);
                    let cur = d[key].as_str().and_then(|s| s.parse::<Snow>().ok());
                    if let Some(Some(c)) = form::channel(&mut body, app, gid, right.width(), cur, false, None) {
                        d[key] = json!(c.to_string());
                    }
                    body.add_space(24.0);
                }
                ready = d["rules"].is_string() && d["updates"].is_string();
            }
            3 => {
                block(&mut body, "Default Notification Settings", "Because Community Servers tend to have a lot of people talking at once, defaulting Server Notifications for members to Mentions Only can help reduce members leaving. Members can always override this option.");
                let mut n = d["notif"].as_u64().unwrap_or(1) as u8;
                if super::super::settings::radio(&mut body, &mut n, &[(1u8, "Only @mentions", ""), (0, "All Messages", "")]) {
                    d["notif"] = json!(n);
                }
                body.add_space(8.0);
                let mut agree = d["agree"].as_bool().unwrap_or(false);
                checkbox(&mut body, &mut agree, "I agree to follow Discord's Community Guidelines.");
                if super::super::settings::link(&mut body, "View our Community Guidelines").clicked() {
                    body.ctx().open_url(egui::OpenUrl::new_tab("https://discord.com/guidelines"));
                }
                d["agree"] = json!(agree);
                ready = agree;
            }
            5 => {
                body.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
            }
            _ => {}
        }
        if step != 5 {
            let label = match step {
                3 => "Finish Setup",
                4 => "Done",
                _ => "Next",
            };
            let g = ui.painter().layout_no_wrap(label.into(), medium(16.0), Color32::WHITE);
            let br = Rect::from_min_size(pos2(right.right() - g.size().x - 40.0, all.bottom() - 62.0), vec2(g.size().x + 40.0, 40.0));
            let resp = ui.interact(br, ui.id().with("next"), if ready { Sense::click() } else { Sense::hover() });
            let fill = if ready { if resp.hovered() { Color32::from_rgb(0x1a, 0x6b, 0x38) } else { Color32::from_rgb(0x24, 0x80, 0x46) } } else { Color32::from_rgb(0x0b, 0x4a, 0x26) };
            ui.painter().rect_filled(br, CornerRadius::same(8), fill);
            ui.painter().text(br.center(), Align2::CENTER_CENTER, label, medium(16.0), Color32::WHITE.gamma_multiply(if ready { 1.0 } else { 0.5 }));
            super::super::settings::mark(ui, &format!("plate:{label}"), br);
            if resp.hovered() && ready {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            next = resp.clicked() && ready;
            if step == 2 || step == 3 {
                let bl = Rect::from_min_size(pos2(right.left(), all.bottom() - 62.0), vec2(60.0, 40.0));
                let bresp = ui.interact(bl, ui.id().with("back"), Sense::click());
                ui.painter().text(bl.left_center(), Align2::LEFT_CENTER, "Back", medium(16.0), pal().header_primary);
                back = bresp.clicked();
            }
        }
    });
    form::raise(ctx, "community_setup");
    form::put(app, "community", d.clone());
    let sub = &mut app.server.as_mut().unwrap().sub;
    if close || m.should_close() {
        *sub = 0;
        return;
    }
    if back {
        *sub -= 1;
    }
    if next {
        match step {
            1 | 2 => *sub += 1,
            3 => {
                let features: Vec<Value> = app.store.guilds.get(&gid).map(|g| g.features.iter().map(|f| json!(f)).chain(std::iter::once(json!("COMMUNITY"))).collect()).unwrap_or_default();
                let body = json!({
                    "features": features,
                    "verification_level": raw["verification_level"].as_u64().unwrap_or(0).max(1),
                    "explicit_content_filter": 2,
                    "rules_channel_id": d["rules"],
                    "public_updates_channel_id": d["updates"],
                    "default_message_notifications": d["notif"],
                });
                *sub = 5;
                let s = app.server.as_mut().unwrap();
                s.saving = true;
                app.rest("PATCH", format!("/guilds/{gid}"), Some(body), Tag::Server("guild"));
            }
            _ => app.server.as_mut().unwrap().sub = 0,
        }
    }
}

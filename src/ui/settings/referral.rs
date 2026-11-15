//! Share Nitro with Friends: the three trials a Nitro member can hand out, who has one, and
//! the dialog that picks friends for the rest.

use egui::{Align2, Frame, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::{card, end, lead, link, primary, refetch, remote, section};
use crate::net::Tag;
use crate::state::{Id, sid};
use crate::theme::*;
use crate::ui::{App, avatar, modal_button};

const TRIALS: usize = 3;
const PENDING: u64 = 2;
const HELP: &str = "https://support.discord.com/hc/en-us/articles/22567595858327";

#[derive(Default)]
pub struct Picker {
    query: String,
    asked: String,
    picked: Vec<Id>,
}

/// The banner's line for where the three trials stand.
pub fn blurb(eligible: bool, sent: usize, redeemed: usize) -> &'static str {
    if !eligible {
        "At this time, none of your friends qualify for the 2-week Nitro Trial."
    } else if sent >= TRIALS && redeemed >= TRIALS {
        "You've used all your trials! More are coming soon. If you're feeling generous, you could always gift Nitro."
    } else if sent >= TRIALS {
        "All your trials have been sent. Your friends have 10 days to accept before the trial is returned to you."
    } else {
        "Send three friends a 2-week Nitro trial and become the MVP of the group chat."
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.keyed.iter().any(|(k, v)| k.starts_with("referral_sent:") && v.is_some()) {
        app.keyed.retain(|k, _| !k.starts_with("referral_sent:"));
        refetch(app, "referrals");
    }
    let Some(Ok(info)) = remote(app, "referrals", "/users/@me/referrals/eligibility").cloned() else { return };
    let sent: Vec<Id> = info["sent_user_ids"].as_array().into_iter().flatten().map(sid).collect();
    let redeemed = info["recipient_status"].as_object().map_or(0, |m| m.values().filter(|v| v.as_u64().is_some_and(|s| s != PENDING)).count());
    let eligible = info["has_eligible_friends"].as_bool().unwrap_or(true);
    let left = info["referrals_remaining"].as_u64().unwrap_or(0) as usize;
    section(ui, "Share Nitro with Friends");
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for n in 0..TRIALS {
                let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                match sent.get(n).and_then(|u| app.store.user(*u)) {
                    Some(u) => avatar(ui, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), &u.avatar_url()),
                    None => {
                        ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(1.0, pal().border_normal));
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, (n + 1).to_string(), semi(12.0), pal().header_secondary);
                    }
                }
            }
            ui.add_space(8.0);
            ui.add(egui::Label::new(RichText::new(blurb(eligible, sent.len(), redeemed)).font(reg(14.0)).color(pal().header_secondary)).wrap());
        });
    });
    lead(ui);
    ui.horizontal(|ui| {
        if link(ui, "Learn more").clicked() {
            let _ = open::that_detached(HELP);
        }
    });
    end(ui, 16.0);
    if eligible && left > 0 && primary(ui, "Select Friends").clicked() {
        app.referral = Some(Picker::default());
    }
}

fn ask(app: &mut App, q: &str) {
    app.keyed.insert(format!("referral_users:{q}"), None);
    app.rest("POST", "/users/@me/referrals/eligible-users".into(), Some(json!({ "index": 0, "limit": 10, "search_query": q })), Tag::Keyed(format!("referral_users:{q}")));
}

pub fn modal(app: &mut App, ctx: &egui::Context) {
    let Some(mut p) = app.referral.take() else { return };
    let left = match app.remote.get("referrals") {
        Some(Some(Ok(v))) => v["referrals_remaining"].as_u64().unwrap_or(0) as usize,
        _ => 0,
    };
    if p.asked != p.query || !app.keyed.contains_key(&format!("referral_users:{}", p.query)) {
        p.asked = p.query.clone();
        if !app.keyed.contains_key(&format!("referral_users:{}", p.query)) {
            ask(app, &p.query.clone());
        }
    }
    let users: Option<Vec<Value>> = match app.keyed.get(&format!("referral_users:{}", p.query)) {
        Some(Some(Ok(v))) => Some(v["users"].as_array().cloned().unwrap_or_default()),
        Some(Some(Err(_))) => Some(Vec::new()),
        _ => None,
    };
    let mut send = false;
    let closed = crate::ui::create::modal(ctx, "share_nitro", Some("Share Nitro with Friends"), |ui| {
        ui.label(RichText::new(format!("You've got {left} trials to share with friends!")).font(reg(16.0)).color(pal().header_secondary));
        ui.add_space(16.0);
        crate::ui::create::field(ui, &mut p.query, "Search", Some(crate::ui::icons::Icon::Search), 1, None);
        ui.add_space(8.0);
        let rows = users.as_ref().map_or(0, Vec::len);
        let tall = if rows == 0 { 56.0 } else { (rows as f32 * 44.0).min(264.0) };
        egui::ScrollArea::vertical().max_height(tall).auto_shrink([false, false]).show(ui, |ui| match &users {
            None => {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
                ui.add_space(16.0);
            }
            Some(list) if list.is_empty() => {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("None of your friends are eligible for a trial right now.").color(pal().header_secondary)));
                ui.add_space(16.0);
            }
            Some(list) => {
                for u in list {
                    let id = sid(&u["id"]);
                    let on = p.picked.contains(&id);
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 8, pal().control);
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let face = Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(32.0, 32.0));
                    let pic = match u["avatar"].as_str() {
                        Some(h) => format!("{}/avatars/{id}/{h}.png?size=64", crate::api::cdn()),
                        None => format!("{}/embed/avatars/{}.png", crate::api::cdn(), (id >> 22) % 6),
                    };
                    avatar(ui, face, &pic);
                    let name = u["global_name"].as_str().or(u["username"].as_str()).unwrap_or("");
                    crate::ui::text::paint(ui.painter(), pos2(r.left() + 52.0, r.center().y), Align2::LEFT_CENTER, name, medium(16.0), pal().header_primary, r.width() - 96.0);
                    let b = Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(20.0, 20.0));
                    if on {
                        ui.painter().rect_filled(b, 4, pal().blurple);
                        crate::ui::icons::draw(ui.painter(), crate::ui::icons::Icon::Check, b.shrink(2.0), egui::Color32::WHITE);
                    } else {
                        ui.painter().rect_stroke(b, 4, Stroke::new(1.0, pal().header_secondary), egui::StrokeKind::Inside);
                    }
                    super::mark(ui, &format!("referral:user:{id}"), r);
                    if resp.clicked() {
                        if on {
                            p.picked.retain(|x| *x != id);
                        } else if p.picked.len() < left {
                            p.picked.push(id);
                        }
                    }
                }
            }
        });
        ui.add_space(16.0);
        Frame::NONE.show(ui, |ui| {
            let w = ui.available_width();
            let go = ui.add_enabled_ui(!p.picked.is_empty(), |ui| modal_button(ui, "Share Nitro", Some(pal().blurple), w)).inner;
            super::mark(ui, "referral:send", go.rect);
            send = go.clicked();
        });
    });
    if send {
        for u in &p.picked {
            app.keyed.insert(format!("referral_sent:{u}"), None);
            app.rest("POST", format!("/users/@me/referrals/{u}"), None, Tag::Keyed(format!("referral_sent:{u}")));
        }
        app.keyed.retain(|k, _| !k.starts_with("referral_users:"));
    } else if !closed {
        app.referral = Some(p);
    }
}

#[cfg(test)]
mod tests {
    use super::blurb;

    #[test]
    fn the_banner_says_where_the_three_trials_stand() {
        assert!(blurb(false, 0, 0).starts_with("At this time"));
        assert!(blurb(true, 1, 0).starts_with("Send three friends"));
        assert!(blurb(true, 3, 1).starts_with("All your trials have been sent"));
        assert!(blurb(true, 3, 3).starts_with("You've used all your trials"));
    }
}

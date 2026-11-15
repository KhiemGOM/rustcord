//! The Orb Wallet: the popout under the Shop's balance chip, with the balance, a card that
//! points at the Shop or Nitro, the challenges that pay Orbs and what was redeemed so far.

use egui::{Align2, Color32, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use super::{get, want};
use crate::theme::*;
use crate::ui::icons::{self, Icon};
use crate::ui::{App, modal_button};

const COMPLETED: u64 = 2;
const CLAIMED: u64 = 3;
const SHOP_CARD_FROM: u64 = 1400;
const TO_SHOP: [&str; 2] = ["1531422538125676707", "1531422538125676708"];
const TO_NITRO: &str = "1531422538125676709";
pub const CHALLENGES: &str = "/users/@me/achievements/list?reward_type=orbs";
const HELP: &str = "https://support.discord.com/hc/articles/30593690165783-Discord-Orbs-FAQ#h_01M3WB2TWDD7WVN8ETJCW2PKDM";

pub enum Go {
    Shop,
    Orbs,
    Quests,
    Nitro,
    Claim(String),
    Retry,
}

pub fn enabled(app: &App) -> bool {
    app.store.experiments.as_ref().is_some_and(|e| e.contains(&crate::ui::favorites::hashed("2026-07-orbchievements")))
}

pub fn fetch(app: &mut App) {
    if enabled(app) {
        want(app, "orb_challenges", CHALLENGES);
        want(app, "orb_redeemed", "/users/@me/virtual-currency/total-redeemed");
    }
}

/// Which card sits under the balance: the Shop's for Nitro members who can afford something,
/// Nitro's for everyone else.
pub fn card(nitro: bool, balance: u64) -> Option<bool> {
    if nitro { (balance >= SHOP_CARD_FROM).then_some(true) } else { Some(false) }
}

fn times(m: f64) -> String {
    if m.fract() == 0.0 { format!("{m:.0}") } else { format!("{m}") }
}

fn pill(ui: &mut Ui, text: &str) {
    let g = ui.painter().layout_no_wrap(text.into(), semi(12.0), Color32::WHITE);
    let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 30.0, 20.0), Sense::hover());
    ui.painter().rect_filled(r, 10, Color32::from_rgb(0xb7, 0x3c, 0xc8));
    icons::draw(ui.painter(), Icon::Nitro, Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), vec2(12.0, 12.0)), Color32::WHITE);
    ui.painter().galley(pos2(r.left() + 22.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
}

fn wide(ui: &mut Ui, icon: Icon, text: &str, w: f32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click());
    let fill = crate::ui::motion::mix(pal().control, pal().control_hover, crate::ui::motion::hot(ui.ctx(), &resp));
    ui.painter().rect(r, 8, fill, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    let g = ui.painter().layout_no_wrap(text.into(), medium(16.0), pal().header_primary);
    let x = r.center().x - (g.size().x + 24.0) / 2.0;
    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(x + 8.0, r.center().y), vec2(16.0, 16.0)), pal().header_primary);
    ui.painter().galley(pos2(x + 24.0, r.center().y - g.size().y / 2.0), g, pal().header_primary);
    crate::ui::settings::mark(ui, &format!("wallet:{text}"), r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn challenge(ui: &mut Ui, c: &Value, busy: bool, go: &mut Option<Go>) {
    let id = c["achievement_identifier"].as_str().unwrap_or("");
    let done = c["status"].as_u64() == Some(COMPLETED);
    let def = c["achievement_definition_id"].as_str().unwrap_or("");
    let leads = !done && (TO_SHOP.contains(&def) || def == TO_NITRO);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), if leads { Sense::click() } else { Sense::hover() });
    if leads && resp.hovered() {
        ui.painter().rect_filled(r, 8, pal().control);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let dot = Rect::from_center_size(pos2(r.left() + 28.0, r.center().y), vec2(40.0, 40.0));
    ui.painter().circle_filled(dot.center(), 20.0, if done { pal().green.gamma_multiply(0.2) } else { pal().control });
    icons::draw(ui.painter(), if done { Icon::Check } else { Icon::Gem }, Rect::from_center_size(dot.center(), vec2(20.0, 20.0)), if done { pal().green } else { pal().header_secondary });
    let x = dot.right() + 12.0;
    crate::ui::text::paint(ui.painter(), pos2(x, r.center().y - 10.0), Align2::LEFT_CENTER, c["title"].as_str().unwrap_or(""), medium(14.0), pal().text_normal, r.right() - x - 76.0);
    let amount = c["reward_amount"].as_u64().unwrap_or(0);
    let verb = ui.painter().text(pos2(x, r.center().y + 10.0), Align2::LEFT_CENTER, if done { "Claim" } else { "Earn" }, reg(14.0), pal().text_muted);
    icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(pos2(verb.right() + 10.0, verb.center().y), vec2(12.0, 12.0)), pal().header_secondary);
    ui.painter().text(pos2(verb.right() + 18.0, verb.center().y), Align2::LEFT_CENTER, format!("{amount} Orbs"), reg(14.0), pal().text_muted);
    crate::ui::settings::mark(ui, &format!("wallet:challenge:{id}"), r);
    if done || busy {
        let b = Rect::from_min_size(pos2(r.right() - 72.0, r.center().y - 16.0), vec2(64.0, 32.0));
        let br = ui.interact(b, ui.id().with(("wallet_claim", id)), Sense::click());
        ui.painter().rect_filled(b, 8, if br.hovered() { pal().blurple.gamma_multiply(0.85) } else { pal().blurple });
        ui.painter().text(b.center(), Align2::CENTER_CENTER, if busy { "..." } else { "Claim" }, medium(14.0), Color32::WHITE);
        crate::ui::settings::mark(ui, &format!("wallet:claim:{id}"), b);
        if br.clicked() && !busy {
            *go = Some(Go::Claim(id.to_string()));
        }
    } else if leads {
        icons::draw(ui.painter(), Icon::ChevronRight, Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(16.0, 16.0)), pal().text_muted);
        if resp.clicked() {
            *go = Some(if def == TO_NITRO { Go::Nitro } else { Go::Shop });
        }
    }
}

pub fn body(app: &App, ui: &mut Ui, balance: u64) -> Option<Go> {
    let mut go = None;
    let nitro = app.store.me_raw["premium_type"].as_u64() == Some(2);
    let data = get(app, "orb_challenges");
    let failed = matches!(app.remote.get("orb_challenges"), Some(Some(Err(_))));
    let claims = || app.keyed.iter().filter_map(|(k, v)| k.strip_prefix("orb_claim:").map(|id| (id, v)));
    let claim_failed = claims().any(|(_, v)| matches!(v, Some(Err(_))));
    let claiming = claims().find(|(_, v)| v.is_none()).map(|(id, _)| id.to_string());
    let gated = data.and_then(|d| d["nitro_gated_orb_multiplier"].as_f64());
    ui.set_width(338.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.add_space(16.0);
    Frame::new().inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 24.0, pal().control);
            icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(r.center(), vec2(28.0, 28.0)), pal().header_primary);
            ui.label(RichText::new(balance.to_string()).font(display(28.0)).color(pal().header_primary));
        });
        let hidden = ui.data(|d| d.get_temp::<bool>(egui::Id::new("orb_card_closed"))).unwrap_or(false);
        let which = card(nitro, balance).filter(|shop| !hidden && (*shop || gated.is_some()));
        if let Some(shop) = which {
            ui.add_space(16.0);
            Frame::new().fill(pal().control).stroke(Stroke::new(1.0, pal().border_muted)).corner_radius(16).inner_margin(16).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let top = ui.max_rect();
                let x = Rect::from_min_size(pos2(top.right() - 12.0, top.top() - 12.0), vec2(24.0, 24.0));
                let xr = ui.interact(x, ui.id().with("orb_card_x"), Sense::click());
                icons::draw(ui.painter(), Icon::Close, x.shrink(4.0), if xr.hovered() { pal().header_primary } else { pal().header_secondary });
                if xr.on_hover_text("Close").clicked() {
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("orb_card_closed"), true));
                }
                let (title, sub, button) = if shop {
                    ("Your Orbs are ready to redeem".to_string(), "You've got enough to grab something in the Shop.".to_string(), "Redeem Orbs")
                } else {
                    ("Unlock more Orbs".to_string(), format!("Subscribe to grow your stash with monthly Orbs and {}x rewards.", times(gated.unwrap_or(1.0))), "Get Nitro")
                };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    let (g, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                    ui.painter().rect_filled(g, 12, pal().bg_tertiary);
                    icons::draw(ui.painter(), if shop { Icon::Shop } else { Icon::Nitro }, g.shrink(14.0), pal().header_primary);
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() - 12.0);
                        ui.label(RichText::new(title).font(bold(16.0)).color(pal().text_normal));
                        ui.add_space(4.0);
                        ui.label(RichText::new(sub).font(medium(12.0)).color(pal().text_normal));
                    });
                });
                ui.add_space(16.0);
                let w = ui.available_width();
                let b = modal_button(ui, button, shop.then_some(pal().blurple).or(Some(Color32::from_rgb(0xb7, 0x3c, 0xc8))), w);
                crate::ui::settings::mark(ui, "wallet:card", b.rect);
                if b.clicked() {
                    go = Some(if shop { Go::Orbs } else { Go::Nitro });
                }
            });
        }
    });
    ui.add_space(16.0);
    let line = ui.cursor().top();
    ui.painter().hline(ui.max_rect().x_range(), line, Stroke::new(1.0, pal().border_subtle));
    Frame::new().inner_margin(Margin { left: 8, right: 8, top: 16, bottom: 0 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if failed || claim_failed {
            Frame::new().fill(pal().control).corner_radius(8).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let text = if claim_failed { "Something went wrong claiming this. Please try again." } else { "Something went wrong loading Challenges. Try again" };
                let r = ui.add(egui::Label::new(RichText::new(text).font(reg(14.0)).color(pal().text_normal)).sense(Sense::click()));
                if r.clicked() && !claim_failed {
                    go = Some(Go::Retry);
                }
            });
            ui.add_space(16.0);
        }
        let list: Vec<&Value> = data.and_then(|d| d["achievements"].as_array()).into_iter().flatten().filter(|c| c["status"].as_u64() != Some(CLAIMED)).collect();
        if data.is_none() && !failed {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| crate::ui::motion::spinner(ui, 24.0, pal().text_muted));
            ui.add_space(24.0);
        } else if !list.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Active Challenges").font(medium(12.0)).color(pal().header_secondary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if let Some(m) = gated {
                        pill(ui, &if nitro { format!("{}x", times(m)) } else { format!("Unlock {}x", times(m)) });
                    }
                });
            });
            ui.add_space(8.0);
            for (i, c) in list.iter().enumerate() {
                challenge(ui, c, claiming.is_some() && claiming.as_deref() == c["achievement_identifier"].as_str(), &mut go);
                if i + 1 < list.len() {
                    let y = ui.cursor().top();
                    ui.painter().hline(ui.max_rect().x_range().shrink(8.0), y, Stroke::new(1.0, pal().border_subtle));
                }
            }
            ui.add_space(8.0);
        } else if !failed {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("You've cleared them all").font(medium(14.0)).color(pal().text_normal));
                ui.label(RichText::new("New challenges land soon. Your Orbs are ready when you are.").font(reg(12.0)).color(pal().header_secondary));
            });
            ui.add_space(32.0);
        }
    });
    let line = ui.cursor().top();
    ui.painter().hline(ui.max_rect().x_range(), line, Stroke::new(1.0, pal().border_subtle));
    ui.add_space(16.0);
    Frame::new().inner_margin(Margin::symmetric(16, 0)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let w = (ui.available_width() - 12.0) / 2.0;
            if wide(ui, Icon::Quests, "Quests", w) {
                go = Some(Go::Quests);
            }
            if wide(ui, Icon::Shop, "Shop", w) {
                go = Some(Go::Shop);
            }
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if let Some(n) = get(app, "orb_redeemed").and_then(|v| v["total_redeemed"].as_u64()) {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                icons::draw(ui.painter(), Icon::Trophy, r, pal().text_muted);
                ui.label(RichText::new(format!("{} Orbs redeemed all time", crate::ui::chatinvite::thousands(n))).font(reg(12.0)).color(pal().text_muted));
                ui.label(RichText::new("•").font(reg(12.0)).color(pal().text_muted));
            }
            let r = ui.add(egui::Label::new(RichText::new("Need help?").font(reg(12.0)).color(pal().link)).sense(Sense::click()));
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(HELP));
            }
        });
    });
    ui.add_space(16.0);
    go
}

#[cfg(test)]
mod tests {
    use super::card;

    #[test]
    fn the_card_under_the_balance_is_the_shops_for_members_with_enough_orbs() {
        assert_eq!(card(true, 1400), Some(true));
        assert_eq!(card(true, 1399), None);
        assert_eq!(card(false, 5000), Some(false));
    }
}

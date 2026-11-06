use egui::{Align2, Color32, CornerRadius, Frame, Id, Margin, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::req::Invoice;
use super::{BASIC_MONTH, BASIC_YEAR, GIFT_STYLES, NITRO_MONTH, NITRO_YEAR, Buy, Step, What};
use crate::state::Id as Uid;
use crate::theme::*;
use crate::ui::App;
use crate::ui::icons::{self, Icon};
use crate::ui::settings::{billing, mark};
use crate::ui::shop::product;

const SPACE_8: f32 = 8.0;
const SPACE_16: f32 = 16.0;
const SPACE_24: f32 = 24.0;
const SPACE_32: f32 = 32.0;
const SMALL_MODAL: f32 = 440.0;
const LARGE_MODAL: f32 = 680.0;
const HEAD: f32 = 64.0;
const FOOT: f32 = 72.0;
const ROW: f32 = 56.0;
const ART: f32 = 96.0;
const GIFT_ROWS: f32 = 160.0;
const RADIUS: u8 = 12;

enum Act {
    Close,
    Back,
    Purchase,
    Source(String),
    Orbs,
    Gift(bool),
    To(Uid),
    Style(u8),
    Qty(i32),
    Plan(String),
    Waiver,
    AddCard,
    Billing,
    Manage,
}

fn button(ui: &mut Ui, r: Rect, label: &str, fill: Color32, ink: Color32, on: bool, key: &str) -> bool {
    let resp = ui.interact(r, Id::new(("buy_button", key)), if on { Sense::CLICK } else { Sense::hover() });
    let fill = if !on { fill.gamma_multiply(0.5) } else if resp.hovered() { fill.gamma_multiply(0.85) } else { fill };
    ui.painter().rect_filled(r, CornerRadius::same(8), fill);
    crate::ui::text::paint(ui.painter(), r.center(), Align2::CENTER_CENTER, label, medium(14.0), if on { ink } else { ink.gamma_multiply(0.6) }, r.width() - SPACE_16);
    if on && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    mark(ui, &format!("buy:{key}"), r);
    on && resp.clicked()
}

fn label(ui: &mut Ui, text: &str) {
    ui.add_space(SPACE_16);
    ui.label(egui::RichText::new(text.to_uppercase()).font(bold(12.0)).color(pal().header_secondary));
    ui.add_space(SPACE_8);
}

fn para(ui: &mut Ui, text: &str, font: egui::FontId, ink: Color32) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(ink)).wrap());
}

fn art(ui: &mut Ui, b: &Buy, avatar: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ART, ART), Sense::hover());
    match &b.what {
        What::Shop(p) => {
            let (a, c) = product::backdrop(p);
            ui.painter().rect_filled(r, CornerRadius::same(8), a.lerp_to_gamma(c, 0.5));
            product::preview(ui, r, p, avatar);
        }
        What::Boost(_) => {
            ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_rgb(0x4a, 0x1d, 0x5e));
            icons::draw(ui.painter(), Icon::Gem, Rect::from_center_size(r.center(), vec2(ART * 0.45, ART * 0.45)), Color32::from_rgb(0xff, 0x73, 0xfa));
        }
        What::Listing(_) => {
            ui.painter().rect_filled(r, CornerRadius::same(8), pal().bg_tertiary);
            icons::draw(ui.painter(), Icon::Shop, Rect::from_center_size(r.center(), vec2(ART * 0.45, ART * 0.45)), pal().header_secondary);
        }
        _ => {
            ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_rgb(0x2b, 0x1f, 0x5c));
            icons::draw(ui.painter(), if b.gift { Icon::Gift } else { Icon::Nitro }, Rect::from_center_size(r.center(), vec2(ART * 0.45, ART * 0.45)), Color32::WHITE);
        }
    }
}

fn names(b: &Buy) -> (String, String) {
    match &b.what {
        What::Shop(p) => (p["name"].as_str().unwrap_or_default().to_string(), product::label(p).to_string()),
        What::Listing(l) => (l["name"].as_str().unwrap_or_default().to_string(), "Server Product".into()),
        What::Nitro(p) => (billing::plan_name(p).to_string(), "Nitro is a recurring subscription. Cancel anytime in your settings.".into()),
        What::NitroGift => {
            let n = b.plan.as_deref().map_or("Nitro", billing::plan_name);
            (format!("Gift {n}"), "You are purchasing a gift.".into())
        }
        What::Boost(_) => (format!("{} Server Boost{}", b.qty, if b.qty == 1 { "" } else { "s" }), "Boosts are a recurring subscription. Cancel anytime in your settings.".into()),
    }
}

fn payment(ui: &mut Ui, b: &Buy, acts: &mut Vec<Act>) {
    label(ui, "Payment Method");
    let Some(list) = &b.sources else {
        crate::ui::motion::spinner(ui, 20.0, pal().header_secondary);
        return;
    };
    if list.is_empty() && b.orbs_price().is_none() {
        para(ui, "No payment method saved", reg(14.0), pal().header_secondary);
        ui.add_space(SPACE_8);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        if button(ui, r, "Add Payment Method", BLURPLE, Color32::WHITE, true, "add_card") {
            acts.push(Act::AddCard);
        }
        return;
    }
    let width = ui.available_width();
    let wait = b.prev.asked != 0;
    for s in list {
        let on = !b.pay_orbs && b.source.as_deref() == s["id"].as_str();
        let ok = super::usable(s);
        let (r, resp) = ui.allocate_exact_size(vec2(width, ROW), if ok && !wait { Sense::CLICK } else { Sense::hover() });
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(8), if on { pal().bg_mod_selected } else if resp.hovered() && ok { pal().bg_mod_hover } else { pal().bg_tertiary });
        if on {
            p.rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0, BLURPLE), StrokeKind::Inside);
        }
        billing::brand_tile(p, Rect::from_center_size(pos2(r.left() + 36.0, r.center().y), vec2(40.0, 28.0)), s["brand"].as_str().unwrap_or(if s["type"] == 2 { "paypal" } else { "" }));
        let ink = if ok { pal().header_primary } else { pal().text_muted };
        let g = p.layout_no_wrap(billing::pay_label(s), medium(14.0), ink);
        p.galley(pos2(r.left() + 68.0, r.center().y - if billing::expires(s).is_empty() { g.size().y / 2.0 } else { 16.0 }), g, ink);
        let note = if s["invalid"] == true { "Invalid".to_string() } else if !ok { "This payment method can't be used here.".to_string() } else { billing::expires(s) };
        p.text(pos2(r.left() + 68.0, r.center().y + 2.0), Align2::LEFT_TOP, note, reg(12.0), pal().header_secondary);
        mark(ui, &format!("buy:source:{}", s["id"].as_str().unwrap_or_default()), r);
        if ok && !wait && resp.clicked() {
            acts.push(Act::Source(s["id"].as_str().unwrap_or_default().to_string()));
        }
        ui.add_space(SPACE_8);
    }
    if let Some(price) = b.orbs_price() {
        let (r, resp) = ui.allocate_exact_size(vec2(width, ROW), Sense::CLICK);
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(8), if b.pay_orbs { pal().bg_mod_selected } else if resp.hovered() { pal().bg_mod_hover } else { pal().bg_tertiary });
        if b.pay_orbs {
            p.rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0, BLURPLE), StrokeKind::Inside);
        }
        icons::draw(p, Icon::Gem, Rect::from_center_size(pos2(r.left() + 36.0, r.center().y), vec2(24.0, 24.0)), Color32::from_rgb(0x5b, 0x9b, 0xf8));
        p.text(pos2(r.left() + 68.0, r.center().y - 16.0), Align2::LEFT_TOP, format!("Orbs  ·  {price}"), medium(14.0), pal().header_primary);
        let bal = b.orbs.map_or("Orbs balance: loading".to_string(), |o| format!("Orbs balance: {o}"));
        p.text(pos2(r.left() + 68.0, r.center().y + 2.0), Align2::LEFT_TOP, bal, reg(12.0), pal().header_secondary);
        mark(ui, "buy:orbs", r);
        if resp.clicked() {
            acts.push(Act::Orbs);
        }
        ui.add_space(SPACE_8);
    }
    let (r, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::CLICK);
    let ink = if resp.hovered() { pal().header_primary } else { pal().link };
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, "Add a new payment method", medium(14.0), ink);
    mark(ui, "buy:add_card", r);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        acts.push(Act::AddCard);
    }
}

fn gift(ui: &mut Ui, b: &mut Buy, friends: &[(Uid, String)], app_avatars: &dyn Fn(Uid) -> String, acts: &mut Vec<Act>) {
    label(ui, "Gift Recipient");
    let w = ui.available_width();
    crate::ui::settings::input(ui, &mut b.filter, "Search friends", false);
    ui.add_space(SPACE_8);
    let q = b.filter.to_lowercase();
    ScrollArea::vertical().id_salt("buy_friends").max_height(GIFT_ROWS).show(ui, |ui| {
        let shown: Vec<&(Uid, String)> = friends.iter().filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)).collect();
        if shown.is_empty() {
            para(ui, "No friends to send this to.", reg(14.0), pal().header_secondary);
        }
        for (id, name) in shown {
            let on = b.to == Some(*id);
            let (r, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::CLICK);
            if on || resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(8), if on { pal().bg_mod_selected } else { pal().bg_mod_hover });
            }
            crate::ui::avatar(ui, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), vec2(24.0, 24.0)), &app_avatars(*id));
            ui.painter().text(pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, name, medium(14.0), pal().header_primary);
            mark(ui, &format!("buy:friend:{id}"), r);
            if resp.clicked() {
                acts.push(Act::To(*id));
            }
        }
    });
    ui.add_space(SPACE_8);
    para(ui, "Gift will be delivered to recipient via direct message.", reg(12.0), pal().header_secondary);
    label(ui, "Give the gift of style");
    ui.horizontal_wrapped(|ui| {
        for (n, name) in GIFT_STYLES {
            let on = b.style == n;
            let g = ui.painter().layout_no_wrap(name.into(), medium(14.0), Color32::WHITE);
            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 32.0), Sense::CLICK);
            ui.painter().rect(r, CornerRadius::same(8), if on { pal().bg_mod_selected } else { pal().bg_tertiary }, Stroke::new(if on { 2.0 } else { 1.0 }, if on { BLURPLE } else { pal().border_subtle }), StrokeKind::Inside);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, name, medium(14.0), pal().header_primary);
            mark(ui, &format!("buy:style:{n}"), r);
            if resp.clicked() {
                acts.push(Act::Style(n));
            }
        }
    });
    ui.add_space(SPACE_8);
    crate::ui::settings::input(ui, &mut b.message, "Add a message (optional)", false);
}

fn totals(ui: &mut Ui, b: &Buy) {
    label(ui, "Total");
    let line = |ui: &mut Ui, name: &str, value: &str, strong: bool| {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let (f, c) = if strong { (semi(16.0), pal().header_primary) } else { (reg(14.0), pal().header_secondary) };
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, name, f.clone(), c);
        ui.painter().text(r.right_center(), Align2::RIGHT_CENTER, value, f, c);
        mark(ui, &format!("buy:{}", name.to_lowercase()), r);
    };
    if b.pay_orbs {
        line(ui, "Total", &format!("{} Orbs", b.orbs_price().unwrap_or(0)), true);
        return;
    }
    match b.ready() {
        Some((inv, renewal)) => totals_of(ui, inv, renewal, line),
        None if b.want().is_some_and(|k| k == b.prev.key) && matches!(b.prev.main, Some(Err(_))) => {
            para(ui, b.blocker().as_deref().unwrap_or("Something went wrong"), reg(14.0), pal().critical);
        }
        None => {
            crate::ui::motion::spinner(ui, 20.0, pal().header_secondary);
        }
    }
}

fn totals_of(ui: &mut Ui, inv: &Invoice, renewal: Option<&Invoice>, line: impl Fn(&mut Ui, &str, &str, bool)) {
    line(ui, "Subtotal", &billing::money(inv.subtotal, &inv.currency), false);
    line(ui, "Tax", &if inv.tax == 0 && inv.inclusive { "Included".to_string() } else { billing::money(inv.tax, &inv.currency) }, false);
    line(ui, "Total", &billing::money(inv.total, &inv.currency), true);
    if let Some(r) = renewal {
        let text = format!("Renews at {}", billing::money(r.total, &r.currency));
        ui.add_space(SPACE_8);
        para(ui, &text, reg(12.0), pal().header_secondary);
    }
}

fn stepper(ui: &mut Ui, b: &Buy, acts: &mut Vec<Act>) {
    label(ui, "Boosts");
    ui.horizontal(|ui| {
        for (d, sign) in [(-1, "−"), (1, "+")] {
            if d == 1 {
                ui.label(egui::RichText::new(b.qty.to_string()).font(semi(20.0)).color(pal().header_primary));
            }
            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::CLICK);
            ui.painter().rect_filled(r, CornerRadius::same(8), if resp.hovered() { pal().bg_mod_hover } else { pal().bg_tertiary });
            ui.painter().text(r.center(), Align2::CENTER_CENTER, sign, semi(16.0), pal().header_primary);
            mark(ui, &format!("buy:qty{d}"), r);
            if resp.clicked() {
                acts.push(Act::Qty(d));
            }
        }
    });
    if b.free_slots() > 0 {
        ui.add_space(SPACE_8);
        para(ui, &format!("You also have {} unused Boost{} that will go to this server.", b.free_slots(), if b.free_slots() == 1 { "" } else { "s" }), reg(12.0), pal().header_secondary);
    }
}

fn plans(ui: &mut Ui, b: &Buy, acts: &mut Vec<Act>) {
    let cards = [("NITRO", NITRO_MONTH, NITRO_YEAR, &b.plans[0]), ("NITRO BASIC", BASIC_MONTH, BASIC_YEAR, &b.plans[1])];
    let w = (ui.available_width() - SPACE_16) / 2.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_16;
        for (name, month, year, list) in cards {
            let (r, _) = ui.allocate_exact_size(vec2(w, 230.0), Sense::hover());
            ui.painter().rect(r, CornerRadius::same(8), pal().bg_tertiary, Stroke::new(1.0, pal().border_subtle), StrokeKind::Inside);
            ui.painter().text(r.min + vec2(SPACE_16, SPACE_16), Align2::LEFT_TOP, name, bold_italic(28.0), pal().header_primary);
            let price = |id: &str| list.as_ref().and_then(|l| l.iter().find(|p| p["id"] == id)).map(|p| billing::money(p["price"].as_i64().unwrap_or(0), p["currency"].as_str().unwrap_or("usd")));
            for (i, (id, per)) in [(month, "/month"), (year, "/year")].into_iter().enumerate() {
                let rr = Rect::from_min_size(pos2(r.left() + SPACE_16, r.top() + 72.0 + i as f32 * 56.0), vec2(w - 2.0 * SPACE_16, 48.0));
                let resp = ui.interact(rr, Id::new(("buy_plan", id)), Sense::CLICK);
                ui.painter().rect(rr, CornerRadius::same(8), if resp.hovered() { pal().bg_mod_hover } else { pal().bg_secondary }, Stroke::new(1.0, pal().border_subtle), StrokeKind::Inside);
                let text = price(id).map_or("…".to_string(), |p| format!("{p}{per}"));
                ui.painter().text(rr.center(), Align2::CENTER_CENTER, text, semi(16.0), pal().header_primary);
                mark(ui, &format!("buy:plan:{id}"), rr);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() && price(id).is_some() {
                    acts.push(Act::Plan(id.to_string()));
                }
            }
        }
    });
}

fn failed(ui: &mut Ui, msg: &str, unsure: bool) {
    ui.add_space(SPACE_16);
    ui.label(egui::RichText::new("Oops, something went wrong...").font(semi(20.0)).color(pal().header_primary));
    ui.add_space(SPACE_8);
    para(ui, msg, reg(14.0), pal().header_secondary);
    if unsure {
        ui.add_space(SPACE_8);
        para(ui, "We can't tell if you were charged. Check Billing before trying again.", semi(14.0), pal().critical);
    } else {
        ui.add_space(SPACE_8);
        para(ui, "You were not charged.", reg(14.0), pal().header_secondary);
    }
}

pub(super) fn show(app: &mut App, ctx: &egui::Context, friends: &[(Uid, String)]) {
    let Some(mut b) = app.buy.take() else { return };
    let avatar = app.store.user(app.store.me).map(|u| u.avatar_url()).unwrap_or_default();
    let faces: std::collections::HashMap<Uid, String> = friends.iter().filter_map(|(id, _)| app.store.user(*id).map(|u| (*id, u.avatar_url()))).collect();
    let screen = ctx.content_rect();
    let wide = matches!(b.step, Step::Plans);
    let w = (if wide { LARGE_MODAL } else { SMALL_MODAL }).min(screen.width() - 2.0 * SPACE_16);
    let body_h = (screen.height() - 2.0 * SPACE_32 - HEAD - FOOT).max(120.0);
    let mut acts: Vec<Act> = Vec::new();
    let busy = matches!(b.step, Step::Sending | Step::Confirming { .. });
    let shown = crate::ui::motion::Modal::new(Id::new("buy_modal")).frame(Frame::new().fill(pal().bg_secondary).corner_radius(RADIUS)).show(ctx, |ui| {
        ui.set_width(w);
        let pad = Margin::symmetric(SPACE_24 as i8, 0);
        Frame::new().inner_margin(pad).show(ui, |ui| {
            ui.set_width(w - 2.0 * SPACE_24);
            let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEAD), Sense::hover());
            let title = match b.step {
                Step::Plans => "Send a Gift",
                Step::Done(_) => "Thank you",
                _ => "Checkout",
            };
            ui.painter().text(head.left_center(), Align2::LEFT_CENTER, title, semi(20.0), pal().header_primary);
            let x = Rect::from_center_size(pos2(head.right() - 12.0, head.center().y), vec2(24.0, 24.0));
            let xr = ui.interact(x, Id::new("buy_x"), Sense::CLICK);
            icons::draw(ui.painter(), Icon::Close, x, if xr.hovered() { pal().header_primary } else { pal().header_secondary });
            mark(ui, "buy:close", x);
            if xr.clicked() && !busy {
                acts.push(Act::Close);
            }
            ScrollArea::vertical().id_salt(("buy_body", wide)).max_height(body_h).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                match b.step.clone() {
                    Step::Plans => plans(ui, &b, &mut acts),
                    Step::Review => review(ui, &mut b, &avatar, friends, &faces, &mut acts),
                    Step::Sending => {
                        ui.add_space(SPACE_32);
                        ui.vertical_centered(|ui| {
                            crate::ui::motion::spinner(ui, 32.0, pal().header_secondary);
                            ui.add_space(SPACE_16);
                            ui.label(egui::RichText::new("Processing...").font(medium(16.0)).color(pal().header_primary));
                            ui.add_space(SPACE_8);
                            para(ui, "Don't close Rustcord.", reg(14.0), pal().header_secondary);
                        });
                        ui.add_space(SPACE_32);
                        mark(ui, "buy:sending", ui.min_rect());
                    }
                    Step::Confirming { .. } => {
                        ui.add_space(SPACE_16);
                        ui.label(egui::RichText::new("Your purchase is almost complete.").font(semi(20.0)).color(pal().header_primary));
                        ui.add_space(SPACE_8);
                        para(ui, "Confirm the payment in the window that opened. This closes by itself when Discord has it.", reg(14.0), pal().header_secondary);
                        ui.add_space(SPACE_16);
                        crate::ui::motion::spinner(ui, 24.0, pal().header_secondary);
                        ui.add_space(SPACE_16);
                        mark(ui, "buy:confirming", ui.min_rect());
                    }
                    Step::Done(text) => {
                        ui.add_space(SPACE_16);
                        let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 24.0, GREEN);
                        icons::draw(ui.painter(), Icon::Check, Rect::from_center_size(r.center(), vec2(24.0, 24.0)), Color32::WHITE);
                        ui.add_space(SPACE_16);
                        para(ui, &text, reg(16.0), pal().header_primary);
                        mark(ui, "buy:done", ui.min_rect());
                        ui.add_space(SPACE_16);
                    }
                    Step::Failed { msg, unsure } => {
                        failed(ui, &msg, unsure);
                        mark(ui, "buy:failed", ui.min_rect());
                        ui.add_space(SPACE_16);
                    }
                }
            });
            let (foot, _) = ui.allocate_exact_size(vec2(ui.available_width(), FOOT), Sense::hover());
            footer(ui, foot, &b, &mut acts);
        });
    });
    if shown.should_close() && !busy {
        acts.push(Act::Close);
    }
    let mut keep = true;
    for a in acts {
        match a {
            Act::Close => keep = false,
            Act::Back => match &b.step {
                Step::Failed { .. } => b.step = Step::Review,
                Step::Review if matches!(b.what, What::NitroGift) => b.step = Step::Plans,
                _ => keep = false,
            },
            Act::Source(id) => {
                b.source = Some(id);
                b.pay_orbs = false;
            }
            Act::Orbs => b.pay_orbs = true,
            Act::Gift(on) => {
                b.gift = on;
                b.pay_orbs = false;
                b.prev = Default::default();
            }
            Act::To(id) => b.to = Some(id),
            Act::Style(s) => b.style = s,
            Act::Qty(d) => {
                b.qty = (b.qty as i32 + d).clamp(1, 20) as u32;
                b.prev = Default::default();
            }
            Act::Plan(p) => {
                b.plan = Some(p);
                b.step = Step::Review;
                b.prev = Default::default();
            }
            Act::Waiver => b.waiver = !b.waiver,
            Act::AddCard => {
                crate::webwin::add_card();
                b.card_window = true;
            }
            Act::Billing => {
                app.settings_page = Some(crate::ui::settings::Page::Billing);
                keep = false;
            }
            Act::Manage => crate::webwin::manage(),
            Act::Purchase => {
                app.buy = Some(b);
                super::purchase(app);
                return;
            }
        }
    }
    if keep {
        app.buy = Some(b);
    }
}

fn review(ui: &mut Ui, b: &mut Buy, avatar: &str, friends: &[(Uid, String)], faces: &std::collections::HashMap<Uid, String>, acts: &mut Vec<Act>) {
    let (name, sub) = names(b);
    ui.horizontal(|ui| {
        art(ui, b, avatar);
        ui.add_space(SPACE_16);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(&name).font(semi(20.0)).color(pal().header_primary));
            ui.add_space(4.0);
            para(ui, &sub, reg(14.0), pal().header_secondary);
        });
    });
    if matches!(b.what, What::Nitro(_)) && b.mine().is_some() {
        ui.add_space(SPACE_16);
        para(ui, "You already have a Nitro subscription. Change it from Subscriptions in your settings.", reg(14.0), pal().header_secondary);
        mark(ui, "buy:has_sub", ui.min_rect());
        return;
    }
    if matches!(b.what, What::Boost(_)) {
        stepper(ui, b, acts);
    }
    if matches!(b.what, What::Shop(_) | What::Listing(_)) {
        ui.add_space(SPACE_16);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::CLICK);
        let on = b.gift;
        let box_ = Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(20.0, 20.0));
        ui.painter().rect(box_, CornerRadius::same(4), if on { BLURPLE } else { Color32::TRANSPARENT }, Stroke::new(2.0, if on { BLURPLE } else { pal().header_secondary }), StrokeKind::Inside);
        if on {
            icons::draw(ui.painter(), Icon::Check, box_, Color32::WHITE);
        }
        ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, "Buy as Gift", medium(14.0), pal().header_primary);
        mark(ui, "buy:gift_toggle", r);
        if resp.clicked() {
            acts.push(Act::Gift(!on));
        }
        if b.gift {
            para(ui, "Prices for gifted items are set separately from purchases for yourself and may differ.", reg(12.0), pal().header_secondary);
        }
    }
    if b.gift {
        gift(ui, b, friends, &|id| faces.get(&id).cloned().unwrap_or_default(), acts);
    }
    payment(ui, b, acts);
    totals(ui, b);
    if b.eea() {
        ui.add_space(SPACE_16);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::CLICK);
        let box_ = Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(20.0, 20.0));
        ui.painter().rect(box_, CornerRadius::same(4), if b.waiver { BLURPLE } else { Color32::TRANSPARENT }, Stroke::new(2.0, if b.waiver { BLURPLE } else { pal().header_secondary }), StrokeKind::Inside);
        if b.waiver {
            icons::draw(ui.painter(), Icon::Check, box_, Color32::WHITE);
        }
        ui.painter().text(pos2(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, "I agree to Discord’s Right to Withdrawal waiver", medium(14.0), pal().header_primary);
        mark(ui, "buy:waiver", r);
        if resp.clicked() {
            acts.push(Act::Waiver);
        }
    }
    ui.add_space(SPACE_16);
    let verb = if b.gift { "Buy gift" } else { "Purchase" };
    para(ui, &format!("By clicking “{verb}”, you agree to the Paid Services Terms"), reg(12.0), pal().header_secondary);
    if let Some(why) = b.blocker() {
        ui.add_space(SPACE_8);
        para(ui, &why, reg(12.0), pal().text_muted);
        mark(ui, "buy:blocker", ui.min_rect());
    }
    ui.add_space(SPACE_16);
}

fn footer(ui: &mut Ui, foot: Rect, b: &Buy, acts: &mut Vec<Act>) {
    let h = 40.0;
    let y = foot.center().y - h / 2.0;
    let paint = ui.painter().clone();
    let go = |text: &str| paint.layout_no_wrap(text.into(), medium(14.0), Color32::WHITE).size().x + 2.0 * SPACE_24;
    let right = |text: &str, x: f32| Rect::from_min_size(pos2(x - go(text), y), vec2(go(text), h));
    let mut x = foot.right();
    match &b.step {
        Step::Review if matches!(b.what, What::Nitro(_)) && b.mine().is_some() => {
            let r = right("Manage Subscription", x);
            if button(ui, r, "Manage Subscription", BLURPLE, Color32::WHITE, true, "manage") {
                acts.push(Act::Manage);
            }
            x = r.left() - SPACE_8;
            let r = right("Close", x);
            if button(ui, r, "Close", Color32::TRANSPARENT, pal().header_primary, true, "cancel") {
                acts.push(Act::Close);
            }
        }
        Step::Review => {
            let label = if b.pay_orbs { "Claim with Orbs" } else if b.gift { "Buy gift" } else { "Purchase" };
            let r = right(label, x);
            let on = b.blocker().is_none();
            if button(ui, r, label, GREEN, Color32::WHITE, on, "purchase") {
                acts.push(Act::Purchase);
            }
            x = r.left() - SPACE_8;
            let back = if matches!(b.what, What::NitroGift) { "Back" } else { "Cancel" };
            let r = right(back, x);
            if button(ui, r, back, Color32::TRANSPARENT, pal().header_primary, true, "cancel") {
                acts.push(Act::Back);
            }
        }
        Step::Plans => {
            let r = right("Cancel", x);
            if button(ui, r, "Cancel", Color32::TRANSPARENT, pal().header_primary, true, "cancel") {
                acts.push(Act::Close);
            }
        }
        Step::Sending | Step::Confirming { .. } => {}
        Step::Done(_) => {
            let r = right("Done", x);
            if button(ui, r, "Done", BLURPLE, Color32::WHITE, true, "done") {
                acts.push(Act::Close);
            }
        }
        Step::Failed { unsure, .. } => {
            let r = right("Close", x);
            if button(ui, r, "Close", BLURPLE, Color32::WHITE, true, "close") {
                acts.push(Act::Close);
            }
            x = r.left() - SPACE_8;
            if *unsure {
                let r = right("Open Billing", x);
                if button(ui, r, "Open Billing", Color32::TRANSPARENT, pal().header_primary, true, "billing") {
                    acts.push(Act::Billing);
                }
            } else {
                let r = right("Back", x);
                if button(ui, r, "Back", Color32::TRANSPARENT, pal().header_primary, true, "back") {
                    acts.push(Act::Back);
                }
            }
        }
    }
}

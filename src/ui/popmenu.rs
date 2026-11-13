use super::*;

/// `**bold**` stretches in a line of dialog text.
pub(crate) fn bolded(text: &str, size: f32, ink: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (i, part) in text.split("**").enumerate() {
        let (font, col) = if i % 2 == 1 { (semi(size), pal().header_primary) } else { (reg(size), ink) };
        job.append(part, 0.0, egui::TextFormat::simple(font, col));
    }
    job.wrap.max_width = width;
    job
}

/// A menu that opens at a point, like a right click, with room for submenus. False once closed.
pub(crate) fn float_menu(ctx: &egui::Context, id: &str, at: Pos2, content: impl FnOnce(&mut Ui)) -> bool {
    let mut open = true;
    let screen = ctx.content_rect();
    let h = ctx.data(|d| d.get_temp::<f32>(egui::Id::new("menu_h"))).unwrap_or(0.0) + 16.0;
    let at = pos2(at.x, at.y.min(screen.bottom() - 8.0 - h).max(screen.top() + 8.0));
    egui::Popup::new(egui::Id::new(id), ctx.clone(), egui::PopupAnchor::Position(at), egui::LayerId::background())
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .frame(Frame::menu(&ctx.global_style()))
        .align(egui::RectAlign::BOTTOM_START)
        .open_bool(&mut open)
        .show(|ui| menu_body(ui, content));
    open
}

/// Shift+click on a menu's checkbox ticks it and leaves the menu open, as in discord.
pub(crate) fn menu_closing(ctx: &egui::Context) -> egui::PopupCloseBehavior {
    if ctx.input(|i| i.modifiers.shift) { egui::PopupCloseBehavior::CloseOnClickOutside } else { egui::PopupCloseBehavior::CloseOnClick }
}

/// Long menus scroll rather than run off a small window.
pub(crate) fn menu_body(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let h = ui.ctx().content_rect().height() - 32.0;
    ui.set_max_height(h);
    let out = egui::ScrollArea::vertical().max_height(h).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        content(ui)
    });
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("menu_h"), out.content_size.y));
}

thread_local! {
    static ROWS: std::cell::RefCell<Option<Vec<(String, egui::Id)>>> = const { std::cell::RefCell::new(None) };
}

/// Runs `menu`, returning the label and id of each row it drew, top to bottom.
pub(crate) fn rows_of(menu: impl FnOnce()) -> Vec<(String, egui::Id)> {
    ROWS.with(|r| *r.borrow_mut() = Some(Vec::new()));
    menu();
    ROWS.with(|r| r.borrow_mut().take()).unwrap_or_default()
}

/// A select's option, listed with the menu's rows for `select_keys`.
pub(crate) fn option(ui: &mut Ui, on: bool, label: &str) -> egui::Response {
    let r = ui.selectable_label(on, label);
    list_row(label, r.id);
    r
}

/// Lists a hand-drawn row with its menu's rows for `select_keys`.
pub(crate) fn list_row(label: &str, id: egui::Id) {
    ROWS.with(|rows| {
        if let Some(v) = rows.borrow_mut().as_mut() {
            v.push((label.to_string(), id));
        }
    });
}

/// A select's keys, once its list is open: up and down move by one and wrap, page up and down
/// by ten, and letters jump to the first row starting with what was typed in the last second.
pub(crate) fn select_keys(ctx: &egui::Context, rows: &[(String, egui::Id)], popup: egui::Id) {
    if rows.is_empty() {
        return;
    }
    let n = rows.len() as i64;
    let at = rows.iter().position(|(_, id)| ctx.memory(|m| m.has_focus(*id))).map_or(-1, |a| a as i64);
    let mut to = None;
    ctx.input_mut(|i| {
        if take_key(i, egui::Modifiers::NONE, egui::Key::ArrowDown) {
            to = Some(if at + 1 >= n { 0 } else { at + 1 });
        }
        if take_key(i, egui::Modifiers::NONE, egui::Key::ArrowUp) {
            to = Some(if at <= 0 { n - 1 } else { at - 1 });
        }
        if take_key(i, egui::Modifiers::NONE, egui::Key::PageDown) {
            to = Some((at.max(0) + 10).min(n - 1));
        }
        if take_key(i, egui::Modifiers::NONE, egui::Key::PageUp) {
            to = Some((at.max(0) - 10).max(0));
        }
    });
    let typed: String = ctx.input(|i| i.events.iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect());
    if !typed.is_empty() {
        let key = egui::Id::new("select_typed");
        let (mut prefix, at_t): (String, Option<Instant>) = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
        if at_t.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            prefix.clear();
        }
        prefix.push_str(&typed.to_lowercase());
        to = rows.iter().position(|(l, _)| l.to_lowercase().starts_with(&prefix)).map(|i| i as i64).or(to);
        ctx.data_mut(|d| d.insert_temp(key, (prefix, Some(Instant::now()))));
    }
    if let Some(i) = to {
        ctx.memory_mut(|m| m.request_focus(rows[i as usize].1));
    }
    // Enter on a row presses it, which a keyboard press doesn't close the list for
    let pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
    if pressed && rows.iter().any(|(_, id)| ctx.memory(|m| m.has_focus(*id))) {
        egui::Popup::close_id(ctx, popup);
    }
}

/// Discord's mute durations, in seconds; None is until turned back on.
pub(crate) fn mute_menu(ui: &mut Ui) -> Option<Option<i64>> {
    let mut pick = None;
    for (label, secs) in [
        ("For 15 Minutes", Some(900)),
        ("For 1 Hour", Some(3600)),
        ("For 3 Hours", Some(10800)),
        ("For 8 Hours", Some(28800)),
        ("For 24 Hours", Some(86400)),
        ("Until I turn it back on", None),
    ] {
        if menu_item(ui, None, label, false) {
            pick = Some(secs);
        }
    }
    pick
}

pub(crate) const LEVELS: [&str; 3] = ["All Messages", "Only @mentions", "Nothing"];

/// The radio rows under a channel's Notification Settings. Some(None) goes back to the default.
pub(crate) fn level_menu(ui: &mut Ui, level: Option<u8>, inherited: u8, default_label: &str) -> Option<Option<u8>> {
    let mut pick = None;
    if menu_row(ui, None, default_label, Some(LEVELS[inherited.min(2) as usize]), false, Trail::Radio(level.is_none())).clicked() {
        pick = Some(None);
    }
    for (i, label) in LEVELS.iter().enumerate() {
        if menu_row(ui, None, label, None, false, Trail::Radio(level == Some(i as u8))).clicked() {
            pick = Some(Some(i as u8));
        }
    }
    pick
}

/// What sits at the right end of a menu row.
#[derive(PartialEq)]
pub(crate) enum Trail {
    None,
    Caret,
    Check(bool),
    Radio(bool),
    Image(String),
}

/// One row of discord's menus. Rows size the menu to the widest of them, 34 tall, or 36 with
/// an icon at either end, or 52 with a grey line under the label.
pub(crate) fn menu_row(ui: &mut Ui, icon: Option<icons::Icon>, label: &str, sub: Option<&str>, danger: bool, trail: Trail) -> egui::Response {
    let (resp, lead) = menu_row_at(ui, icon.is_some(), label, sub, danger, trail);
    if let Some(i) = icon {
        icons::draw(ui.painter(), i, lead, if danger { pal().critical } else { pal().header_secondary });
    }
    resp
}

/// A menu row that leaves its leading 20x20 for the caller to paint.
pub(crate) fn menu_row_at(ui: &mut Ui, lead: bool, label: &str, sub: Option<&str>, danger: bool, trail: Trail) -> (egui::Response, Rect) {
    let ink = if danger { pal().critical } else { pal().header_primary };
    let text = ui.painter().layout_no_wrap(label.to_string(), medium(14.0), ink);
    let sub = sub.map(|s| {
        let (plain, bold) = s.split_at(s.find("@mentions").unwrap_or(s.len()));
        let mut job = egui::text::LayoutJob::default();
        job.append(plain, 0.0, egui::TextFormat::simple(reg(12.0), pal().text_muted));
        job.append(bold, 0.0, egui::TextFormat::simple(semi(12.0), pal().text_muted));
        ui.painter().layout_job(job)
    });
    let indent = if lead { 36.0 } else { 8.0 };
    let tail = if trail == Trail::None { 8.0 } else { 36.0 };
    let w = indent + text.size().x.max(sub.as_ref().map_or(0.0, |s| s.size().x)) + tail;
    let h = if sub.is_some() { 52.0 } else if lead || trail != Trail::None { 36.0 } else { 34.0 };
    let (r, resp) = ui.allocate_at_least(vec2(w, h), Sense::click());
    list_row(label, resp.id);
    // menus have no app to report to; the e2e driver finds rows here by label
    settings::mark(ui, &format!("menu_row:{label}"), r);
    let p = ui.painter();
    if resp.has_focus() {
        p.rect_filled(r, CornerRadius::same(4), if danger { crate::theme::CRITICAL_BG } else { crate::theme::MENU_HOVER });
    } else {
        crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(4), if danger { crate::theme::CRITICAL_BG } else { crate::theme::MENU_HOVER }, 0.05);
    }
    let top = if sub.is_some() { r.top() + 8.0 } else { r.center().y - text.size().y / 2.0 };
    if let Some(s) = sub {
        p.galley(pos2(r.left() + indent, r.top() + 28.0), s, pal().text_muted);
    }
    p.galley(pos2(r.left() + indent, top), text, ink);
    let at = Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), vec2(20.0, 20.0));
    match trail {
        Trail::None => {}
        Trail::Caret => icons::draw(p, icons::Icon::ChevronRight, at, pal().text_muted),
        Trail::Check(true) => {
            p.rect_filled(at.shrink(1.0), CornerRadius::same(4), pal().blurple);
            icons::draw(p, icons::Icon::Check, at.shrink(3.0), Color32::WHITE);
        }
        Trail::Check(false) => {
            p.rect_stroke(at.shrink(1.0), CornerRadius::same(4), egui::Stroke::new(1.0, pal().border_muted), egui::StrokeKind::Inside);
        }
        Trail::Radio(true) => {
            p.circle_filled(at.center(), 9.0, pal().blurple);
            p.circle_filled(at.center(), 4.0, Color32::WHITE);
        }
        Trail::Image(url) => {
            egui::Image::from_uri(url).show_loading_spinner(false).paint_at(ui, at);
        }
        Trail::Radio(false) => {
            p.circle_stroke(at.center(), 8.5, egui::Stroke::new(1.5, pal().header_secondary));
        }
    }
    (resp, Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(20.0, 20.0)))
}

pub(crate) fn menu_sep(ui: &mut Ui) {
    let (r, _) = ui.allocate_at_least(vec2(0.0, 17.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(8.0), r.center().y, egui::Stroke::new(1.0, pal().border_subtle));
}

/// Discord's server tag chip after a name: badge and tag on a grey wash, 16 tall.
pub(crate) fn tag_chip(p: &egui::Painter, u: &crate::state::User, at: Pos2, fade: f32) -> Option<Rect> {
    let (tag, _, _) = u.tag.as_ref()?;
    let ink = pal().text_normal.gamma_multiply(fade);
    let g = p.layout_no_wrap(tag.clone(), semi(12.0), ink);
    let badge = u.tag_badge_url();
    let w = 8.0 + if badge.is_some() { 16.0 } else { 0.0 } + g.size().x;
    let r = Rect::from_min_size(pos2(at.x, at.y - 8.0), vec2(w, 16.0));
    p.rect_filled(r, CornerRadius::same(4), Color32::from_rgba_unmultiplied(151, 151, 159, 61).gamma_multiply(fade));
    let mut x = r.left() + 4.0;
    if let Some(url) = badge {
        text::image_tint(p, &url, Rect::from_min_size(pos2(x, at.y - 7.0), vec2(14.0, 14.0)), Color32::WHITE.gamma_multiply(fade));
        x += 16.0;
    }
    p.galley(pos2(x, at.y - g.size().y / 2.0), g, ink);
    Some(r)
}

/// A profile's server tag: the same badge and tag as a button, 24 tall, in regular weight.
pub(crate) fn profile_tag(p: &egui::Painter, u: &crate::state::User, at: Pos2) -> Option<Rect> {
    let (tag, _, _) = u.tag.as_ref()?;
    let g = p.layout_no_wrap(tag.clone(), reg(12.0), pal().text_normal);
    let badge = u.tag_badge_url();
    let w = 17.0 + if badge.is_some() { 16.0 } else { 0.0 } + g.size().x;
    let r = Rect::from_min_size(pos2(at.x, at.y - 12.0), vec2(w, 24.0));
    p.rect_filled(r, CornerRadius::same(8), Color32::from_rgba_unmultiplied(151, 151, 159, 31));
    let mut x = r.left() + 8.8;
    if let Some(url) = badge {
        text::image_tint(p, &url, Rect::from_min_size(pos2(x, at.y - 7.0), vec2(14.0, 14.0)), Color32::WHITE);
        x += 16.0;
    }
    p.galley(pos2(x, at.y - g.size().y / 2.0), g, pal().text_normal);
    Some(r)
}

/// The blurple "APP" tag after a bot's name, with the check for verified apps. A message's is a
/// size up (0.8em of the name) and a pixel shorter.
pub(crate) fn app_tag(p: &egui::Painter, at: Pos2, verified: bool, fade: f32, chat: bool) -> Rect {
    let (size, pad, h) = if chat { (12.8, 4.4, 15.0) } else { (12.0, 4.0, 16.0) };
    let g = p.layout_no_wrap("APP".into(), semi(size), Color32::WHITE);
    let w = 2.0 * pad + if verified { 16.0 } else { 0.0 } + g.size().x;
    let r = Rect::from_min_size(pos2(at.x, at.y - h / 2.0), vec2(w, h));
    p.rect_filled(r, CornerRadius::same(4), pal().blurple.gamma_multiply(fade.max(0.5)));
    let mut x = r.left() + pad;
    if verified {
        icons::draw(p, icons::Icon::Verified, Rect::from_min_size(pos2(x - 2.0, at.y - 8.0), vec2(16.0, 16.0)), Color32::WHITE);
        x += 14.0;
    }
    p.galley(pos2(x, at.y - g.size().y / 2.0), g, Color32::WHITE);
    r
}

pub(crate) fn channel_title(store: &Store, ch: Id) -> String {
    let Some(c) = store.channels.get(&ch) else { return String::new() };
    match c.kind {
        1 => c.recipients.first().map(|u| store.name_in(0, *u)).unwrap_or_else(|| "Unknown".into()),
        3 if c.name.is_empty() => c
            .recipients
            .iter()
            .map(|u| store.name_in(0, *u))
            .collect::<Vec<_>>()
            .join(", "),
        _ => c.name.clone(),
    }
}

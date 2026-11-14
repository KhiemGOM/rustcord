use super::*;

pub(crate) fn avatar(ui: &Ui, rect: Rect, url: &str) {
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, pal().bg_accent);
    crate::gif::paint_still(ui, url, rect, CornerRadius::same((rect.width() / 2.0) as u8), ui.rect_contains_pointer(rect));
}

pub(crate) fn status_dot(ui: &Ui, avatar: Rect, s: Status, ring: Color32) {
    let r = (avatar.width() * 0.16).max(4.0);
    let c = pos2(avatar.right() - r * 0.9, avatar.bottom() - r * 0.9);
    ui.painter().circle_filled(c, r + 2.5, ring);
    let col = status_color(s);
    match s {
        Status::Offline => {
            ui.painter().circle_filled(c, r, col);
            ui.painter().circle_filled(c, r * 0.45, ring);
        }
        Status::Idle => {
            ui.painter().circle_filled(c, r, col);
            ui.painter().circle_filled(c + vec2(-r * 0.45, -r * 0.45), r * 0.6, ring);
        }
        Status::Dnd => {
            ui.painter().circle_filled(c, r, col);
            ui.painter().rect_filled(
                Rect::from_center_size(c, vec2(r * 1.2, r * 0.4)),
                CornerRadius::same(2),
                ring,
            );
        }
        Status::Online => {
            ui.painter().circle_filled(c, r, col);
        }
    }
}

pub(crate) fn badge(ui: &Ui, right_center: Pos2, n: u32, ring: Option<Color32>) {
    badge_scaled(ui, right_center, n, ring, 1.0);
}

/// A badge that grows in when it first shows, on discord's .2s (0, 0, .2, 1.1) curve.
pub(crate) fn badge_in(ui: &Ui, id: egui::Id, right_center: Pos2, n: u32, ring: Option<Color32>) {
    badge_scaled(ui, right_center, n, ring, motion::enter(ui.ctx(), id, 0.2, motion::Curve::Bezier(0.0, 0.0, 0.2, 1.1)));
}

pub(super) fn badge_scaled(ui: &Ui, right_center: Pos2, n: u32, ring: Option<Color32>, k: f32) {
    let text = if n > 99 { "99+".to_string() } else { n.to_string() };
    let galley = ui.painter().layout_no_wrap(text, semi(12.0), Color32::WHITE);
    let w = (galley.size().x + 10.0).max(16.0);
    let rect = Rect::from_min_size(pos2(right_center.x - w, right_center.y - 8.0), vec2(w, 16.0));
    let rect = Rect::from_center_size(rect.center(), rect.size() * k);
    if let Some(ring) = ring {
        ui.painter().rect_filled(rect.expand(3.0 * k), CornerRadius::same(11), ring);
    }
    ui.painter().rect_filled(rect, CornerRadius::same(8), pal().red);
    // centre the digits' ink, not the line box, which keeps room under them for descenders
    let mut ink = (f32::MAX, f32::MIN);
    for r in &galley.rows {
        for g in &r.glyphs {
            let top = r.pos.y + g.pos.y + g.uv_rect.offset.y;
            ink = (ink.0.min(top), ink.1.max(top + g.uv_rect.size.y));
        }
    }
    let mid = if ink.0 < ink.1 { (ink.0 + ink.1) / 2.0 } else { galley.size().y / 2.0 };
    ui.painter().galley_with_override_text_color(pos2(rect.center().x - galley.size().x / 2.0, rect.center().y - mid), galley, Color32::WHITE.gamma_multiply(k.clamp(0.0, 1.0)));
}

pub(crate) fn button(ui: &mut Ui, text: &str, fill: Color32, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let fill = motion::mix(fill, fill.gamma_multiply(0.85), motion::hot(ui.ctx(), &resp));
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, medium(16.0), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub(crate) fn modal_button(ui: &mut Ui, text: &str, fill: Option<Color32>, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let k = motion::hot(ui.ctx(), &resp);
    let (fill, ink, edge) = match fill {
        Some(c) => (motion::mix(c, c.gamma_multiply(0.85), k), Color32::WHITE, egui::Stroke::NONE),
        None => (motion::mix(pal().control, pal().control_hover, k), pal().header_primary, egui::Stroke::new(1.0, pal().border_subtle)),
    };
    ui.painter().rect(rect, CornerRadius::same(8), fill, edge, egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, medium(16.0), ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Square icon button. Small ones (header style) only brighten on hover,
/// 32px+ ones get the rounded hover plate like the user panel and message toolbar.
pub(crate) fn icon_button(ui: &mut Ui, icon: icons::Icon, size: f32, tip: &str) -> egui::Response {
    toggle_button(ui, icon, size, tip, false)
}

/// An icon button that stays lit while what it opens is showing, like discord's member list toggle.
pub(crate) fn toggle_button(ui: &mut Ui, icon: icons::Icon, size: f32, tip: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let k = motion::hot(ui.ctx(), &resp);
    let col = motion::mix(if on { pal().header_primary } else { pal().header_secondary }, pal().interactive_hover, k);
    if size >= 32.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal().control_hover.gamma_multiply(k));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let s = if size >= 32.0 { 20.0 } else { (size * 0.86).min(24.0) };
    icons::draw(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(s, s)), col);
    resp.on_hover_text(tip)
}

/// A context menu row the way discord draws them: icon on the left, 14 medium label,
/// red for destructive ones, a soft wash on hover.
pub(crate) fn menu_item(ui: &mut Ui, icon: Option<icons::Icon>, label: &str, danger: bool) -> bool {
    menu_row(ui, icon, label, None, danger, Trail::None).clicked()
}

/// A confirmation like discord's, for leaving servers, blocking, pinning and so on: Cancel
/// beside a red button, or a blurple one for the harmless ones.
pub(crate) struct Confirm {
    pub title: String,
    // **bold** stretches as discord bolds names in these
    pub body: String,
    pub button: &'static str,
    pub fill: Color32,
    // the message it's about, shown under the body as the pin and delete dialogs do
    pub msg: Option<(Id, Id)>,
    // discord's protip under that: the words, the bold part, the rest
    pub tip: Option<[&'static str; 3]>,
    // "Don't ask again" beside the buttons, and what ticking it does when the answer is yes
    pub check: Option<(bool, fn(&mut App))>,
    pub run: Box<dyn FnOnce(&mut App)>,
    // Enter only answers once the dialog has been up a frame, not the Enter that opened it
    pub(super) shown: bool,
}

impl Confirm {
    pub fn new(title: impl Into<String>, body: impl Into<String>, button: &'static str, run: impl FnOnce(&mut App) + 'static) -> Self {
        Confirm { title: title.into(), body: body.into(), button, fill: pal().danger, msg: None, tip: None, check: None, run: Box::new(run), shown: false }
    }

    pub fn blurple(mut self) -> Self {
        self.fill = pal().blurple;
        self
    }

    pub fn about(mut self, ch: Id, id: Id) -> Self {
        self.msg = Some((ch, id));
        self
    }

    pub fn tip(mut self, tip: [&'static str; 3]) -> Self {
        self.tip = Some(tip);
        self
    }

    pub fn dont_ask(mut self, then: fn(&mut App)) -> Self {
        self.check = Some((false, then));
        self
    }
}

/// Discord's quieter button: a grey wash with a hairline, for Cancel and the like.
pub(crate) fn grey_button(ui: &mut Ui, text: &str, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let fill = motion::mix(pal().control, pal().control_hover, motion::hot(ui.ctx(), &resp));
    ui.painter().rect(rect, CornerRadius::same(8), fill, egui::Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, medium(16.0), pal().header_primary);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

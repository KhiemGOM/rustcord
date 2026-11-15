use super::*;

/// What a name is painted with. A role gradient tiles every 100px and drifts while
/// the message is hovered, glowing as it goes; a nitro display style stretches its
/// colours over the whole name.
pub(crate) struct Ink {
    pub stops: Vec<Color32>,
    pub(super) tile: bool,
    pub(super) glow: bool,
}

impl Ink {
    pub(crate) fn flat(c: Color32) -> Self {
        Ink { stops: vec![c], tile: false, glow: false }
    }

    pub(crate) fn at(&self, w: f32, phase: f32) -> impl Fn(f32) -> Color32 + '_ {
        move |x| {
            let t = if self.tile { (x - phase).rem_euclid(100.0) / 100.0 } else { (x / w - 0.1) / 0.8 };
            ramp(&self.stops, t)
        }
    }

    pub(crate) fn layout(&self, ctx: &egui::Context, name: &str, font: egui::FontId, max_w: f32, hot: bool) -> text::Rich {
        self.layout_em(ctx, name, font, max_w, hot, 1.25)
    }

    /// `em` as in `text::layout_em`: 1.0 draws emoji with the system font, as the lists do.
    pub(crate) fn layout_em(&self, ctx: &egui::Context, name: &str, font: egui::FontId, max_w: f32, hot: bool, em: f32) -> text::Rich {
        if self.stops.len() < 2 {
            return text::layout_em(ctx, name, font, self.stops[0], max_w, em);
        }
        let rich = text::layout_em(ctx, name, font, Color32::WHITE, max_w, em);
        let phase = if hot && self.tile { ctx.input(|i| i.time) as f32 * 100.0 / 1.5 } else { 0.0 };
        if phase > 0.0 {
            ctx.request_repaint();
        }
        let w = rich.size().x.max(1.0);
        rich.tint(self.at(w, phase))
    }

    pub(crate) fn paint(&self, p: &egui::Painter, rich: &text::Rich, pos: Pos2, hot: bool) {
        if self.glow || (self.tile && hot) {
            // css blurs a copy of the name by 4px; a few faint rings of it look about the same
            let phase = if hot && self.tile { p.ctx().input(|i| i.time) as f32 * 100.0 / 1.5 } else { 0.0 };
            let w = rich.size().x.max(1.0);
            let at = self.at(w, phase);
            for (r, a) in [(1.5, 0.03), (3.0, 0.025), (4.5, 0.018), (6.0, 0.012)] {
                let tinted = rich.clone().tint(|x| at(x).gamma_multiply(a));
                for k in 0..12 {
                    let ang = k as f32 * std::f32::consts::TAU / 12.0;
                    p.galley(pos + vec2(ang.cos(), ang.sin()) * r, tinted.galley.clone(), Color32::WHITE);
                }
            }
        }
        rich.paint(p, pos);
    }
}

/// A gradient role's colours, repeating along the name.
pub(crate) fn grad_ink(r: &crate::state::Role) -> Ink {
    // the light theme darkens gradient names to 85% brightness
    let k = if pal().dark { 1.0 } else { 0.85 };
    let dim = |c: u32| {
        let c = role_color(c);
        Color32::from_rgb((c.r() as f32 * k) as u8, (c.g() as f32 * k) as u8, (c.b() as f32 * k) as u8)
    };
    let mut stops: Vec<Color32> = r.grad.iter().map(|c| dim(*c)).collect();
    stops.push(stops[0]);
    Ink { stops, tile: true, glow: false }
}

/// The ink for an author's name, and the role colour for the dot some people prefer.
pub(super) fn name_ink(d: &D, who: Id) -> (Ink, Option<Color32>) {
    use crate::cache::RoleColors::*;
    // discord paints a nitro style's colours only outside servers; in one, roles colour the name
    // and the style lends just its font
    if d.guild == 0
        && d.s.name_styles
        && let Some((fx, cols)) = d.store.user(who).and_then(|u| u.style.clone())
    {
        let min = if matches!(fx, 2 | 6 | 7) { 2.5 } else { 3.0 };
        let stops: Vec<Color32> = cols.iter().map(|c| contrast(role_color(*c), pal().bg_primary, min)).collect();
        // gradient, glow and prism spread their colours; the other effects sit flat on the first
        return match fx {
            2 | 6 | 7 => (Ink { stops, tile: false, glow: fx == 6 }, None),
            _ => (Ink::flat(stops[0]), None),
        };
    }
    let role = d.store.role_look(d.guild, who);
    let solid = role.map(|r| role_color(r.color));
    match d.s.role_colors {
        InNames => match role {
            Some(r) if d.store.role_gradient(d.guild, who).is_some() => (grad_ink(r), None),
            _ => (Ink::flat(solid.unwrap_or(pal().header_primary)), None),
        },
        NextToNames => (Ink::flat(pal().header_primary), solid),
        Hidden => (Ink::flat(pal().header_primary), None),
    }
}

/// Name, role icon, optional APP tag and timestamp on one 22px line. The timestamp shares the
/// name's baseline rather than its centre, which is what makes discord's header read right.
#[allow(clippy::too_many_arguments)]
pub(super) fn author_line(ui: &mut Ui, d: &mut D, m: &Message, name: &str, ink: &Ink, dot: Option<Color32>, font: f32, hot: bool) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), (font * 1.375).round()), Sense::hover());
    let face = name_font(d.store.user(m.author).map_or(0, |u| u.font), font);
    let n = ink.layout(ui.ctx(), name, face, row.width() - 160.0, hot);
    // an odd-height galley centred in the 22 row lands on a half pixel and rounds a row low
    let ny = (row.center().y - n.size().y / 2.0).floor();
    let name_rect = Rect::from_min_size(pos2(row.left(), ny), n.size());
    let resp = ui.interact(name_rect, ui.id().with(("author", name)), Sense::click());
    ink.paint(ui.painter(), &n, name_rect.min, hot);
    if resp.hovered() {
        ui.painter().hline(name_rect.x_range(), name_rect.bottom() - 2.0, Stroke::new(1.0, ink.stops[0]));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let vis = ui.clip_rect().contains_rect(row);
    if vis {
        d.rects.push((format!("name:{}", m.author), name_rect));
    }
    let base = ny + n.baseline();
    // headerText's 4px margin; the icon (which has none after it) and the timestamp bring their own
    let mut x = name_rect.right() + 4.0;
    if let Some(c) = dot {
        ui.painter().circle_filled(pos2(x + 5.0, row.center().y), 5.0, c);
        x += 14.0;
    }
    let cy = row.center().y;
    let user = d.store.user(m.author);
    if let Some(r) = user.filter(|u| !u.bot).and_then(|u| crate::ui::tag_chip(ui.painter(), u, pos2(x, cy), 1.0)) {
        if crate::ui::tagpop::hit(ui, r, ("msg_tag", m.id))
            && let Some((_, g, _)) = user.and_then(|u| u.tag.as_ref())
        {
            d.acts.push(Act::Tag(*g, r.right_top() + vec2(8.0, -3.0)));
        }
        x = r.right() + 4.0;
    }
    if let Some((r, url)) = d.store.role_icon(d.guild, m.author).and_then(|r| Some((r, r.icon_url(40)?))) {
        let ir = Rect::from_min_size(pos2(x, row.top() + 1.0), vec2(20.0, 20.0));
        text::image(ui.painter(), &url, ir);
        ui.interact(ir, ui.id().with(("role_icon", m.id)), Sense::hover()).on_hover_text(&r.name);
        if vis {
            d.rects.push((format!("role_icon:{}", m.author), ir));
        }
        x = ir.right();
    }
    if let Some(u) = user.filter(|u| u.bot) {
        x = crate::ui::app_tag(ui.painter(), pos2(x, cy + 0.6), u.flags & (1 << 16) != 0, 1.0, true).right() + 4.0;
    }
    let op = d.store.channels.get(&d.ch).is_some_and(|c| c.is_thread() && c.owner == m.author && d.store.channels.get(&c.parent).is_some_and(|p| p.is_forum()));
    if op {
        let r = Rect::from_min_size(pos2(x + 4.0, cy - 7.5), vec2(26.0, 15.0));
        ui.painter().rect_filled(r, CornerRadius::same(4), Color32::from_rgb(199, 210, 255));
        ui.painter().text(r.center(), Align2::CENTER_CENTER, "OP", semi(12.0), Color32::from_rgb(70, 84, 192));
        ui.interact(r, ui.id().with(("op", m.id)), Sense::hover()).on_hover_text("Original Poster");
        if vis {
            d.rects.push(("op_chip".into(), r));
        }
        x = r.right();
    }
    let t = ui.painter().layout_no_wrap(stamp(m.ts()), medium(12.0 * font / 16.0), pal().chat_muted);
    let ty = base - text::baseline(&t);
    let tr = Rect::from_min_size(pos2(x + 4.0, ty), t.size());
    ui.painter().galley(tr.min, t, pal().chat_muted);
    let r = ui.interact(tr, ui.id().with(("stamp", name, m.id)), Sense::hover());
    if r.hovered() {
        let t = markdown::local(m.ts());
        r.on_hover_text(format!("{} {}", t.format("%A, %B %-d, %Y"), t.format(markdown::hm())));
    }
    if vis {
        d.rects.push((format!("stamp:{}", m.author), tr));
    }
    if m.flags & (1 << 12) != 0 {
        let ir = Rect::from_min_size(pos2(tr.right() + 4.0, tr.center().y - 8.0), vec2(16.0, 16.0));
        icons::draw(ui.painter(), Icon::BellOff, ir, pal().chat_muted);
        ui.interact(ir, ui.id().with(("silent", m.id)), Sense::hover()).on_hover_text("This is a @silent message.");
        if vis {
            d.rects.push((format!("silent:{}", m.id), ir));
        }
    }
    if resp.clicked() {
        d.acts.push(if ui.input(|i| i.modifiers.shift) { Act::Mention(m.author) } else { Act::Profile(m.author, name_rect.right_top() + vec2(8.0, 0.0)) });
    }
    if resp.secondary_clicked() {
        d.acts.push(Act::UserMenu(m.author, resp.interact_pointer_pos().unwrap_or(name_rect.center())));
    }
}

// compact mode: "Name  message text" on one line, the name clickable like everywhere else
pub(super) fn compact_prefix(d: &D, m: &Message, blocks: &mut [Block], name: &str, color: Color32, font: f32) {
    let Some(Block::Text(t)) = blocks.first_mut() else { return };
    let fmt = egui::text::TextFormat {
        font_id: medium(font),
        color,
        line_height: Some((font * 1.375).round()),
        valign: Align::Center,
        ..Default::default()
    };
    markdown::prepend(t, &format!("{name}  "), fmt, Some(Link::User(m.author)));
    if d.s.compact_avatars
        && let Some(u) = d.store.user(m.author)
    {
        let size = (font * 1.1).round();
        let w = d.ctx.fonts_mut(|f| f.glyph_width(&reg(font), '\u{a0}'));
        let gap = egui::text::TextFormat { font_id: reg(font), color: Color32::TRANSPARENT, valign: Align::Center, ..Default::default() };
        markdown::prepend(t, "\u{a0}", gap, None);
        t.job.sections[0].leading_space = size - w + 4.0;
        t.emojis.push((0, u.avatar_url(), size, size - w + 4.0));
    }
}

pub(super) fn compact_name_line(ui: &mut Ui, d: &mut D, m: &Message, name: &str, color: Color32, font: f32) {
    let r = text::label(ui, name, medium(font), color, Sense::click());
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if r.clicked() {
        d.acts.push(if ui.input(|i| i.modifiers.shift) { Act::Mention(m.author) } else { Act::Profile(m.author, r.rect.right_top() + vec2(8.0, 0.0)) });
    }
    if r.secondary_clicked() {
        d.acts.push(Act::UserMenu(m.author, r.interact_pointer_pos().unwrap_or(r.rect.center())));
    }
}

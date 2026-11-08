use super::*;

pub(super) fn key_down(vk: u8) -> bool {
    // high bit set means the key is held right now
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// `consume_key` treats extra modifiers as a match, so plain Enter would also
/// swallow Shift+Enter. Shortcuts here want the exact combination.
pub(crate) fn take_key(i: &mut egui::InputState, mods: egui::Modifiers, key: egui::Key) -> bool {
    let mut hit = false;
    i.events.retain(|e| {
        let m = matches!(e, egui::Event::Key { key: k, modifiers: em, pressed: true, .. } if *k == key && em.matches_exact(mods));
        hit |= m;
        !m
    });
    hit
}

/// Enter in a dialog: it goes ahead with the main button, unless Tab has put the keyboard on
/// some other widget, which then takes the key itself. A text field only lets it through when
/// `text_ok`, which is not for ones that type new lines.
pub(crate) fn enter_submits(ctx: &egui::Context, text_ok: bool) -> bool {
    if !ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift && !i.modifiers.command) {
        return false;
    }
    match ctx.memory(|m| m.focused()) {
        None => true,
        Some(id) => text_ok && egui::TextEdit::load_state(ctx, id).is_some(),
    }
}

/// Takes a typed character out of this frame's input, so nothing else types it.
pub(crate) fn take_text(i: &mut egui::InputState, s: &str) -> bool {
    let mut hit = false;
    i.events.retain(|e| {
        let m = matches!(e, egui::Event::Text(t) if t == s);
        hit |= m;
        !m
    });
    hit
}

/// Discord's channel list sits in the app frame: a hairline down its left and along its top
/// with a 12px rounded corner where they meet, the frame colour showing outside the curve, an
/// 8px strip of frame below the account card, and on onyx a line where the chat page starts.
pub(super) fn side_edge(p: &egui::Painter, r: Rect) {
    let rad = 12.0;
    let c = pos2(r.left() + rad, r.top() + rad);
    let arc: Vec<Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 24.0);
            c + vec2(a.cos(), a.sin()) * rad
        })
        .collect();
    let mut corner = egui::Mesh::default();
    corner.colored_vertex(r.left_top(), pal().bg_tertiary);
    for q in &arc {
        corner.colored_vertex(*q, pal().bg_tertiary);
    }
    for i in 1..arc.len() as u32 {
        corner.add_triangle(0, i, i + 1);
    }
    p.add(corner);
    p.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 8.0), r.max), CornerRadius::ZERO, pal().bg_tertiary);
    let h = 0.5;
    let mut edge = vec![pos2(r.left() + h, r.bottom())];
    edge.extend(arc.iter().map(|q| *q + vec2(h, h)));
    edge.push(pos2(r.right(), r.top() + h));
    p.add(egui::Shape::line(edge, egui::Stroke::new(1.0, pal().frame_border)));
    if onyx() {
        p.vline(r.right() - h, r.y_range(), hairline(p.pixels_per_point(), pal().border_subtle));
    }
}

/// ":)" and friends become emoji, but only as whole words, so urls and
/// code like `a:b` are left alone.
pub(super) fn emoticons(text: &str) -> String {
    const MAP: [(&str, &str); 14] = [
        (":)", "🙂"), (":-)", "🙂"), (":(", "🙁"), (":-(", "🙁"), (":D", "😃"), (":-D", "😃"), (";)", "😉"),
        (";-)", "😉"), (":P", "😛"), (":p", "😛"), ("<3", "❤️"), (":O", "😮"), (":o", "😮"), (":'(", "😢"),
    ];
    if text.contains("```") {
        return text.to_string();
    }
    text.split(' ')
        .map(|w| MAP.iter().find(|(k, _)| *k == w).map(|(_, e)| e.to_string()).unwrap_or_else(|| w.to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

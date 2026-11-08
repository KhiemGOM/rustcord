use super::*;

pub(super) fn poll_left(expiry: i64) -> String {
    let ms = expiry - chrono::Utc::now().timestamp_millis();
    if ms <= 0 {
        return "Poll closed".into();
    }
    let mins = ms / 60_000;
    if mins >= 1440 {
        format!("{}d left", mins / 1440)
    } else if mins >= 60 {
        format!("{}h left", mins / 60)
    } else {
        format!("{}m left", mins.max(1))
    }
}

/// A poll, laid out like discord's: question, answers you pick with a Vote button, then the
/// result bars once you've voted, asked to see them, or the poll's closed.
pub(super) fn poll(ui: &mut Ui, d: &mut D, m: &Message) {
    let Some(p) = &m.poll else { return };
    let key = egui::Id::new(("poll", m.id));
    let (mut pending, mut show): (Vec<u8>, bool) = ui.data(|s| s.get_temp(key)).unwrap_or_default();
    let voted = p.answers.iter().any(|a| a.me);
    let results = p.finalized || voted || show;
    let total = p.total().max(1);
    let w = ui.available_width().min(440.0);

    Frame::new()
        .fill(pal().bg_secondary)
        .stroke(Stroke::new(1.0, pal().border_muted))
        .corner_radius(8)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            text::label(ui, &p.question, semi(16.0), pal().header_primary, Sense::hover());
            if !results {
                ui.label(RichText::new(if p.multi { "Select one or more answers" } else { "Select one answer" }).font(reg(14.0)).color(pal().text_muted));
            }
            ui.add_space(8.0);
            for a in &p.answers {
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                if !results && ui.clip_rect().contains_rect(row) {
                    d.rects.push((format!("poll_opt:{}", a.id), row));
                }
                let picked = if results { a.me } else { pending.contains(&a.id) };
                if results {
                    let frac = a.count as f32 / total as f32;
                    ui.painter().rect_filled(row, CornerRadius::same(8), pal().input_bg);
                    let bar = Rect::from_min_size(row.min, vec2(row.width() * frac, row.height()));
                    let fill = if a.me { Color32::from_rgb(0x3d, 0x45, 0xa5) } else { pal().control };
                    ui.painter().rect_filled(bar, CornerRadius::same(8), fill);
                } else {
                    let fill = crate::ui::motion::mix(pal().input_bg, pal().bg_accent, crate::ui::motion::hot(ui.ctx(), &resp));
                    ui.painter().rect(row, CornerRadius::same(8), fill, Stroke::new(1.0, if picked { pal().blurple } else { Color32::TRANSPARENT }), egui::StrokeKind::Inside);
                }
                let mut x = row.left() + 14.0;
                if let Some(em) = &a.emoji {
                    let er = Rect::from_min_size(pos2(x, row.center().y - 9.0), vec2(18.0, 18.0));
                    Image::from_uri(gif::uri(ui.ctx(), &emoji_url(em), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
                    x += 26.0;
                }
                ui.painter().text(pos2(x, row.center().y), Align2::LEFT_CENTER, &a.text, reg(16.0), pal().header_primary);
                if results {
                    let pct = (a.count as f32 / total as f32 * 100.0).round() as u32;
                    ui.painter().text(pos2(row.right() - 14.0, row.center().y), Align2::RIGHT_CENTER, format!("{pct}%"), medium(14.0), pal().header_secondary);
                } else {
                    let c = pos2(row.right() - 24.0, row.center().y);
                    if picked {
                        ui.painter().circle_filled(c, 10.0, pal().blurple);
                        ui.painter().circle_filled(c, 4.0, Color32::WHITE);
                    } else {
                        ui.painter().circle_stroke(c, 9.0, Stroke::new(2.0, pal().text_muted));
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        if p.multi {
                            if let Some(i) = pending.iter().position(|x| *x == a.id) {
                                pending.remove(i);
                            } else {
                                pending.push(a.id);
                            }
                        } else {
                            pending = vec![a.id];
                        }
                    }
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let votes = format!("{} vote{}", p.total(), if p.total() == 1 { "" } else { "s" });
                let tail = if p.finalized { "Poll closed".to_string() } else { poll_left(p.expiry) };
                let count = ui.add(Label::new(RichText::new(votes).font(reg(14.0)).color(pal().text_muted)).selectable(false).sense(if results { Sense::click() } else { Sense::hover() }));
                if results {
                    if ui.clip_rect().contains_rect(count.rect) {
                        d.rects.push(("poll_votes_btn".into(), count.rect));
                    }
                    if count.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if count.on_hover_text("View Votes").clicked() {
                        d.acts.push(Act::Votes(m.id));
                    }
                }
                ui.label(RichText::new(format!(" \u{2022} {tail}")).font(reg(14.0)).color(pal().text_muted));
                if !p.finalized {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if results {
                            if voted && link_label(ui, "Remove your vote").clicked() {
                                d.acts.push(Act::Vote(m.id, vec![]));
                            } else if !voted && show && link_label(ui, "Back to vote").clicked() {
                                show = false;
                            }
                        } else {
                            let btn = Frame::new().fill(if pending.is_empty() { pal().control } else { pal().blurple }).corner_radius(8).inner_margin(Margin::symmetric(16, 8)).show(ui, |ui| {
                                ui.label(RichText::new("Vote").font(medium(14.0)).color(if pending.is_empty() { pal().text_muted } else { Color32::WHITE }));
                            });
                            let r = btn.response.interact(Sense::click());
                            if ui.clip_rect().contains_rect(r.rect) {
                                d.rects.push(("poll_vote".into(), r.rect));
                            }
                            if !pending.is_empty() {
                                if r.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if r.clicked() {
                                    d.acts.push(Act::Vote(m.id, std::mem::take(&mut pending)));
                                }
                            }
                            ui.add_space(8.0);
                            if link_label(ui, "Show results").clicked() {
                                show = true;
                            }
                        }
                    });
                }
            });
        });
    ui.data_mut(|s| s.insert_temp(key, (pending, show)));
}

pub(super) fn link_label(ui: &mut Ui, text: &str) -> egui::Response {
    let r = ui.add(Label::new(RichText::new(text).font(medium(14.0)).color(pal().header_secondary)).selectable(false).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

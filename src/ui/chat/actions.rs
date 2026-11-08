use super::*;

pub(super) fn action_bar(d: &mut D, m: &Message, row: Rect) {
    let own = m.author == d.store.me;
    if d.ctx.input(|i| i.modifiers.shift) {
        return shift_bar(d, m, row, own);
    }
    // discord's bar: three one-click reactions, a rule, then react, edit (own), reply, forward, more
    let quick: Vec<ReactEmoji> = d.quick.iter().take(3).cloned().collect();
    let n = quick.len() as f32 + if own { 5.0 } else { 4.0 };
    let size = vec2(n * 32.0 + 9.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    let rect = Rect::from_min_size(pos, size);
    d.bar = Some((m.id, rect));
    // under popouts and menus, which would otherwise lose clicks to a toolbar left over them
    let bar_in = crate::ui::motion::enter(d.ctx, egui::Id::new(("msg_bar", m.id)), 0.15, crate::ui::motion::EASE);
    let bar = egui::Area::new(egui::Id::new("msg_bar")).order(egui::Order::Middle).fixed_pos(pos).fade_in(false);
    d.ctx.set_transform_layer(bar.layer(), egui::emath::TSTransform::from_translation(vec2(0.0, 2.0 * (1.0 - bar_in))));
    bar.show(d.ctx, |ui| {
            ui.multiply_opacity(bar_in);
            Frame::new()
                .fill(pal().bg_secondary)
                .stroke(Stroke::new(1.0, pal().border_subtle))
                .corner_radius(8)
                .inner_margin(1)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for e in quick {
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(8), pal().control_hover, 0.05);
                            let er = Rect::from_center_size(r.center(), vec2(20.0, 20.0));
                            Image::from_uri(gif::uri(ui.ctx(), &emoji_url(&e), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
                            if resp.on_hover_text("Click to react").clicked() {
                                d.acts.push(Act::React(m.id, e.clone(), true));
                            }
                        }
                        let (rule, _) = ui.allocate_exact_size(vec2(9.0, 32.0), Sense::hover());
                        ui.painter().vline(rule.center().x, rule.y_range().shrink(8.0), Stroke::new(1.0, pal().border_subtle));
                        let r = icon_button(ui, Icon::AddReaction, 32.0, "Add Reaction");
                        if r.clicked() {
                            d.acts.push(Act::Picker(m.id, r.rect.left_bottom()));
                        }
                        if own && icon_button(ui, Icon::Pencil, 32.0, "Edit").clicked() {
                            d.acts.push(Act::Edit(m.id));
                        }
                        let rr = icon_button(ui, Icon::Reply, 32.0, "Reply");
                        d.rects.push(("bar_reply".into(), rr.rect));
                        if rr.clicked() {
                            d.acts.push(Act::Reply(m.id));
                        }
                        if icon_button(ui, Icon::Forward, 32.0, "Forward").clicked() {
                            d.acts.push(Act::Forward(m.id));
                        }
                        let more = icon_button(ui, Icon::More, 32.0, "More");
                        if more.clicked() {
                            d.acts.push(Act::Menu(m.id, more.rect.left_bottom()));
                        }
                    });
                });
        });
}

/// A message that never went out gets two buttons: send it again, or throw it away.
pub(super) fn failed_bar(d: &mut D, m: &Message, row: Rect) {
    let size = vec2(2.0 * 32.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    d.bar = Some((m.id, Rect::from_min_size(pos, size)));
    egui::Area::new(egui::Id::new("msg_bar")).order(egui::Order::Middle).fixed_pos(pos).show(d.ctx, |ui| {
        Frame::new().fill(pal().bg_secondary).stroke(Stroke::new(1.0, pal().border_subtle)).corner_radius(8).inner_margin(1).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let r = icon_button(ui, Icon::Refresh, 32.0, "Resend message");
                d.rects.push(("bar_resend".into(), r.rect));
                if r.clicked() {
                    d.acts.push(Act::Resend(m.id));
                }
                let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(8), pal().control_hover, 0.05);
                icons::draw(ui.painter(), Icon::Trash, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().red);
                d.rects.push(("bar_delete".into(), r));
                if resp.on_hover_text("Delete Message").clicked() {
                    d.acts.push(Act::Delete(m.id));
                }
            });
        });
    });
}

/// What the bar turns into while shift is held: discord's long row of one-click actions in
/// place of the reactions, ending in a red delete that doesn't ask.
pub(super) fn shift_bar(d: &mut D, m: &Message, row: Rect, own: bool) {
    let manage = d.guild == 0 || d.moderator;
    let threads = d.guild != 0 && d.store.channels.get(&d.ch).is_some_and(|c| matches!(c.kind, 0 | 5)) && m.thread.is_none();
    let saved = d.bookmarks.contains(&m.id);
    let mut items: Vec<(Icon, &str, Act)> = Vec::new();
    if d.s.dev {
        items.push((Icon::Id, "Copy Message ID", Act::CopyId(m.id)));
    }
    items.push((Icon::Link, "Copy Link", Act::CopyLink(m.id)));
    items.push((Icon::MarkUnread, "Mark Unread", Act::MarkUnread(m.id)));
    items.push((if saved { Icon::Bookmarked } else { Icon::Bookmark }, if saved { "Remove Bookmark" } else { "Bookmark Message" }, Act::Bookmark(m.id)));
    if manage {
        items.push((Icon::Pin, if m.pinned { "Unpin Message" } else { "Pin Message" }, Act::Pin(m.id)));
    }
    if threads {
        items.push((Icon::ThreadPlus, "Create Thread", Act::Thread(m.id)));
    }
    items.push((Icon::Reply, "Reply", Act::Reply(m.id)));
    items.push((Icon::AddReaction, "Add Reaction", Act::Picker(m.id, Pos2::ZERO)));
    if own {
        items.push((Icon::Pencil, "Edit", Act::Edit(m.id)));
    }
    items.push((Icon::Forward, "Forward", Act::Forward(m.id)));
    let delete = own || manage;
    let n = items.len() as f32 + if delete { 1.0 } else { 0.0 };
    let size = vec2(n * 32.0 + 2.0, 34.0);
    let pos = pos2(row.right() - 14.0 - size.x, row.top() - 16.0);
    d.bar = Some((m.id, Rect::from_min_size(pos, size)));
    let bar_in = crate::ui::motion::enter(d.ctx, egui::Id::new(("msg_bar", m.id)), 0.15, crate::ui::motion::EASE);
    let bar = egui::Area::new(egui::Id::new("msg_bar")).order(egui::Order::Middle).fixed_pos(pos).fade_in(false);
    d.ctx.set_transform_layer(bar.layer(), egui::emath::TSTransform::from_translation(vec2(0.0, 2.0 * (1.0 - bar_in))));
    bar.show(d.ctx, |ui| {
            ui.multiply_opacity(bar_in);
            Frame::new()
                .fill(pal().bg_secondary)
                .stroke(Stroke::new(1.0, pal().border_subtle))
                .corner_radius(8)
                .inner_margin(1)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (icon, tip, act) in items {
                            let r = icon_button(ui, icon, 32.0, tip);
                            if r.clicked() {
                                d.acts.push(match act {
                                    Act::Picker(id, _) => Act::Picker(id, r.rect.left_bottom()),
                                    a => a,
                                });
                            }
                        }
                        if delete {
                            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            crate::ui::motion::wash(ui, &resp, r, CornerRadius::same(8), pal().control_hover, 0.05);
                            icons::draw(ui.painter(), Icon::Trash, Rect::from_center_size(r.center(), vec2(20.0, 20.0)), pal().red);
                            d.rects.push(("bar_delete".into(), r));
                            if resp.on_hover_text("Delete").clicked() {
                                d.acts.push(Act::Delete(m.id));
                            }
                        }
                    });
                });
        });
}

pub(super) fn edit_box(ui: &mut Ui, d: &mut D, id: Id) {
    let Some((_, text)) = d.editing.as_mut() else { return };
    let tid = egui::Id::new("edit_box");
    let enter = ui.memory(|m| m.has_focus(tid)) && ui.input_mut(|i| crate::ui::take_key(i, Modifiers::NONE, Key::Enter));
    let esc = ui.input_mut(|i| crate::ui::take_key(i, Modifiers::NONE, Key::Escape));
    ui.add_space(4.0);
    Frame::new().fill(pal().bg_input).corner_radius(8).inner_margin(Margin::symmetric(16, 11)).show(ui, |ui| {
        let r = ui.add(
            egui::TextEdit::multiline(text)
                .id(tid)
                .frame(Frame::NONE)
                .desired_rows(1)
                .font(reg(16.0))
                .text_color(pal().text_normal)
                .desired_width(ui.available_width()),
        );
        if !r.has_focus() && !enter {
            r.request_focus();
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let small = |s: &str, c: Color32| RichText::new(s).font(reg(12.0)).color(c);
        ui.label(small("escape to", pal().text_normal));
        if ui.add(Label::new(small("cancel", pal().link)).sense(Sense::click())).clicked() {
            d.acts.push(Act::CancelEdit);
        }
        ui.label(small("• enter to", pal().text_normal));
        if ui.add(Label::new(small("save", pal().link)).sense(Sense::click())).clicked() {
            d.acts.push(Act::SaveEdit(id, text.clone()));
        }
    });
    if enter {
        d.acts.push(Act::SaveEdit(id, text.clone()));
    } else if esc {
        d.acts.push(Act::CancelEdit);
    }
}

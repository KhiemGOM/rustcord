//! Multi-tab channel bar (Phoenix-style channel tabs): middle-click any channel or DM, or press
//! Ctrl+T to pin the current channel into the tab strip. Hidden until at least one tab is pinned.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, PointerButton, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::sync::{LazyLock, Mutex};

use super::icons::{self, Icon};
use super::widgets::TipExt;
use super::{App, badge, channel_title, text};
use crate::state::{Id, Store};
use crate::theme::*;

pub const HEIGHT: f32 = 32.0;
const MAX_TABS: usize = 16;

static CLOSED_TABS: LazyLock<Mutex<Vec<Id>>> = LazyLock::new(Default::default);

pub fn select_index(tabs: &[Id], one_based: usize) -> Option<Id> {
    if tabs.is_empty() || one_based == 0 {
        return None;
    }
    if one_based >= 9 || one_based >= tabs.len() {
        tabs.last().copied()
    } else {
        tabs.get(one_based - 1).copied()
    }
}

pub fn pin(tabs: &mut Vec<Id>, ch: Id) {
    if ch == 0 || tabs.contains(&ch) {
        return;
    }
    if tabs.len() >= MAX_TABS {
        tabs.remove(0);
    }
    tabs.push(ch);
}

pub fn close(tabs: &mut Vec<Id>, ch: Id) -> Option<Id> {
    let idx = tabs.iter().position(|&id| id == ch)?;
    tabs.remove(idx);
    if tabs.is_empty() {
        return None;
    }
    let next = idx.min(tabs.len() - 1);
    Some(tabs[next])
}

pub fn step(tabs: &[Id], cur: Option<Id>, forward: bool) -> Option<Id> {
    if tabs.len() < 2 {
        return None;
    }
    let len = tabs.len();
    let idx = cur.and_then(|c| tabs.iter().position(|&id| id == c)).unwrap_or(0);
    let next = if forward { (idx + 1) % len } else { (idx + len - 1) % len };
    Some(tabs[next])
}

pub fn prune(tabs: &mut Vec<Id>, store: &Store) {
    tabs.retain(|id| store.channels.contains_key(id));
}

impl App {
    pub fn open_tab(&mut self, ch: Id) {
        if !self.store.channels.contains_key(&ch) {
            return;
        }
        if let Some(cur) = self.cur()
            && self.ui.tabs.is_empty()
            && cur != ch
        {
            pin(&mut self.ui.tabs, cur);
        }
        pin(&mut self.ui.tabs, ch);
        let g = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        self.ui.guild = (g != 0).then_some(g);
        self.open(ch);
        self.ui_save_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
    }

    pub fn close_tab(&mut self, ch: Id) {
        let was_cur = self.cur() == Some(ch);
        let existed = self.ui.tabs.contains(&ch);
        let next = close(&mut self.ui.tabs, ch);
        if existed && let Ok(mut hist) = CLOSED_TABS.lock() {
            hist.retain(|id| *id != ch);
            hist.push(ch);
            if hist.len() > MAX_TABS {
                hist.remove(0);
            }
        }
        self.ui_save_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
        if was_cur && let Some(n) = next {
            let g = self.store.channels.get(&n).map_or(0, |c| c.guild);
            self.ui.guild = (g != 0).then_some(g);
            self.open(n);
        }
    }

    pub fn reopen_closed_tab(&mut self) -> bool {
        let ch = CLOSED_TABS.lock().ok().and_then(|mut h| {
            while let Some(id) = h.pop() {
                if self.store.channels.contains_key(&id) && !self.ui.tabs.contains(&id) {
                    return Some(id);
                }
            }
            None
        });
        let Some(ch) = ch else { return false };
        self.open_tab(ch);
        true
    }

    pub fn goto_tab(&mut self, one_based: usize) -> bool {
        prune(&mut self.ui.tabs, &self.store);
        let Some(ch) = select_index(&self.ui.tabs, one_based) else { return false };
        let g = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        self.ui.guild = (g != 0).then_some(g);
        self.open(ch);
        true
    }

    pub fn step_tab(&mut self, forward: bool) -> bool {
        prune(&mut self.ui.tabs, &self.store);
        let Some(next) = step(&self.ui.tabs, self.cur(), forward) else {
            return false;
        };
        let g = self.store.channels.get(&next).map_or(0, |c| c.guild);
        self.ui.guild = (g != 0).then_some(g);
        self.open(next);
        true
    }
}

pub fn bar(app: &mut App, ui: &mut Ui) {
    prune(&mut app.ui.tabs, &app.store);
    if app.ui.tabs.is_empty() {
        return;
    }
    let cur = app.cur();
    let mut activate = None;
    let mut closing = None;
    let mut add = false;
    egui::Panel::top("channel_tabs")
        .exact_size(HEIGHT)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal().bg_tertiary).inner_margin(Margin::symmetric(8, 0)))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().hline(full.left() - 8.0..=full.right() + 8.0, full.bottom() - 0.5, Stroke::new(1.0, pal().frame_border));
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let n = app.ui.tabs.len().max(1);
                let avail = (ui.available_width() - 32.0).max(120.0);
                let tab_w = ((avail - 4.0 * n as f32) / n as f32).clamp(88.0, 180.0);
                let tabs = app.ui.tabs.clone();
                for ch in tabs {
                    let Some(c) = app.store.channels.get(&ch) else { continue };
                    let active = cur == Some(ch) && !app.friends && app.quests.is_none() && app.shop.is_none();
                    let (r, resp) = ui.allocate_exact_size(vec2(tab_w, 26.0), Sense::click());
                    let fill = if active {
                        pal().bg_primary
                    } else if resp.hovered() {
                        pal().bg_mod_hover
                    } else {
                        Color32::TRANSPARENT
                    };
                    if fill != Color32::TRANSPARENT {
                        ui.painter().rect_filled(r, CornerRadius::same(6), fill);
                    }
                    let unread = c.is_text() && app.store.unread(c);
                    let men = app.store.mentions(ch);
                    let icon = if c.guild == 0 {
                        Icon::Bubble
                    } else if c.is_voice() {
                        Icon::Speaker
                    } else if c.is_forum() {
                        Icon::Forum
                    } else if c.is_thread() {
                        Icon::Thread
                    } else {
                        Icon::Hash
                    };
                    let fg = if active || unread { pal().header_primary } else { pal().channel_default };
                    icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), vec2(14.0, 14.0)), fg);
                    let close_r = Rect::from_center_size(pos2(r.right() - 12.0, r.center().y), vec2(16.0, 16.0));
                    let show_close = active || resp.hovered();
                    let right_pad = if show_close { 24.0 } else if men > 0 { 26.0 } else if unread { 16.0 } else { 8.0 };
                    let title = channel_title(&app.store, ch);
                    text::paint_em(ui.painter(), pos2(r.left() + 24.0, r.center().y), Align2::LEFT_CENTER, &title, medium(13.0), fg, (tab_w - 24.0 - right_pad).max(16.0), 1.0);
                    if show_close {
                        let cr = ui.interact(close_r, ui.id().with(("tab_x", ch)), Sense::click());
                        let ink = if cr.hovered() { pal().header_primary } else { pal().header_secondary };
                        icons::draw(ui.painter(), Icon::Close, close_r.shrink(2.0), ink);
                        if cr.clicked() {
                            closing = Some(ch);
                        }
                    } else if men > 0 {
                        badge(ui, pos2(r.right() - 12.0, r.center().y), men, None);
                    } else if unread {
                        ui.painter().circle_filled(pos2(r.right() - 10.0, r.center().y), 3.0, pal().header_primary);
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked_by(PointerButton::Middle) {
                        closing = Some(ch);
                    } else if resp.clicked() && closing != Some(ch) {
                        activate = Some(ch);
                    }
                }
                let (pr, presp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                if presp.hovered() {
                    ui.painter().rect_filled(pr, CornerRadius::same(6), pal().bg_mod_hover);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                icons::draw(ui.painter(), Icon::Plus, pr.shrink(4.0), pal().header_secondary);
                if presp.tip("Open channel in new tab (Ctrl+K)").clicked() {
                    add = true;
                }
            });
        });
    if let Some(ch) = closing {
        app.close_tab(ch);
    } else if let Some(ch) = activate {
        let g = app.store.channels.get(&ch).map_or(0, |c| c.guild);
        app.ui.guild = (g != 0).then_some(g);
        app.open(ch);
    }
    if add {
        app.switcher = Some(String::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_deduplicates_and_caps_at_max() {
        let mut tabs = Vec::new();
        for i in 1..=20 {
            pin(&mut tabs, i);
        }
        assert_eq!(tabs.len(), MAX_TABS);
        assert_eq!(tabs[0], 5);
        pin(&mut tabs, 10);
        assert_eq!(tabs.len(), MAX_TABS);
    }

    #[test]
    fn close_and_step_wrap_cleanly() {
        let mut tabs = vec![10, 20, 30];
        assert_eq!(step(&tabs, Some(10), true), Some(20));
        assert_eq!(step(&tabs, Some(10), false), Some(30));
        assert_eq!(step(&tabs, Some(30), true), Some(10));
        assert_eq!(close(&mut tabs, 20), Some(30));
        assert_eq!(tabs, vec![10, 30]);
        assert_eq!(close(&mut tabs, 30), Some(10));
        assert_eq!(close(&mut tabs, 10), None);
        assert!(tabs.is_empty());
        let list = [11, 22, 33, 44];
        assert_eq!(select_index(&list, 1), Some(11));
        assert_eq!(select_index(&list, 3), Some(33));
        assert_eq!(select_index(&list, 9), Some(44));
        assert_eq!(select_index(&[], 1), None);
    }
}

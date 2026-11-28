//! Multi-tab channel bar (Phoenix-style channel tabs): middle-click any channel or DM, or press
//! Ctrl+T to pin the current channel into the title bar tab strip.

use egui::{Align2, Color32, CornerRadius, PointerButton, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::sync::{LazyLock, Mutex};

use super::icons::{self, Icon};
use super::widgets::TipExt;
use super::{App, avatar, badge, channel_title, text};
use crate::state::{Id, Set, Store};
use crate::theme::*;

pub const CLASSIC_HEIGHT: f32 = 32.0;
pub const TABBED_HEIGHT: f32 = 40.0;
const MAX_TABS: usize = 16;

static CLOSED_TABS: LazyLock<Mutex<Vec<Id>>> = LazyLock::new(Default::default);

pub fn titlebar_height(signed_in: bool, tabs: &[Id]) -> f32 {
    if signed_in && !tabs.is_empty() { TABBED_HEIGHT } else { CLASSIC_HEIGHT }
}

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

pub fn close_others(tabs: &mut Vec<Id>, keep: Id, pinned: &Set<Id>) {
    tabs.retain(|id| *id == keep || pinned.contains(id));
}

pub fn ordered(tabs: &[Id], pinned: &Set<Id>) -> Vec<Id> {
    let mut out = Vec::with_capacity(tabs.len());
    out.extend(tabs.iter().copied().filter(|id| pinned.contains(id)));
    out.extend(tabs.iter().copied().filter(|id| !pinned.contains(id)));
    out
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

fn voice_elapsed(secs: i64) -> String {
    let s = secs.max(0);
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
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
        self.pinned_tabs.remove(&ch);
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
        let list = ordered(&self.ui.tabs, &self.pinned_tabs);
        let Some(ch) = select_index(&list, one_based) else { return false };
        let g = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        self.ui.guild = (g != 0).then_some(g);
        self.open(ch);
        true
    }

    pub fn step_tab(&mut self, forward: bool) -> bool {
        prune(&mut self.ui.tabs, &self.store);
        let list = ordered(&self.ui.tabs, &self.pinned_tabs);
        let Some(next) = step(&list, self.cur(), forward) else {
            return false;
        };
        let g = self.store.channels.get(&next).map_or(0, |c| c.guild);
        self.ui.guild = (g != 0).then_some(g);
        self.open(next);
        true
    }
}

enum TabAct {
    Pin(Id),
    CloseOthers(Id),
    Close(Id),
}

/// Phoenix's extra title-bar controls (`Friends` + Quick Switcher `Search`) placed left of `inbox`.
/// Returns the left edge available for the tab strip or centered window title.
pub fn titlebar_right_controls(app: &mut App, ui: &mut Ui, inbox: Rect, mid: f32) -> f32 {
    let hover = Color32::from_rgba_unmultiplied_const(0x97, 0x97, 0x9f, 31);
    let idle = pal().header_secondary;
    let lit = pal().interactive_hover;
    let at = |x: f32| Rect::from_min_size(pos2(x, mid - 12.0), vec2(24.0, 24.0));

    let friends = at(inbox.left() - 8.0 - 24.0);
    app.rects.insert("title:friends".into(), friends);
    let fresp = ui.interact(friends, egui::Id::new(("title", "friends")), Sense::click());
    let f_active = app.ui.guild.is_none() && app.friends;
    if fresp.hovered() {
        ui.painter().rect_filled(friends, CornerRadius::same(6), hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::draw(ui.painter(), Icon::Friends, Rect::from_center_size(friends.center(), vec2(16.0, 16.0)), if fresp.hovered() || f_active { lit } else { idle });
    let incoming = app.store.relationships.values().filter(|t| **t == crate::state::INCOMING).count() as u32;
    if incoming > 0 {
        badge(ui, friends.center() + vec2(10.0, -6.0), incoming, Some(pal().bg_tertiary));
    }
    if fresp.tip("Friends").clicked() {
        app.ui.guild = None;
        app.friends = true;
        app.icymi = false;
        app.quests = None;
        app.shop = None;
        app.msg_requests = None;
    }

    let search = at(friends.left() - 8.0 - 24.0);
    app.rects.insert("title:search".into(), search);
    let sresp = ui.interact(search, egui::Id::new(("title", "search")), Sense::click());
    if sresp.hovered() {
        ui.painter().rect_filled(search, CornerRadius::same(6), hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::draw(ui.painter(), Icon::Search, Rect::from_center_size(search.center(), vec2(16.0, 16.0)), if sresp.hovered() { lit } else { idle });
    if sresp.tip("Search (Ctrl+K)").clicked() {
        app.switcher_new_tab = false;
        app.switcher = Some(String::new());
    }
    search.left()
}

/// Phoenix's `AppTitleBar` channel tab strip, rendered directly inside the 40px title bar.
pub fn titlebar_strip(app: &mut App, ui: &mut Ui, area: Rect) {
    prune(&mut app.ui.tabs, &app.store);
    app.pinned_tabs.retain(|id| app.ui.tabs.contains(id));
    if app.ui.tabs.is_empty() || area.width() < 60.0 {
        return;
    }
    let cur = app.cur();
    let on_home = app.ui.guild.is_none() && (app.friends || app.icymi || app.quests.is_some() || app.shop.is_some() || app.msg_requests.is_some());
    let tabs = ordered(&app.ui.tabs, &app.pinned_tabs);
    let pin_count = tabs.iter().filter(|id| app.pinned_tabs.contains(id)).count();
    let norm_count = tabs.len().saturating_sub(pin_count);
    let plus_w = 28.0;
    let gap = 4.0;
    let pins_total = pin_count as f32 * (36.0 + gap);
    let avail = (area.width() - plus_w - gap - pins_total).max(60.0);
    let tab_w = if norm_count > 0 {
        ((avail - gap * norm_count as f32) / norm_count as f32).clamp(84.0, 240.0)
    } else {
        240.0
    };

    let mut activate = None;
    let mut act = None;
    let mut add = false;
    let mut x = area.left();

    for ch in tabs {
        let Some(c) = app.store.channels.get(&ch) else { continue };
        let is_pinned = app.pinned_tabs.contains(&ch);
        let w = if is_pinned { 36.0 } else { tab_w };
        if x + w > area.right() - plus_w - gap {
            break;
        }
        let active = !on_home && cur == Some(ch);
        let h = if active { 34.0 } else { 30.0 };
        let r = if active {
            Rect::from_min_size(pos2(x, area.bottom() - h), vec2(w, h))
        } else {
            Rect::from_min_size(pos2(x, area.center().y - h / 2.0), vec2(w, h))
        };
        let resp = ui.interact(r, egui::Id::new(("title_tab", ch)), Sense::click());
        let cr = if active {
            CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }
        } else {
            CornerRadius::same(7)
        };
        if active {
            ui.painter().rect_filled(r, cr, pal().bg_primary);
            ui.painter().rect_stroke(r, cr, Stroke::new(1.0, pal().frame_border), egui::StrokeKind::Inside);
            // mask the bottom stroke so the active tab merges into the workspace border
            ui.painter().hline(r.left() + 1.0..=r.right() - 1.0, r.bottom() - 0.5, Stroke::new(1.5, pal().bg_primary));
        } else if resp.hovered() {
            ui.painter().rect_filled(r, cr, pal().bg_mod_hover);
        }

        let unread = c.is_text() && app.store.unread(c);
        let men = app.store.mentions(ch);
        let in_voice = app.voice.chan == Some(ch);
        let fg = if active || unread { pal().header_primary } else { pal().channel_default };
        let title = channel_title(&app.store, ch);

        let icon_center = if is_pinned { r.center() } else { pos2(r.left() + 17.0, r.center().y) };
        let icon_rect = Rect::from_center_size(icon_center, vec2(18.0, 18.0));
        if c.kind == 1
            && let Some(u) = c.recipients.first().and_then(|u| app.store.user(*u))
        {
            avatar(ui, icon_rect, &u.avatar_url());
        } else {
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
            icons::draw(ui.painter(), icon, icon_rect, if in_voice { pal().positive } else { fg });
        }

        if is_pinned {
            if men > 0 {
                badge(ui, r.right_bottom() - vec2(6.0, 6.0), men, Some(pal().bg_tertiary));
            } else if unread {
                ui.painter().circle_filled(pos2(r.right() - 6.0, r.top() + 7.0), 3.0, pal().header_primary);
            }
        } else {
            let close_r = Rect::from_center_size(pos2(r.right() - 14.0, r.center().y), vec2(20.0, 20.0));
            let show_close = active || resp.hovered();
            let timer = in_voice
                .then(|| app.store.voice_start.get(&ch).map(|t| voice_elapsed(chrono::Utc::now().timestamp() - t)))
                .flatten();
            let timer_w = timer.as_ref().map_or(0.0, |t| ui.painter().layout_no_wrap(t.clone(), medium(11.0), pal().positive).size().x + 8.0);
            let right_pad = if show_close { 26.0 } else if men > 0 { 28.0 } else if unread { 18.0 } else { 10.0 } + timer_w;
            text::paint_em(ui.painter(), pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, &title, medium(14.0), fg, (w - 30.0 - right_pad).max(16.0), 1.0);
            if let Some(ts) = timer {
                let tx = r.right() - if show_close { 28.0 } else { 10.0 };
                ui.painter().text(pos2(tx, r.center().y), Align2::RIGHT_CENTER, ts, medium(11.0), pal().positive);
            }
            if show_close {
                let cr_resp = ui.interact(close_r, egui::Id::new(("title_tab_x", ch)), Sense::click());
                if cr_resp.hovered() {
                    ui.painter().rect_filled(close_r, CornerRadius::same(4), pal().bg_mod_hover);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let ink = if cr_resp.hovered() { pal().header_primary } else { pal().header_secondary };
                icons::draw(ui.painter(), Icon::Close, Rect::from_center_size(close_r.center(), vec2(12.0, 12.0)), ink);
                if cr_resp.tip("Close Tab").clicked() {
                    act = Some(TabAct::Close(ch));
                }
            } else if men > 0 {
                badge(ui, pos2(r.right() - 14.0, r.center().y), men, None);
            } else if unread {
                ui.painter().circle_filled(pos2(r.right() - 11.0, r.center().y), 3.0, pal().header_primary);
            }
        }

        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked_by(PointerButton::Middle) {
            act = Some(TabAct::Close(ch));
        } else if resp.clicked() && !matches!(act, Some(TabAct::Close(id)) if id == ch) {
            activate = Some(ch);
        }
        let resp = if is_pinned { resp.tip(title) } else { resp };
        egui::Popup::context_menu(&resp).close_behavior(super::menu_closing(ui.ctx())).show(|ui| {
            super::menu_body(ui, |ui| {
                if super::menu_item(ui, Some(Icon::Pin), if is_pinned { "Unpin Tab" } else { "Pin Tab" }, false) {
                    act = Some(TabAct::Pin(ch));
                }
                if super::menu_item(ui, None, "Close Other Tabs", false) {
                    act = Some(TabAct::CloseOthers(ch));
                }
                if super::menu_item(ui, Some(Icon::Close), "Close Tab", true) {
                    act = Some(TabAct::Close(ch));
                }
            });
        });
        x += w + gap;
    }

    let plus_r = Rect::from_min_size(pos2(x, area.center().y - 14.0), vec2(plus_w, 28.0));
    let presp = ui.interact(plus_r, egui::Id::new("title_tab_new"), Sense::click());
    if presp.hovered() {
        ui.painter().rect_filled(plus_r, CornerRadius::same(7), pal().bg_mod_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::draw(ui.painter(), Icon::Plus, Rect::from_center_size(plus_r.center(), vec2(16.0, 16.0)), if presp.hovered() { pal().interactive_hover } else { pal().header_secondary });
    if presp.tip("New Tab").clicked() {
        add = true;
    }

    match act {
        Some(TabAct::Pin(ch)) => {
            if !app.pinned_tabs.remove(&ch) {
                app.pinned_tabs.insert(ch);
            }
        }
        Some(TabAct::CloseOthers(ch)) => {
            close_others(&mut app.ui.tabs, ch, &app.pinned_tabs);
            app.ui_save_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
        }
        Some(TabAct::Close(ch)) => app.close_tab(ch),
        None => {}
    }
    if let Some(ch) = activate {
        let g = app.store.channels.get(&ch).map_or(0, |c| c.guild);
        app.ui.guild = (g != 0).then_some(g);
        app.open(ch);
    }
    if add {
        app.switcher_new_tab = true;
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
    fn close_step_pin_order_and_titlebar_height() {
        assert_eq!(titlebar_height(true, &[]), CLASSIC_HEIGHT);
        assert_eq!(titlebar_height(true, &[10]), TABBED_HEIGHT);
        assert_eq!(titlebar_height(false, &[10]), CLASSIC_HEIGHT);

        let mut tabs = vec![10, 20, 30];
        let mut pinned = Set::default();
        pinned.insert(30);
        assert_eq!(ordered(&tabs, &pinned), vec![30, 10, 20]);
        close_others(&mut tabs, 10, &pinned);
        assert_eq!(tabs, vec![10, 30]);

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
        assert_eq!(voice_elapsed(75), "01:15");
        assert_eq!(voice_elapsed(3661), "1:01:01");
    }
}

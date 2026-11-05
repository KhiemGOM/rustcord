//! Threads: opening one beside its parent channel or full size, the side card's header, and the
//! More menu both headers share with Join, Close, Lock and Delete.

use egui::{Align, Align2, Layout, Rect, Ui, pos2, vec2};
use serde_json::json;

use super::icons::{self, Icon};
use super::{App, Confirm, Trail, icon_button, menu_row, menu_sep, text};
use crate::net::{Cmd, Tag};
use crate::state::{Channel, Id, MANAGE_THREADS, Page};
use crate::theme::*;

enum Act {
    Join(bool),
    Top,
    Edit,
    Split(bool),
    Archive(bool),
    Lock(bool),
    Delete,
    Link,
    Id,
}

impl App {
    /// Discord's openThread: `full` is how the place it was clicked from opens threads, and
    /// Shift flips it. Full size, or with split view off, replaces the channel on screen.
    pub fn open_thread(&mut self, id: Id, full: bool, shift: bool) {
        let Some(c) = self.store.channels.get(&id) else { return };
        let parent = c.parent;
        if !c.is_thread() {
            return self.open(id);
        }
        if full != shift || !self.settings.thread_sidebar || !self.store.channels.contains_key(&parent) {
            self.side_thread = None;
            return self.open(id);
        }
        if self.cur() != Some(parent) {
            self.open(parent);
        }
        self.side_thread = Some(id);
        self.attach(id);
    }

    pub fn attach(&mut self, id: Id) {
        let Some(guild) = self.store.channels.get(&id).map(|c| c.guild) else { return };
        let e = self.store.msgs.entry(id).or_default();
        if !e.fresh && !e.loading {
            e.loading = true;
            self.net.send(Cmd::Messages { channel: id, page: Page::Latest });
        }
        self.subscribe_thread(guild, id);
    }

    pub fn subscribe_thread(&self, guild: Id, id: Id) {
        let d = json!({ "subscriptions": { guild.to_string(): { "thread_member_lists": [id.to_string()] } } });
        self.net.send(Cmd::Gateway(json!({ "op": 37, "d": d })));
    }

    fn thread_patch(&self, id: Id, body: serde_json::Value) {
        self.rest("PATCH", format!("/channels/{id}"), Some(body), Tag::Remote("action"));
    }

    fn act_on_thread(&mut self, c: &Channel, a: Act) {
        let id = c.id;
        match a {
            Act::Join(on) => {
                self.rest(if on { "PUT" } else { "DELETE" }, format!("/channels/{id}/thread-members/@me"), None, Tag::Remote("action"));
                if let Some(c) = self.store.channels.get_mut(&id) {
                    c.joined = on;
                }
                let me = self.store.me;
                if let Some(l) = self.store.thread_members.get_mut(&id) {
                    l.retain(|u| *u != me);
                    if on {
                        l.push(me);
                    }
                }
            }
            Act::Top => {
                let at_start = self.store.msgs.entry(id).or_default().at_start;
                if at_start {
                    self.scroll_fix = Some((id, 0.0));
                } else {
                    self.store.msgs.entry(id).or_default().loading = true;
                    self.jump_top = Some(id);
                    self.net.send(Cmd::Messages { channel: id, page: Page::Around(id) });
                }
            }
            Act::Edit => self.open_chan_settings(id),
            Act::Split(on) => {
                if on {
                    self.open_thread(id, false, false);
                } else {
                    self.side_thread = None;
                    self.open(id);
                }
            }
            Act::Archive(on) => self.thread_patch(id, json!({ "archived": on })),
            Act::Lock(on) => self.thread_patch(id, json!({ "locked": on })),
            Act::Delete => {
                let body = format!("Are you sure you want to delete **{}**? This cannot be undone.", c.name);
                self.confirm = Some(Confirm::new("Delete Thread", body, "Delete Thread", move |app| {
                    app.rest("DELETE", format!("/channels/{id}"), None, Tag::Remote("action"));
                    if app.side_thread == Some(id) {
                        app.side_thread = None;
                    }
                }));
            }
            Act::Link | Act::Id => {}
        }
    }
}

fn menu_item(ui: &mut Ui, icon: Option<Icon>, label: &str, danger: bool) -> bool {
    let r = menu_row(ui, icon, label, None, danger, Trail::None);
    super::settings::mark(ui, &format!("thread_menu:{label}"), r.rect);
    r.clicked()
}

/// The ⋯ button of a thread's header and its menu. `side` is true in the split card, where
/// Open in Split View becomes Open in Full View.
pub fn more(app: &mut App, ui: &mut Ui, chan: &Channel, side: bool) {
    let b = icon_button(ui, Icon::More, 32.0, "More");
    app.rects.insert(if side { "side_thread:more" } else { "thread:more" }.into(), b.rect);
    let manage = app.store.perms(chan) & MANAGE_THREADS != 0;
    let owner = !chan.locked && chan.owner == app.store.me;
    let can_edit = manage || owner;
    let dev = app.settings.dev;
    let mut act = None;
    egui::Popup::menu(&b).gap(4.0).show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let label = if chan.joined { "Leave Thread" } else { "Join Thread" };
        if menu_item(ui, None, label, false) {
            act = Some(Act::Join(!chan.joined));
        }
        menu_sep(ui);
        if menu_item(ui, None, "Jump To Top", false) {
            act = Some(Act::Top);
        }
        if can_edit && menu_item(ui, None, "Edit Thread", false) {
            act = Some(Act::Edit);
        }
        if menu_item(ui, None, if side { "Open in Full View" } else { "Open in Split View" }, false) {
            act = Some(Act::Split(!side));
        }
        if can_edit || manage {
            menu_sep(ui);
        }
        if can_edit && menu_item(ui, None, if chan.archived { "Open Thread" } else { "Close Thread" }, false) {
            act = Some(Act::Archive(!chan.archived));
        }
        if manage {
            if menu_item(ui, None, if chan.locked { "Unlock Thread" } else { "Lock Thread" }, false) {
                act = Some(Act::Lock(!chan.locked));
            }
            if menu_item(ui, None, "Delete Thread", true) {
                act = Some(Act::Delete);
            }
        }
        menu_sep(ui);
        if menu_item(ui, None, "Copy Link", false) {
            act = Some(Act::Link);
        }
        if dev && menu_item(ui, Some(Icon::Id), "Copy Thread ID", false) {
            act = Some(Act::Id);
        }
    });
    match act {
        Some(Act::Link) => ui.ctx().copy_text(format!("https://discord.com/channels/{}/{}", chan.guild, chan.id)),
        Some(Act::Id) => ui.ctx().copy_text(chan.id.to_string()),
        Some(a) => app.act_on_thread(chan, a),
        None => {}
    }
}

/// The split card beside the parent channel: the thread's name, its bell and menu, and a close.
pub fn side(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let mut close = false;
    super::chat::side_card(app, ui, chan, "side_thread", |app, ui, h| {
        let y = h.top() + 25.0;
        icons::draw(ui.painter(), Icon::Thread, Rect::from_center_size(pos2(h.left() + 26.0, y), vec2(20.0, 20.0)), pal().channel_default);
        let tools = 3.0 * 32.0 + 2.0 * 8.0 + 8.0;
        text::paint_em(ui.painter(), pos2(h.left() + 45.0, y), Align2::LEFT_CENTER, &chan.name, medium(16.0), pal().header_primary, h.width() - 45.0 - tools - 8.0, 1.0);
        let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(h.left(), h.top()), pos2(h.right() - 8.0, h.bottom() - 1.0))).layout(Layout::right_to_left(Align::Center)));
        bar.spacing_mut().item_spacing.x = 8.0;
        let x = icon_button(&mut bar, Icon::Close, 32.0, "Close");
        app.rects.insert("side_thread:close".into(), x.rect);
        close = x.clicked();
        more(app, &mut bar, chan, true);
        super::chat::bell(app, &mut bar, chan);
    });
    if close {
        app.side_thread = None;
    }
}

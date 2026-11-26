//! ICYMI / "For You" tab in the Inbox popout: ranked highlights across visible unmuted channels.

use egui::{Align2, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::{App, badge, channel_title, text};
use crate::state::{Channel, Id, Message, Store};
use crate::theme::*;

pub struct Highlight {
    pub ch: Id,
    pub msg: Option<Id>,
    pub score: u64,
    pub mentions: u32,
    pub preview: String,
    pub author: String,
}

const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

pub fn rank(store: &Store, only_guild: Option<Id>) -> Vec<Highlight> {
    rank_at(store, only_guild, chrono::Utc::now().timestamp_millis().max(0) as u64)
}

pub fn rank_at(store: &Store, only_guild: Option<Id>, now_ms: u64) -> Vec<Highlight> {
    let mut items = Vec::new();
    for c in store.channels.values() {
        if !c.is_text() || c.is_message_request() || c.is_spam() || store.muted.contains(&c.guild) || store.is_muted(c) || !store.can_view(c) {
            continue;
        }
        if let Some(g) = only_guild && c.guild != g && c.guild != 0 {
            continue;
        }
        let men = store.mentions(c.id);
        let unread = store.unread(c);
        if !unread && men == 0 {
            continue;
        }
        let latest: Option<&Message> = store.msgs.get(&c.id).and_then(|e| e.list.last());
        let react_bonus: u64 = latest.map_or(0, |m| m.reactions.iter().map(|r| r.count as u64).sum::<u64>().min(20) * 5);
        let dm_bonus: u64 = if c.guild == 0 { 200 } else { 0 };
        let msg_ms = DISCORD_EPOCH_MS.saturating_add(c.last_msg >> 22);
        let age_mins = now_ms.saturating_sub(msg_ms) / 60_000;
        let recency = 500u64.saturating_sub(age_mins.min(500));
        let score = (men as u64) * 1000 + dm_bonus + react_bonus + recency;
        let (msg, author, preview) = match latest {
            Some(m) => {
                let a = store.name_in(c.guild, m.author);
                let body = if !m.content.is_empty() {
                    m.content.lines().next().unwrap_or("").to_string()
                } else if !m.attachments.is_empty() {
                    format!("Sent {} attachment{}", m.attachments.len(), if m.attachments.len() == 1 { "" } else { "s" })
                } else if !m.embeds.is_empty() {
                    "Shared an embed".into()
                } else {
                    "New activity".into()
                };
                (Some(m.id), a, body)
            }
            None => (None, String::new(), "Unread messages".into()),
        };
        items.push(Highlight { ch: c.id, msg, score, mentions: men, preview, author });
    }
    items.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| b.ch.cmp(&a.ch)));
    items.truncate(20);
    items
}

pub fn height(app: &App) -> f32 {
    let scope = app.ui.guild.filter(|_| !app.settings.inbox_all);
    let n = rank(&app.store, scope).len();
    if n == 0 { 110.0 } else { 24.0 + n as f32 * 66.0 }
}

pub fn list(ui: &mut Ui, app: &mut App) -> (Option<Id>, Option<(Id, Id)>) {
    let scope = app.ui.guild.filter(|_| !app.settings.inbox_all);
    let items = rank(&app.store, scope);
    if items.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new("You're all caught up!").font(semi(16.0)).color(pal().header_primary));
            ui.add_space(4.0);
            ui.label(RichText::new("Highlights from your active servers and DMs will appear here.").font(reg(13.0)).color(pal().header_secondary));
        });
        return (None, None);
    }
    let mut open = None;
    let mut jump = None;
    let mut read = None;
    for item in items {
        let Some(c) = app.store.channels.get(&item.ch) else { continue };
        let ch_name = channel_title(&app.store, item.ch);
        let g_name = if c.guild != 0 {
            app.store.guilds.get(&c.guild).map(|g| g.name.clone()).unwrap_or_default()
        } else {
            "Direct Message".into()
        };
        let is_dm = c.guild == 0;
        let card = Frame::new()
            .fill(pal().bg_primary)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, pal().border_subtle))
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    icons::draw(ui.painter(), if is_dm { Icon::Bubble } else { Icon::Hash }, ir, pal().channel_default);
                    text::label(ui, &ch_name, semi(14.0), pal().header_primary, Sense::hover());
                    ui.label(RichText::new(format!("· {g_name}")).font(reg(12.0)).color(pal().text_muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (mr, mresp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                        icons::draw(ui.painter(), Icon::Check, mr.shrink(3.0), if mresp.hovered() { pal().header_primary } else { pal().header_secondary });
                        if mresp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if mresp.clicked() {
                            read = Some(item.ch);
                        }
                        if item.mentions > 0 {
                            let (br, _) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::hover());
                            badge(ui, br.center(), item.mentions, None);
                        }
                    });
                });
                ui.add_space(4.0);
                let line = if item.author.is_empty() {
                    item.preview.clone()
                } else {
                    format!("{}: {}", item.author, item.preview)
                };
                let (lr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
                text::paint_em(ui.painter(), pos2(lr.left(), lr.center().y), Align2::LEFT_CENTER, &line, reg(13.0), pal().text_normal, lr.width(), 1.0);
            });
        let resp = ui.interact(card.response.rect, ui.id().with(("icymi", item.ch)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_stroke(card.response.rect, 8, Stroke::new(1.0, pal().blurple.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && read != Some(item.ch) {
            if let Some(m) = item.msg {
                jump = Some((item.ch, m));
            } else {
                open = Some(item.ch);
            }
        }
        ui.add_space(6.0);
    }
    if let Some(c) = read {
        app.mark_read(&[c]);
    }
    (open, jump)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ReadState;

    fn text_chan(id: Id, guild: Id, last_msg: Id) -> Channel {
        Channel { id, guild, last_msg, name: format!("c{id}"), kind: if guild == 0 { 1 } else { 0 }, ..Default::default() }
    }

    #[test]
    fn ranks_mentions_dms_and_real_snowflake_recency_skipping_muted_and_requests() {
        let mut s = Store::default();
        let now = 1_795_000_000_000u64;
        let snow = |mins_ago: u64| -> Id { (now - mins_ago * 60_000 - DISCORD_EPOCH_MS) << 22 };
        s.channels.insert(1, text_chan(1, 10, snow(200)));
        s.channels.insert(5, text_chan(5, 10, snow(5)));
        s.channels.insert(2, text_chan(2, 10, snow(120)));
        s.channels.insert(3, text_chan(3, 0, snow(60)));
        s.channels.insert(4, text_chan(4, 10, snow(1)));
        let mut req = text_chan(6, 0, snow(1));
        req.extra = Some(Box::new(crate::state::ChannelExtra { msg_request: true, ..Default::default() }));
        s.channels.insert(6, req);
        for id in [1, 2, 3, 4, 5, 6] {
            s.reads.insert(id, ReadState { last_acked: 100, mentions: if id == 2 { 2 } else if id == 4 { 3 } else { 0 }, ..Default::default() });
        }
        s.muted.insert(4);
        let ranked = rank_at(&s, None, now);
        let ids: Vec<Id> = ranked.iter().map(|h| h.ch).collect();
        assert_eq!(ids, vec![2, 3, 5, 1]);
    }
}

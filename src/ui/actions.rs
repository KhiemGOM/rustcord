//! What a message's toolbar, menu and keys do. Where discord asks first, `ask` is false while
//! shift is held, which skips the dialog the way discord's protips say.

use serde_json::json;

use super::{App, Confirm};
use crate::net::{Cmd, Tag};
use crate::state::Id;

/// What a send was made of, kept until it lands so a failed one can go out again as it was.
#[derive(Clone)]
pub struct Unsent {
    pub content: String,
    pub reply: Option<(Id, bool)>,
    pub files: Vec<(std::path::PathBuf, String, String)>,
    pub tts: bool,
}

impl App {
    pub fn remember_send(&mut self, nonce: &str, u: Unsent) {
        if self.unsent.len() > 20 {
            let live: Vec<String> = self.store.msgs.values().flat_map(|e| e.list.iter()).filter(|m| m.pending || m.failed).filter_map(|m| m.nonce.clone()).collect();
            self.unsent.retain(|n, _| live.contains(n));
        }
        self.unsent.insert(nonce.to_string(), u);
    }

    /// Resend message: the failed one goes out again under its own nonce, so discord can't post it twice.
    pub fn resend(&mut self, ch: Id, id: Id) {
        let Some(m) = self.store.find_msg(ch, id).filter(|m| m.failed) else { return };
        let Some(nonce) = m.nonce.clone() else { return };
        let u = self.unsent.get(&nonce).cloned().unwrap_or(Unsent { content: m.content.clone(), reply: m.reply.map(|r| (r, false)), files: Vec::new(), tts: false });
        if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
            m.failed = false;
            m.pending = true;
        }
        self.net.send(Cmd::Send { channel: ch, content: u.content, nonce, reply: u.reply, files: u.files, stickers: Vec::new(), tts: u.tts });
    }

    pub fn edit_msg(&mut self, ch: Id, id: Id, text: String) {
        let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) else { return };
        let (was, edited) = (std::mem::replace(&mut m.content, text.clone()), std::mem::replace(&mut m.edited, true));
        self.rest("PATCH", format!("/channels/{ch}/messages/{id}"), Some(json!({ "content": text })), Tag::undo("Failed to update message", move |app: &mut App| {
            if let Some(m) = app.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                m.content = was.clone();
                m.edited = edited;
            }
        }));
    }

    pub fn delete_msg(&mut self, ch: Id, id: Id, ask: bool) {
        if let Some(m) = self.store.find_msg(ch, id).filter(|m| m.failed) {
            if let Some(n) = &m.nonce {
                self.unsent.remove(n);
            }
            if let Some(e) = self.store.msgs.get_mut(&ch) {
                e.list.retain(|m| m.id != id);
            }
            return;
        }
        if !ask {
            let gone = self.store.find_msg(ch, id).cloned();
            self.rest("DELETE", format!("/channels/{ch}/messages/{id}"), None, Tag::undo("Failed to delete message", move |app: &mut App| {
                if let (Some(m), Some(e)) = (&gone, app.store.msgs.get_mut(&ch)) {
                    let at = e.list.partition_point(|x| x.id < m.id);
                    e.list.insert(at, m.clone());
                }
            }));
            if let Some(m) = self.store.msgs.get_mut(&ch) {
                m.list.retain(|m| m.id != id);
            }
            return;
        }
        let c = Confirm::new("Delete Message", "Are you sure you want to delete this message?", "Delete", move |app: &mut App| app.delete_msg(ch, id, false))
            .about(ch, id)
            .tip(["You can hold down shift when clicking ", "delete message", " to bypass this confirmation entirely."]);
        self.confirm = Some(c);
    }

    /// Pin Message, or Unpin Message on one that already is.
    pub fn pin_msg(&mut self, ch: Id, id: Id, ask: bool) {
        // one only seen in the pins popout may not be loaded in the channel
        let listed = self.pins.as_ref().is_some_and(|(c, l)| *c == ch && l.as_ref().is_some_and(|l| l.iter().any(|m| m.id == id)));
        let pinned = listed || self.store.find_msg(ch, id).is_some_and(|m| m.pinned);
        if !ask {
            let method = if pinned { "DELETE" } else { "PUT" };
            let say = if pinned { "Discord failed to unpin that message for some reason. Sorry about that." } else { "Failed to pin message" };
            self.rest(method, format!("/channels/{ch}/messages/pins/{id}"), None, Tag::undo(say, move |app: &mut App| {
                if let Some(m) = app.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                    m.pinned = pinned;
                }
            }));
            if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                m.pinned = !pinned;
            }
            if pinned && let Some((c, Some(list))) = &mut self.pins
                && *c == ch
            {
                list.retain(|m| m.id != id);
            }
            return;
        }
        let c = if pinned {
            Confirm::new("Unpin Message", "You sure you want to remove this pinned message?", "Remove it please!", move |app: &mut App| app.pin_msg(ch, id, false))
                .about(ch, id)
                .tip(["You can hold down shift when clicking ", "unpin message", " to bypass this confirmation entirely."])
        } else {
            let name = self.store.channels.get(&ch).filter(|c| c.guild != 0).map(|c| c.name.clone());
            let body = match name {
                Some(n) => format!("Hey, just double checking that you want to pin this message to #{n} for posterity and greatness?"),
                None => "Hey, just double checking that you want to pin this message to the current channel for posterity and greatness?".into(),
            };
            Confirm::new("Pin It. Pin It Good.", body, "Oh yeah. Pin it", move |app: &mut App| app.pin_msg(ch, id, false)).blurple().about(ch, id)
        };
        self.confirm = Some(c);
    }

    /// The ✕ over your own message's embeds: they go for everyone (SUPPRESS_EMBEDS).
    pub fn clear_embeds(&mut self, ch: Id, id: Id, ask: bool) {
        if !ask {
            let (flags, embeds) = self.store.find_msg(ch, id).map(|m| (m.flags, m.embeds.clone())).unwrap_or_default();
            self.rest("PATCH", format!("/channels/{ch}/messages/{id}"), Some(json!({ "flags": 4 })), Tag::undo("Failed to remove embeds", move |app: &mut App| {
                if let Some(m) = app.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                    m.flags = flags;
                    m.embeds = embeds.clone();
                }
            }));
            if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                m.flags |= 4;
                m.embeds.clear();
            }
            return;
        }
        let body = "This will remove all embeds on this message for everyone. Hold shift when clearing embeds to skip this modal.";
        self.confirm = Some(Confirm::new("Are you sure?", body, "Remove All Embeds", move |app: &mut App| app.clear_embeds(ch, id, false)));
    }

    /// Submitting a poll vote: set our picks locally and send discord the whole set (empty clears).
    pub fn vote(&mut self, ch: Id, id: Id, picks: Vec<u8>) {
        let before: Vec<u8> = self.store.find_msg(ch, id).and_then(|m| m.poll.as_ref()).map(|p| p.answers.iter().filter(|a| a.me).map(|a| a.id).collect()).unwrap_or_default();
        if self.store.set_votes(ch, id, &picks) {
            let ids: Vec<String> = picks.iter().map(|a| a.to_string()).collect();
            self.rest("PUT", format!("/channels/{ch}/polls/{id}/answers/@me"), Some(json!({ "answer_ids": ids })), Tag::undo("Failed to vote", move |app: &mut App| {
                app.store.set_votes(ch, id, &before);
            }));
        }
    }

    /// Play an ogg/opus clip inline (voice message, soundboard sound), or pause it if it's the
    /// one already going. The bytes are fetched off the ui thread.
    pub fn play_audio(&mut self, url: String) {
        if crate::audioplay::toggle(&url) {
            crate::media::stop();
            let http = crate::api::http();
            self.net.rt.spawn(async move {
                if let Ok(r) = http.get(&url).send().await {
                    if let Ok(b) = r.bytes().await {
                        crate::audioplay::play_bytes(url, b.to_vec());
                    }
                }
            });
        }
    }

    pub fn clear_reactions(&mut self, ch: Id, id: Id, ask: bool) {
        if !ask {
            let kept = self.store.find_msg(ch, id).map(|m| m.reactions.clone()).unwrap_or_default();
            self.rest("DELETE", format!("/channels/{ch}/messages/{id}/reactions"), None, Tag::undo("Failed to remove reactions", move |app: &mut App| {
                if let Some(m) = app.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                    m.reactions = kept.clone();
                }
            }));
            if let Some(m) = self.store.msgs.get_mut(&ch).and_then(|e| e.list.iter_mut().find(|m| m.id == id)) {
                m.reactions.clear();
            }
            return;
        }
        let body = "Are you sure you want to remove all reactions from this message?";
        self.confirm = Some(Confirm::new("Remove All Reactions", body, "Remove All Reactions", move |app: &mut App| app.clear_reactions(ch, id, false)));
    }

    /// Bookmark Message, or Remove Bookmark on one that's saved already.
    pub fn bookmark(&mut self, ch: Id, id: Id) {
        if self.bookmarks.remove(&id) {
            self.rest("DELETE", format!("/users/@me/saved-messages/{ch}/{id}"), None, Tag::undo("Failed to remove bookmark!", move |app: &mut App| {
                app.bookmarks.insert(id);
            }));
            self.toast("Bookmark removed.");
        } else {
            self.bookmarks.insert(id);
            self.rest("PUT", format!("/users/@me/saved-messages/{ch}/{id}"), Some(json!({ "source": "message_context_menu" })), Tag::undo("Failed to save message!", move |app: &mut App| {
                app.bookmarks.remove(&id);
            }));
            self.toast("Bookmark added.");
        }
        if let Some(i) = &mut self.inbox {
            i.saved = None;
        }
    }

    pub fn remind(&mut self, ch: Id, id: Id, at: chrono::DateTime<chrono::Utc>) {
        let due = at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.rest("PUT", format!("/users/@me/saved-messages/{ch}/{id}"), Some(json!({ "due_at": due, "source": "message_context_menu" })), Tag::fail("Failed to save message!"));
        self.toast("Reminder added.");
        if let Some(i) = &mut self.inbox {
            i.saved = None;
        }
    }

    /// Speak Message: "{name} said {text}", read out by windows.
    pub fn speak(&mut self, ch: Id, id: Id) {
        let Some(m) = self.store.find_msg(ch, id) else { return };
        let guild = self.store.channels.get(&ch).map_or(0, |c| c.guild);
        let text = format!("{} said {}", self.store.name_in(guild, m.author), crate::markdown::plain(&m.content, &self.store, guild));
        match &mut self.e2e {
            Some(e) => e.said.push(text),
            None => crate::speech::say(text),
        }
    }

    /// Create Reminder's choices as discord words them: half an hour, an hour, four, nine in
    /// the morning (today's if it's not past yet) and the next Monday at nine.
    pub fn reminder_times() -> Vec<(String, chrono::DateTime<chrono::Utc>)> {
        use chrono::{Datelike, Duration, Local, TimeZone, Timelike};
        let now = Local::now();
        let nine = |d: chrono::NaiveDate| Local.from_local_datetime(&d.and_hms_opt(9, 0, 0).unwrap_or_default()).single().unwrap_or(now);
        let today = now.date_naive();
        let morning = if now.hour() >= 9 { ("Tomorrow at 9:00am", nine(today + Duration::days(1))) } else { ("Today at 9:00am", nine(today)) };
        let wd = now.weekday().num_days_from_sunday() as i64;
        let monday = if wd == 0 || (wd == 1 && now.hour() < 9) { today + Duration::days(1 - wd) } else { today + Duration::days(8 - wd) };
        let utc = |t: chrono::DateTime<Local>| t.with_timezone(&chrono::Utc);
        vec![
            ("in 30m".into(), utc(now + Duration::minutes(30))),
            ("in 1h".into(), utc(now + Duration::hours(1))),
            ("in 4h".into(), utc(now + Duration::hours(4))),
            (morning.0.into(), utc(morning.1)),
            ("Monday at 9:00am".into(), utc(nine(monday))),
        ]
    }

    pub fn message_link(&self, ch: Id, id: Id) -> String {
        let g = self.store.channels.get(&ch).map(|c| c.guild).filter(|g| *g != 0);
        let g = g.map(|g| g.to_string()).unwrap_or_else(|| "@me".into());
        format!("https://discord.com/channels/{g}/{ch}/{id}")
    }

    /// Someone's @name into the open channel's composer, as shift+click on a name does.
    pub fn mention(&mut self, uid: Id, guild: Id) {
        let Some(ch) = self.cur() else { return };
        let shown = format!("@{}", self.store.name_in(guild, uid));
        let d = self.drafts.entry(ch).or_default();
        if !d.is_empty() && !d.ends_with(' ') {
            d.push(' ');
        }
        d.push_str(&shown);
        d.push(' ');
        self.aliases.entry(ch).or_default().push((shown, format!("<@{uid}>")));
        self.focus_composer = true;
        self.caret_end = true;
    }
}

impl App {
    /// Configure on a join message: the server's engagement settings, where its welcome messages live.
    pub fn configure_joins(&mut self, guild: Id) {
        self.open_server_settings(guild);
        if let Some(s) = &mut self.server {
            s.page = super::server::Page::Engagement;
        }
    }
}

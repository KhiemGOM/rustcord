//! What a call does besides draw: the join, leave and stream sounds, the ring loops, and the
//! toast and taskbar flash for a call ringing while the window is away.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{App, channel_title};
use crate::state::{Id, Status, sid};

const RING_EVERY: f32 = 3.0;
const CALLING_EVERY: f32 = 3.0;
const MAX_NOISY: usize = 25;

static LOOPS: Mutex<Vec<(&'static str, Arc<AtomicBool>)>> = Mutex::new(Vec::new());
static TOASTED: AtomicU64 = AtomicU64::new(0);

/// Starts or stops a sound repeating every `secs`, until stopped.
fn looping(name: &'static str, secs: f32, on: bool) {
    let Ok(mut l) = LOOPS.lock() else { return };
    let at = l.iter().position(|(n, _)| *n == name);
    match (on, at) {
        (true, None) => {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            std::thread::spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    crate::sound::cue(name);
                    for _ in 0..(secs * 20.0) as u32 {
                        if flag.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            });
            l.push((name, stop));
        }
        (false, Some(i)) => l.remove(i).1.store(true, Ordering::Relaxed),
        _ => {}
    }
}

/// How long a call lasted, the way discord's call messages say it.
pub fn lasted(ms: i64) -> String {
    let mins = ms / 60_000;
    match mins {
        _ if ms < 60_000 => "a few seconds".into(),
        1 => "a minute".into(),
        2..=59 => format!("{mins} minutes"),
        60..=89 => "an hour".into(),
        _ => format!("{} hours", (mins + 30) / 60),
    }
}

impl App {
    /// Sounds for what a gateway event changes, read before the stores take it in.
    pub(super) fn voice_cues(&mut self, t: &str, d: &serde_json::Value) {
        let me = self.store.me;
        match t {
            "VOICE_STATE_UPDATE" => {
                let uid = sid(&d["user_id"]);
                let now = sid(&d["channel_id"]);
                let gid = sid(&d["guild_id"]);
                let was = if gid != 0 {
                    self.store.guilds.get(&gid).and_then(|g| g.voice.get(&uid).copied()).unwrap_or(0)
                } else {
                    self.voice.calls.iter().find(|(_, u)| u.contains(&uid)).map_or(0, |(c, _)| *c)
                };
                let Some(mine) = self.voice.chan else { return };
                if uid == me && now != 0 && now != mine {
                    crate::sound::cue("user_moved");
                    return;
                }
                if self.voice.stage_room || (now != mine && was != mine) {
                    return;
                }
                let live = !d["channel_id"].is_null() && d["self_stream"].as_bool() == Some(true);
                let was_live = self.voice.live.contains(&uid);
                if live && !was_live && now == mine {
                    crate::sound::cue("stream_started");
                } else if was_live && !(live && now == mine) && (uid == me || self.voice.stream.as_ref().is_some_and(|l| l.user == uid)) {
                    crate::sound::cue("stream_ended");
                }
                if uid == me || self.voice.members(&self.store, mine).len() > MAX_NOISY {
                    return;
                }
                if now == mine && was != mine {
                    crate::sound::cue("user_join");
                } else if was == mine && now != mine {
                    crate::sound::cue("user_leave");
                }
            }
            "STREAM_CREATE" | "STREAM_UPDATE" => {
                let Some(key) = self.voice.stream.as_ref().map(|l| l.key.clone()) else { return };
                if d["stream_key"].as_str() != Some(&key) {
                    return;
                }
                let viewers: std::collections::HashSet<Id> = d["viewer_ids"].as_array().into_iter().flatten().map(sid).filter(|u| *u != me).collect();
                let before = self.voice.viewers.len();
                if viewers.len() <= MAX_NOISY && viewers.len() != before && d["viewer_ids"].is_array() {
                    crate::sound::cue(if viewers.len() > before { "stream_user_joined" } else { "stream_user_left" });
                }
                if d["viewer_ids"].is_array() {
                    self.voice.viewers = viewers;
                }
            }
            "STREAM_DELETE" => self.voice.viewers.clear(),
            _ => {}
        }
    }

    /// The ring loops follow what's on screen, and a call that starts ringing while the window is
    /// away gets a toast with Answer and Decline, and the taskbar flash. Runs from `logic`.
    pub(super) fn call_fx(&mut self, ctx: &egui::Context) {
        let dnd = self.store.presence.get(&self.store.me) == Some(&Status::Dnd);
        let ringing = self.voice.ringing;
        looping("call_ringing", RING_EVERY, ringing.is_some() && !dnd);
        let calling = self.voice.chan.is_some_and(|c| {
            self.store.channels.get(&c).is_some_and(|ch| ch.guild == 0)
                && self.voice.rung.get(&c).is_some_and(|r| !r.is_empty())
                && self.voice.members(&self.store, c).len() < 2
        });
        looping("call_calling", CALLING_EVERY, calling && !dnd);
        let ch = ringing.unwrap_or(0);
        if TOASTED.swap(ch, Ordering::Relaxed) != ch && ch != 0 {
            self.ring_notice(ctx, ch, dnd);
        }
    }

    fn ring_notice(&mut self, ctx: &egui::Context, ch: Id, dnd: bool) {
        if dnd || !self.settings.notify || (self.settings.streamer && self.settings.streamer_no_notifications) {
            return;
        }
        let focused = ctx.input(|i| i.raw.focused) && crate::winsys::shown();
        if focused && self.e2e.is_none() {
            return;
        }
        let Some(c) = self.store.channels.get(&ch) else { return };
        let title = channel_title(&self.store, ch);
        let caller = c.recipients.first().and_then(|u| self.store.user(*u));
        let (body, avatar) = match (c.kind, caller) {
            (1, Some(u)) => (format!("{title} is calling you!"), u.avatar_url()),
            _ => (format!("Incoming call from {title}"), String::new()),
        };
        let ping = crate::toast::Ping { title: "Incoming Call".into(), body, avatar, at: (ch, 0), call: true };
        if let Some(e) = &mut self.e2e {
            e.mark_once("call_toast");
            e.call_toasts.push(crate::toast::xml(&ping, None));
        } else {
            crate::toast::send(ping);
            if self.settings.flash {
                crate::winsys::flash();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lasted;

    #[test]
    fn durations() {
        assert_eq!(lasted(5_000), "a few seconds");
        assert_eq!(lasted(65_000), "a minute");
        assert_eq!(lasted(125_000), "2 minutes");
        assert_eq!(lasted(3_900_000), "an hour");
        assert_eq!(lasted(7_300_000), "2 hours");
    }
}

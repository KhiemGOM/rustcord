//! VcNarrator: says who joined, left or moved, in the voice channel you're in.

use crate::plugins::{self, flag, num, text};
use crate::state::{Id, Store};

const P: &str = "VcNarrator";

fn clean(s: &str) -> String {
    let latin = flag(P, "latinOnly");
    s.chars()
        .filter(|c| c.is_numeric() || c.is_ascii_punctuation() || c.is_whitespace() || if latin { c.is_alphabetic() && (*c as u32) < 0x250 } else { c.is_alphabetic() })
        .collect::<String>()
        .replace("__", "_")
        .trim()
        .to_string()
}

/// A voice state moved `who` from `was` to `now`; `mine` is the channel you're in.
pub fn voice(store: &Store, me: Id, mine: Option<Id>, guild: Id, who: Id, was: Id, now: Id) {
    if !plugins::on(P) || was == now {
        return;
    }
    let is_me = who == me;
    // a dm call's states don't say where people were, so only guild channels are told apart
    if !is_me && (guild == 0 || mine.is_none_or(|c| c != now && c != was)) {
        return;
    }
    let (kind, ch) = match (was, now) {
        (_, 0) => ("leave", was),
        (0, _) => ("join", now),
        _ => ("move", now),
    };
    if store.channels.get(&ch).is_some_and(|c| c.kind == 13) {
        return;
    }
    let u = store.user(who);
    let user = if is_me && !flag(P, "sayOwnName") { String::new() } else { u.map(|u| u.username.clone()).unwrap_or_default() };
    let display = if user.is_empty() { String::new() } else { u.and_then(|u| u.global_name.clone()).unwrap_or_else(|| user.clone()) };
    let nick = if user.is_empty() { String::new() } else { store.name_in(guild, who) };
    let or = |s: String, empty: bool| if s.is_empty() && !empty { "Someone".to_string() } else { s };
    let channel = store.channels.get(&ch).map(|c| clean(&c.name)).filter(|s| !s.is_empty()).unwrap_or_else(|| "channel".into());
    let line = text(P, &format!("{kind}Message"))
        .replace("{{USER}}", &or(clean(&user), user.is_empty()))
        .replace("{{CHANNEL}}", &channel)
        .replace("{{DISPLAY_NAME}}", &or(clean(&display), display.is_empty()))
        .replace("{{NICKNAME}}", &or(clean(&nick), nick.is_empty()));
    // speechSynthesis' rate is a multiplier; SAPI's -10..10 runs from a third to triple speed
    let rate = (10.0 * num(P, "rate").max(0.1).ln() / 3f64.ln()).round().clamp(-10.0, 10.0) as i32;
    crate::speech::say_with(line, (num(P, "volume") * 100.0).clamp(0.0, 100.0) as u16, rate);
}

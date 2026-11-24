use super::Hit;
use crate::state::{Id, Store};

/// How well `name` answers `q` (both lowercase), as discord's autocomplete scores it: 10 the name
/// itself, 7 its start, 5 inside it, 3 every word inside it, 1 its letters in order.
fn score(name: &str, q: &str, fuzzy: bool) -> f32 {
    // a query with a space also matches a dash there, as channel names are written
    let dashed = q.replacen(' ', "-", 1);
    if name.starts_with(q) || name.starts_with(&dashed) {
        return if name == q { 10.0 } else { 7.0 };
    }
    if name.contains(q) || name.contains(&dashed) {
        return 5.0;
    }
    if q.split([',', ' ']).filter(|w| !w.is_empty()).all(|w| name.contains(w)) {
        return 3.0;
    }
    if fuzzy && in_order(q, name) {
        return 1.0;
    }
    0.0
}

/// The fuzzysearch package: every character of `q` turns up in `name`, in order.
fn in_order(q: &str, name: &str) -> bool {
    let mut rest = name.chars();
    q.chars().all(|c| rest.any(|n| n == c))
}

struct Word {
    text: String,
    whole: bool,
}

fn words(q: &str) -> Vec<Word> {
    let mut v: Vec<Word> = q.split(' ').filter(|w| !w.is_empty()).map(|w| Word { text: w.to_string(), whole: false }).collect();
    if q.contains(' ') {
        v.insert(0, Word { text: q.to_string(), whole: true });
    }
    v
}

/// The best of the words left against `name`; the word it was is used up, or all of them when the
/// whole query matched.
fn best(name: &str, left: &mut Vec<Word>, fuzzy: bool) -> f32 {
    let Some((i, s)) = left.iter().enumerate().map(|(i, w)| (i, score(name, &w.text, fuzzy))).fold(None, |b: Option<(usize, f32)>, (i, s)| match b {
        Some((_, bs)) if bs >= s => b,
        _ if s > 0.0 => Some((i, s)),
        _ => b,
    }) else {
        return 0.0;
    };
    if left[i].whole {
        left.clear();
    } else {
        left.remove(i);
    }
    s
}

/// A channel's score: its name takes one word, and words left over may be its server's or
/// category's name for half; every word has to land somewhere. Voice channels rank a point lower
/// and ones visited lately up to three higher.
fn channel_score(name: &str, guild: &str, parent: Option<&str>, q: &str, voice: bool, recent: bool) -> f32 {
    let mut left = words(q);
    let mut s = best(name, &mut left, true);
    if s == 0.0 {
        return 0.0;
    }
    if !left.is_empty() {
        for other in [Some(guild), parent].into_iter().flatten().filter(|n| !n.is_empty()) {
            s += 0.5 * best(other, &mut left, false);
        }
        s = s.min(6.0);
    }
    if left.len() > 1 || left.first().is_some_and(|w| !w.whole) {
        return 0.0;
    }
    if voice {
        s = (s - 1.0).max(0.5);
    }
    if recent {
        s = (s + 3.0).min(if s >= 7.0 { 10.0 } else { 7.0 });
    }
    s
}

/// Each kind keeps its best five (all of them, up to a hundred, when it's the only kind with
/// anything or a prefix asked for it), and then they're ranked together by score.
fn rank(mut kinds: Vec<Vec<(f32, String, Hit)>>, narrowed: bool) -> Vec<Hit> {
    let single = kinds.iter().filter(|k| !k.is_empty()).count() == 1;
    let keep = if narrowed || single { 100 } else { 5 };
    let mut all = Vec::new();
    for k in &mut kinds {
        k.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        k.truncate(keep);
        all.append(k);
    }
    all.sort_by(|a, b| b.0.total_cmp(&a.0));
    all.into_iter().map(|(_, _, h)| h).collect()
}

/// What `q` (lowercase, its prefix taken off into `only`) finds: people, group dms, channels and
/// servers, the server you're in left out.
pub(super) fn search(store: &Store, q: &str, only: Option<char>, here: Option<Id>, recent: &[Id]) -> Vec<Hit> {
    let (mut people, mut groups, mut chans, mut servers) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    if matches!(only, None | Some('@')) {
        let mut seen = crate::state::Set::default();
        let person = |u: Id| store.user(u).map(|u| score(&u.display().to_lowercase(), q, true).max(score(&u.username.to_lowercase(), q, true)));
        for &c in &store.dms {
            let Some(ch) = store.channels.get(&c) else { continue };
            if ch.kind == 1 && let Some(&u) = ch.recipients.first() {
                seen.insert(u);
                if let Some(s) = person(u).filter(|s| *s > 0.0) {
                    people.push((s, crate::ui::channel_title(store, c).to_lowercase(), Hit::Channel(c)));
                }
            } else if ch.kind == 3 && only.is_none() {
                let title = crate::ui::channel_title(store, c).to_lowercase();
                let s = score(&title, q, true);
                if s > 0.0 {
                    groups.push((s, title, Hit::Channel(c)));
                }
            }
        }
        for (&u, _) in store.relationships.iter().filter(|(u, k)| **k == 1 && !seen.contains(*u)) {
            if let Some(s) = person(u).filter(|s| *s > 0.0) {
                people.push((s, store.user(u).map(|u| u.display().to_lowercase()).unwrap_or_default(), Hit::User(u)));
            }
        }
    }
    for gid in &store.guild_order {
        let Some(g) = store.guilds.get(gid) else { continue };
        let gname = g.name.to_lowercase();
        if matches!(only, None | Some('*')) && Some(*gid) != here {
            let s = score(&gname, q, true);
            if s > 0.0 {
                servers.push((s, gname.clone(), Hit::Guild(*gid)));
            }
        }
        if matches!(only, Some('@' | '*')) {
            continue;
        }
        for c in g.channels.iter().filter_map(|c| store.channels.get(c)) {
            let text = c.is_text() || c.is_forum();
            let wanted = match only {
                Some('#') => text,
                Some('!') => c.is_voice(),
                _ => text || c.is_voice(),
            };
            if !wanted || c.is_thread() || !store.can_view(c) {
                continue;
            }
            let parent = store.channels.get(&c.parent).filter(|p| p.kind == 4).map(|p| p.name.to_lowercase());
            let name = c.name.to_lowercase();
            let s = channel_score(&name, &gname, parent.as_deref(), q, c.is_voice(), recent.contains(&c.id));
            if s > 0.0 {
                chans.push((s, name, Hit::Channel(c.id)));
            }
        }
    }
    rank(vec![people, groups, chans, servers], only.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_scores_as_discord_scores_it() {
        assert_eq!(score("general", "general", true), 10.0);
        assert_eq!(score("record-of-ragnarok", "rec", true), 7.0);
        assert_eq!(score("off-topic", "off topic", true), 7.0);
        assert_eq!(score("the-general", "gen", true), 5.0);
        assert_eq!(score("topic off", "off topic", true), 3.0);
        assert_eq!(score("recruitment", "rcr", true), 1.0);
        assert_eq!(score("recruitment", "rcr", false), 0.0);
        assert_eq!(score("general", "xyz", true), 0.0);
    }

    #[test]
    fn words_left_over_can_be_the_server_or_category_name() {
        assert_eq!(channel_score("rules", "rec room", None, "rules", false, false), 10.0);
        assert_eq!(channel_score("rules", "rec room", None, "rec rules", false, false), 6.0);
        assert_eq!(channel_score("rules", "other", None, "rec rules", false, false), 0.0);
        assert_eq!(channel_score("clips", "game", Some("media"), "media clips", false, false), 6.0);
        assert_eq!(channel_score("lounge", "x", None, "lounge", true, false), 9.0);
        assert_eq!(channel_score("the-lounge", "x", None, "lou", false, true), 7.0);
        assert_eq!(channel_score("lounge", "x", None, "lounge", false, true), 10.0);
    }

    #[test]
    fn each_kind_keeps_five_unless_it_is_alone() {
        let hit = |s: f32, n: &str, id: Id| (s, n.to_string(), Hit::Channel(id));
        let chans: Vec<_> = (0..8).map(|i| hit(7.0, &format!("c{i}"), i)).collect();
        assert_eq!(rank(vec![chans.clone()], false).len(), 8);
        let servers = vec![(10.0, "rec".to_string(), Hit::Guild(99))];
        let both = rank(vec![chans, servers], false);
        assert_eq!(both.len(), 6);
        assert!(matches!(both[0], Hit::Guild(99)));
    }
}

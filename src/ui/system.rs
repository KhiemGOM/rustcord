//! What discord prints for the messages that aren't someone talking: joins, calls, pins, boosts,
//! stage and automod notices and the rest of its MessageTypes. Wording is discord's own.

use egui::Color32;

use super::icons::Icon;
use crate::state::{Id, Message, Store};
use crate::theme::*;

#[derive(Clone)]
pub enum Go {
    Profile(Id),
    Message(Id, Id),
    Pins,
    Threads,
    Channel(Id),
    JoinCall(Id),
    Guild(Id),
    GuildSettings(Id),
}

pub enum Span {
    Text(String),
    Name(String, Id),
    Bold(String),
    Link(String, Go),
}

pub struct Sys {
    pub icon: Icon,
    pub tint: Color32,
    pub spans: Vec<Span>,
    pub button: Option<(String, Go)>,
    pub card: Card,
}

impl Sys {
    pub fn plain(&self) -> String {
        self.spans
            .iter()
            .map(|s| match s {
                Span::Text(t) | Span::Bold(t) | Span::Name(t, _) | Span::Link(t, _) => t.as_str(),
            })
            .collect()
    }
}

pub enum Card {
    None,
    Poll { winner: (String, u32), poll: Option<(Id, Id)> },
    AutoMod { content: String, keyword: Option<String>, rule: Option<String> },
}

const JOINS: [(&str, &str); 13] = [
    ("", " joined the party."),
    ("", " is here."),
    ("Welcome, ", ". We hope you've brought pizza."),
    ("A wild ", " appeared."),
    ("", " just landed."),
    ("", " just slid into the server."),
    ("", " just showed up!"),
    ("Welcome ", ". Say hi!"),
    ("", " hopped into the server."),
    ("Everyone welcome ", "!"),
    ("Glad you're here, ", "."),
    ("Good to see you, ", "."),
    ("Yay you made it, ", "!"),
];

/// moment's humanize, which is what discord prints for how long a call lasted
pub fn humanize(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    let m = (s as f64 / 60.0).round() as i64;
    let h = (s as f64 / 3600.0).round() as i64;
    let d = (s as f64 / 86400.0).round() as i64;
    let plural = |n: i64, unit: &str| if n == 1 { format!("a {unit}") } else { format!("{n} {unit}s") };
    if s < 45 {
        "a few seconds".into()
    } else if s < 90 {
        "a minute".into()
    } else if s < 45 * 60 {
        format!("{m} minutes")
    } else if s < 90 * 60 {
        "an hour".into()
    } else if s < 22 * 3600 {
        format!("{h} hours")
    } else if s < 36 * 3600 {
        "a day".into()
    } else if d < 26 {
        format!("{d} days")
    } else if d < 45 {
        "a month".into()
    } else if d < 320 {
        plural((d as f64 / 30.4).round() as i64, "month").replace("an month", "a month")
    } else if d < 548 {
        "a year".into()
    } else {
        format!("{} years", (d as f64 / 365.25).round() as i64)
    }
}

fn tier(kind: u8) -> &'static str {
    match kind {
        9 => "Level 1",
        10 => "Level 2",
        _ => "Level 3",
    }
}

fn months(n: u64) -> String {
    if n == 1 { "1 month".into() } else { format!("{n} months") }
}

/// The user's name as an inline hook: bold, opens their profile.
fn who(store: &Store, guild: Id, id: Id) -> Span {
    Span::Name(store.name_in(guild, id), id)
}

fn field<'a>(m: &'a Message, name: &str) -> Option<&'a str> {
    m.embeds.first()?.fields.iter().find(|f| f.0 == name).map(|f| f.1.as_str())
}

/// None for the types discord draws nothing for.
pub fn describe(store: &Store, guild: Id, m: &Message) -> Option<Sys> {
    use Span::{Bold, Text};
    let chan = store.channels.get(&m.channel);
    let thread = chan.is_some_and(|c| matches!(c.kind, 10..=12));
    let group = chan.is_some_and(|c| c.kind == 3);
    let me = store.me;
    let u = who(store, guild, m.author);
    let other = m.mentions.first().copied();
    let sys = |icon: Icon, tint: Color32, spans: Vec<Span>| Some(Sys { icon, tint, spans, button: None, card: Card::None });
    let muted = pal().header_secondary;
    match m.kind {
        1 => {
            let to = other.map(|o| who(store, guild, o)).unwrap_or(Text("someone".into()));
            let tail = if thread { " to the thread." } else { " to the group." };
            sys(Icon::ArrowRight, pal().green, vec![u, Text(" added ".into()), to, Text(tail.into())])
        }
        2 => match other.filter(|o| *o != m.author) {
            Some(o) if thread => sys(Icon::ArrowLeft, pal().red, vec![u, Text(" removed ".into()), who(store, guild, o), Text(" from the thread.".into())]),
            Some(o) => sys(Icon::ArrowLeft, pal().red, vec![u, Text(" removed ".into()), who(store, guild, o), Text(" from the group.".into())]),
            None => sys(Icon::ArrowLeft, pal().red, vec![u, Text(" left the group.".into())]),
        },
        3 => {
            let call = m.call.as_ref();
            let missed = call.is_some_and(|c| c.ended.is_some() && !c.with.contains(&me)) && m.author != me;
            let active = call.is_some_and(|c| c.ended.is_none());
            let dur = call.and_then(|c| c.ended).map(|e| humanize(e - m.ts()));
            let (icon, tint, spans) = if missed {
                let mut s = vec![Text("You missed a call from ".into()), u];
                s.push(Text(dur.map(|d| format!(" that lasted {d}.")).unwrap_or_else(|| ".".into())));
                (Icon::Phone, pal().red, s)
            } else {
                let tail = dur.map(|d| format!(" started a call that lasted {d}.")).unwrap_or_else(|| " started a call.".into());
                (Icon::Phone, pal().green, vec![u, Text(tail)])
            };
            Some(Sys { icon, tint, spans, button: active.then(|| ("Join the call".to_string(), Go::JoinCall(m.channel))), card: Card::None })
        }
        4 => {
            let forum = chan.is_some_and(|c| c.kind == 11) && store.channels.get(&chan?.parent).is_some_and(|p| matches!(p.kind, 15 | 16));
            let spans = if group && m.content.is_empty() {
                vec![u, Text(" removed the custom group name.".into())]
            } else if group {
                vec![u, Text(" changed the group name: ".into()), Bold(m.content.clone()), Text(".".into())]
            } else if forum {
                vec![u, Text(" changed the post title: ".into()), Bold(m.content.clone())]
            } else {
                vec![u, Text(" changed the channel name: ".into()), Bold(m.content.clone())]
            };
            sys(Icon::Pencil, muted, spans)
        }
        5 => sys(Icon::Pencil, muted, vec![u, Text(" changed the group icon.".into())]),
        6 => {
            let mut s = vec![u, Text(" pinned ".into())];
            match m.refer.filter(|r| r.1 != 0) {
                Some((c, id)) => s.push(Span::Link("a message".into(), Go::Message(c, id))),
                None => s.push(Text("a message".into())),
            }
            s.push(Text(" to this channel. See all ".into()));
            s.push(Span::Link("pinned messages".into(), Go::Pins));
            s.push(Text(".".into()));
            sys(Icon::Pin, muted, s)
        }
        7 => {
            if store.user(m.author).is_some_and(|u| u.bot) {
                sys(Icon::ArrowRight, pal().green, vec![u, Text(" was added to the server.".into())])
            } else {
                let (a, b) = JOINS[(m.ts().unsigned_abs() % 13) as usize];
                let mut s = Vec::new();
                if !a.is_empty() {
                    s.push(Text(a.into()));
                }
                s.push(u);
                s.push(Text(b.into()));
                sys(Icon::ArrowRight, pal().green, s)
            }
        }
        8..=11 => {
            let n: u32 = m.content.trim().parse().unwrap_or(1).max(1);
            let guild_name = store.guilds.get(&guild).map(|g| g.name.clone()).unwrap_or_default();
            let mut s = vec![u, Text(" just Boosted the server".into())];
            if n > 1 {
                s.push(Text(" ".into()));
                s.push(Bold(n.to_string()));
                s.push(Text(" times!".into()));
            } else {
                s.push(Text("!".into()));
            }
            if m.kind > 8 {
                s.push(Text(format!(" {guild_name} has achieved ")));
                s.push(Bold(format!("{}!", tier(m.kind))));
            }
            sys(Icon::Gem, Color32::from_rgb(0xff, 0x73, 0xfa), s)
        }
        12 => {
            let hook = m.content.rsplit_once(' ').map_or(m.content.as_str(), |_| m.content.as_str());
            sys(Icon::ArrowRight, pal().green, vec![u, Text(" has added ".into()), Bold(hook.to_string()), Text(" to this channel. Its most important updates will show up here.".into())])
        }
        14 | 16 | 17 => {
            let g = store.guilds.get(&guild).map(|g| g.name.clone()).unwrap_or_default();
            let link = Span::Link(g, Go::Guild(guild));
            let s = match m.kind {
                14 => vec![link, Text(" has been removed from Server Discovery because it no longer passes all the requirements. Check ".into()), Span::Link("Server Settings".into(), Go::GuildSettings(guild)), Text(" for more details.".into())],
                16 => vec![link, Text(" has failed Discovery activity requirements for 1 week. If it fails for 4 weeks in a row, it will be automatically removed from Discovery.".into())],
                _ => vec![link, Text(" has failed Discovery activity requirements for 3 weeks in a row. If it fails for 1 more week, it will be removed from Discovery.".into())],
            };
            sys(if m.kind == 14 { Icon::Flag } else { Icon::Help }, if m.kind == 14 { pal().red } else { pal().yellow }, s)
        }
        15 => sys(Icon::Check, pal().green, vec![Text("This server is eligible for Server Discovery again and has been automatically relisted!".into())]),
        18 => {
            let name = if m.content.is_empty() { "a thread".to_string() } else { m.content.clone() };
            let t = m.refer.map(|(c, _)| c).filter(|c| *c != 0);
            let link = t.map(|c| Span::Link(name.clone(), Go::Channel(c))).unwrap_or(Bold(name));
            sys(Icon::Thread, muted, vec![u, Text(" started a thread: ".into()), link, Text(". See all ".into()), Span::Link("threads".into(), Go::Threads), Text(".".into())])
        }
        21 => {
            sys(Icon::Thread, muted, vec![Text("Sorry, we couldn't load the first message in this thread".into())])
        }
        24 => {
            let verb = if field(m, "flagged_message_id").is_some() { " has flagged a message" } else { " has blocked a message" };
            let channel = field(m, "channel_id").and_then(|c| c.parse().ok());
            let mut s = vec![Bold("AutoMod".into()), Text(verb.into())];
            if let Some(c) = channel {
                s.push(Text(" in ".into()));
                s.push(Span::Link(store.channels.get(&c).map(|c| format!("#{}", c.name)).unwrap_or_else(|| "#unknown".into()), Go::Channel(c)));
            }
            Some(Sys {
                icon: Icon::Shield,
                tint: pal().red,
                spans: s,
                button: None,
                card: Card::AutoMod { content: m.content.clone(), keyword: field(m, "keyword").map(str::to_string), rule: field(m, "rule_name").map(str::to_string) },
            })
        }
        25 => {
            let g = store.guilds.get(&guild).map(|g| g.name.clone()).unwrap_or_default();
            let tier = m.extra["tier_name"].as_str().unwrap_or("").to_string();
            let total = m.extra["total_months_subscribed"].as_u64().unwrap_or(0);
            let renew = m.extra["is_renewal"].as_bool().unwrap_or(false);
            let verb = if renew { " renewed " } else { " joined " };
            let mut s = vec![u, Text(verb.into()), Bold(tier)];
            match (renew, total > 1) {
                (true, false) => s.extend([Text(" in their ".into()), Span::Link(g, Go::Guild(guild)), Text(" membership!".into())]),
                (false, false) => s.extend([Text(" as a subscriber of ".into()), Span::Link(g, Go::Guild(guild)), Text("!".into())]),
                _ => s.extend([Text(" and has been a subscriber of ".into()), Span::Link(g, Go::Guild(guild)), Text(format!(" for {}!", months(total)))]),
            }
            sys(Icon::ArrowRight, pal().green, s)
        }
        27 => sys(Icon::Stage, muted, vec![u, Text(" started ".into()), Bold(m.content.clone())]),
        28 => sys(Icon::Stage, muted, vec![u, Text(" ended ".into()), Bold(m.content.clone())]),
        29 => sys(Icon::Mic, muted, vec![u, Text(" is now a speaker.".into())]),
        30 => sys(Icon::Mic, muted, vec![u, Text(" requested to speak.".into())]),
        31 => sys(Icon::Stage, muted, vec![u, Text(" changed the Stage topic: ".into()), Bold(m.content.clone())]),
        32 => {
            let s = match m.extra["application"].as_str() {
                Some(a) => vec![u, Text(format!(" upgraded {a} to premium for this server! 🎉"))],
                None => vec![u, Text(" upgraded a deleted application to premium for this server! 🎉".into())],
            };
            sys(Icon::Nitro, muted, s)
        }
        33 | 34 => {
            let verb = if m.kind == 33 { " added" } else { " removed" };
            let icon = if m.kind == 33 { (Icon::ArrowRight, pal().green) } else { (Icon::ArrowLeft, pal().red) };
            let s = match m.extra["application"].as_str() {
                Some(a) => vec![u, Text(format!("{verb} the ")), Bold(a.to_string()), Text(" app. See our Help Centre for more info.".into())],
                None => vec![u, Text(format!("{verb} a deleted application. See our Help Centre for more info."))],
            };
            sys(icon.0, icon.1, s)
        }
        36 | 37 => {
            let s = if m.kind == 36 {
                let until = m.content.parse::<chrono::DateTime<chrono::Utc>>().ok().map(|t| t.with_timezone(&chrono::Local).format("%m/%d/%Y, %-I:%M %p").to_string()).unwrap_or_default();
                vec![u, Text(format!(" enabled security actions until {until}."))]
            } else {
                vec![u, Text(" disabled security actions.".into())]
            };
            sys(Icon::Shield, pal().red, s)
        }
        38 => {
            let g = store.guilds.get(&guild).map(|g| g.name.clone()).unwrap_or_default();
            sys(Icon::Shield, pal().red, vec![u, Text(format!(" reported a raid in {g}."))])
        }
        39 => sys(Icon::Shield, pal().green, vec![u, Text(" resolved an Activity Alert.".into())]),
        44 => {
            let product = m.extra["guild_product_purchase"]["product_name"].as_str().unwrap_or("a product").to_string();
            sys(Icon::ArrowRight, pal().green, vec![u, Text(" has purchased ".into()), Bold(product), Text("!".into())])
        }
        46 => {
            let q = field(m, "poll_question_text").unwrap_or("").to_string();
            let votes = |n: &str| field(m, n).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            let total = votes("total_votes").max(1);
            let winner = field(m, "victor_answer_text").map(|t| (t.to_string(), (votes("victor_answer_votes") * 100 + total / 2) / total));
            let mut s = vec![u, Text("’s poll ".into())];
            match m.refer.filter(|r| r.1 != 0) {
                Some((c, id)) => s.push(Span::Link(q.clone(), Go::Message(c, id))),
                None => s.push(Bold(q.clone())),
            }
            s.push(Text(" has closed.".into()));
            let card = winner.map_or(Card::None, |winner| Card::Poll { winner, poll: m.refer.filter(|r| r.1 != 0) });
            Some(Sys { icon: Icon::Rules, tint: muted, spans: s, button: None, card })
        }
        49 => match m.extra["application"].as_str() {
            Some(a) => sys(Icon::Refresh, pal().green, vec![u, Text(" has linked this channel to ".into()), Bold(a.to_string()), Text(". Messages are now syncing.".into())]),
            None => None,
        },
        52 => sys(Icon::ArrowRight, pal().green, vec![Text("This application was approved! Welcome!".into())]),
        53 => sys(Icon::ArrowLeft, pal().red, vec![Text("This application was rejected.".into())]),
        54 => sys(Icon::ArrowLeft, pal().red, vec![Text("This application has been withdrawn.".into())]),
        67 => {
            let s = match (m.author == me, m.content.is_empty()) {
                (true, true) => vec![Text("You accepted a friend request.".into())],
                (false, true) => vec![u, Text(" accepted your friend request.".into())],
                (false, false) => vec![u, Text(" accepted your friend request. You said: ".into()), Bold(m.content.clone())],
                (true, false) => vec![Text("You accepted a friend request. They said: ".into()), Bold(m.content.clone())],
            };
            sys(Icon::ArrowRight, pal().green, s)
        }
        _ => None,
    }
}

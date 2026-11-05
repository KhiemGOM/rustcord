use chrono::{Local, TimeZone};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, Stroke};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::api;
use crate::state::{Id, Store};
use crate::theme::*;

pub enum Link {
    Url(String),
    // [text](url) whose text isn't the url itself: discord warns before following those
    Masked(String),
    User(Id),
    Channel(Id),
    Spoiler,
    // a custom emoji: its id, name and whether it moves
    Emoji(Id, String, bool),
}

pub struct Inline {
    pub job: LayoutJob,
    // (char index of the placeholder, url, size, gap reserved before it)
    pub emojis: Vec<(usize, String, f32, f32)>,
    pub links: Vec<(Range<usize>, Link)>,
    // mentions and timestamps: (chars, fill, line height, pill height), painted under the text
    pub pills: Vec<(Range<usize>, Color32, f32, f32)>,
}

pub enum Block {
    Text(Inline),
    Code(String),
    Quote(Vec<Block>),
}

pub struct Cx<'a> {
    pub store: &'a Store,
    pub ctx: &'a egui::Context,
    pub guild: Id,
    pub size: f32,
    pub color: Color32,
    pub revealed: bool,
    pub edited: bool,
    // embeds show an unknown user's mention as written, <@id>, not "@Unknown User"
    pub embed: bool,
}

impl Cx<'_> {
    // messages are 16 on 22, embed text 14 on 18
    fn line(&self, size: f32) -> f32 {
        (size * if self.embed { 18.0 / 14.0 } else { 1.375 }).round()
    }
}

#[derive(Clone, Copy)]
struct St {
    bold: bool,
    italic: bool,
    under: bool,
    strike: bool,
    code: bool,
    spoiler: bool,
    size: f32,
    color: Color32,
}

struct B<'a, 'b> {
    cx: &'b Cx<'a>,
    job: LayoutJob,
    chars: usize,
    emojis: Vec<(usize, String, f32, f32)>,
    links: Vec<(Range<usize>, Link)>,
    pills: Vec<(Range<usize>, Color32, f32, f32)>,
    // room left before the next text, the inside of a pill
    pad: f32,
    jumbo: bool,
}

impl B<'_, '_> {
    fn fmt(&self, st: St) -> TextFormat {
        let family = match (st.code, st.bold, st.italic) {
            (true, true, _) => mono_bold(st.size * 0.875),
            (true, ..) => mono(st.size * 0.875),
            (_, true, true) => bold_italic(st.size),
            (_, true, _) => bold(st.size),
            (_, _, true) => italic(st.size),
            _ => reg(st.size),
        };
        let (color, bg) = if st.spoiler && !self.cx.revealed {
            (pal().spoiler, pal().spoiler)
        } else if st.spoiler {
            (st.color, pal().spoiler_revealed)
        } else {
            (st.color, Color32::TRANSPARENT)
        };
        TextFormat {
            font_id: family,
            line_height: Some(self.cx.line(st.size)),
            color,
            background: if st.code { pal().bg_code } else { bg },
            italics: st.italic && st.code,
            underline: if st.under { Stroke::new(1.0, color) } else { Stroke::NONE },
            strikethrough: if st.strike { Stroke::new(1.0, color) } else { Stroke::NONE },
            valign: Align::Center,
            expand_bg: 0.0,
            ..Default::default()
        }
    }

    fn text(&mut self, s: &str, st: St) {
        if s.is_empty() {
            return;
        }
        let f = self.fmt(st);
        self.job.append(s, std::mem::take(&mut self.pad), f);
        self.chars += s.chars().count();
    }

    /// Discord's mention pill: 2px of fill either side, radius 3, as tall as the font
    /// rather than the line.
    fn pill(&mut self, s: &str, st: St, fill: Color32, link: Option<Link>) {
        let start = self.chars;
        self.pad = 2.0;
        match link {
            Some(l) => self.linked(s, st, l),
            None => self.text(s, st),
        }
        self.pills.push((start..self.chars, fill, self.cx.line(st.size), (st.size * 18.0 / 14.0).round()));
        self.pad = 2.0;
    }

    fn linked(&mut self, s: &str, st: St, link: Link) {
        let start = self.chars;
        self.text(s, st);
        self.links.push((start..self.chars, link));
    }

    fn emoji(&mut self, url: String, st: St) {
        let size = if self.jumbo { 48.0 } else { self.cx.line(st.size) };
        let font = reg(st.size);
        let w = self.cx.ctx.fonts_mut(|f| f.glyph_width(&font, '\u{a0}'));
        let f = TextFormat {
            font_id: font,
            line_height: Some(size.max(self.cx.line(st.size))),
            color: Color32::TRANSPARENT,
            valign: Align::Center,
            ..Default::default()
        };
        // epaint skips letter spacing on a paragraph's first glyph, leading space it always honours
        let lead = size - w + 1.0;
        self.job.append("\u{a0}", lead + std::mem::take(&mut self.pad), f);
        self.emojis.push((self.chars, url, size, lead));
        self.chars += 1;
    }
}

fn is_emoji(c: char) -> bool {
    let mut b = [0u8; 4];
    emojis::get(c.encode_utf8(&mut b)).is_some()
}

// emoji even without a variation selector: the pictograph blocks, plus dingbats and
// misc symbols the unicode data knows as emoji (so ✿ ★ ♪ in nicknames stay text)
fn strong_emoji(c: char) -> bool {
    match c as u32 {
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x231A | 0x231B | 0x23E9..=0x23FA => is_emoji(c),
        _ => false,
    }
}

// only an emoji when followed by U+FE0F, e.g. © ™ ↔
fn weak_emoji(c: char) -> bool {
    c as u32 >= 0xA9 && is_emoji(c)
}

fn regional(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// Byte length of the emoji cluster at the start of `s`, if there is one.
pub fn take_emoji(s: &str) -> Option<usize> {
    let mut chars = s.chars();
    let c = chars.next()?;
    let next = chars.next();
    if c.is_ascii_digit() || c == '#' || c == '*' {
        for keycap in ["\u{fe0f}\u{20e3}", "\u{20e3}"] {
            if s[1..].starts_with(keycap) {
                return Some(1 + keycap.len());
            }
        }
        return None;
    }
    if !(strong_emoji(c) || (weak_emoji(c) && next == Some('\u{fe0f}'))) {
        return None;
    }
    let mut end = c.len_utf8();
    if regional(c) {
        if next.is_some_and(regional) {
            end += 4;
        }
        return Some(end);
    }
    while let Some(n) = s[end..].chars().next() {
        match n as u32 {
            0xFE0F | 0x20E3 | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F => end += n.len_utf8(),
            0x200D => match s[end + 3..].chars().next() {
                Some(a) if strong_emoji(a) || weak_emoji(a) => end += 3 + a.len_utf8(),
                _ => break,
            },
            _ => break,
        }
    }
    Some(end)
}

/// One-line text for previews: mentions and custom emoji spelled out, markdown markers dropped.
pub fn plain(src: &str, store: &Store, guild: Id) -> String {
    let mut out = String::new();
    let mut rest = src;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find('>').unwrap_or(0);
        let inner = &rest[1..end.max(1)];
        let sub = if end == 0 {
            None
        } else if let Some((_, name, _, _)) = custom_emoji(rest) {
            Some(format!(":{name}:"))
        } else if let Some(id) = inner.strip_prefix("@&").and_then(|x| x.parse::<Id>().ok()) {
            Some(store.guilds.get(&guild).and_then(|g| g.roles.get(&id)).map(|r| format!("@{}", r.name)).unwrap_or_else(|| "@role".into()))
        } else if let Some(id) = inner.strip_prefix('@').map(|x| x.trim_start_matches('!')).and_then(|x| x.parse::<Id>().ok()) {
            Some(format!("@{}", store.name_in(guild, id)))
        } else if let Some(id) = inner.strip_prefix('#').and_then(|x| x.parse::<Id>().ok()) {
            Some(store.channels.get(&id).map(|c| format!("#{}", c.name)).unwrap_or_else(|| "#channel".into()))
        } else {
            None
        };
        match sub {
            Some(s) => {
                out.push_str(&s);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('<');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    let mut s = out.replace('\n', " ");
    for m in ["**", "__", "~~", "||", "```", "`"] {
        s = s.replace(m, "");
    }
    s.trim().to_string()
}

/// Put `text` in front of a block (compact mode's author name), shifting the
/// emoji and link positions that point into it. The prefix can be a link itself.
pub fn prepend(t: &mut Inline, text: &str, fmt: TextFormat, link: Option<Link>) {
    let bytes = text.len();
    let chars = text.chars().count();
    let mut job = LayoutJob { wrap: t.job.wrap.clone(), ..Default::default() };
    job.append(text, 0.0, fmt);
    job.text.push_str(&t.job.text);
    for s in &t.job.sections {
        let mut s = s.clone();
        s.byte_range = s.byte_range.start + bytes..s.byte_range.end + bytes;
        job.sections.push(s);
    }
    t.job = job;
    for e in &mut t.emojis {
        e.0 += chars;
    }
    for (r, _) in &mut t.links {
        *r = r.start + chars..r.end + chars;
    }
    for p in &mut t.pills {
        p.0 = p.0.start + chars..p.0.end + chars;
    }
    if let Some(l) = link {
        t.links.push((0..chars, l));
    }
}

pub fn twemoji(seq: &str) -> String {
    let zwj = seq.contains('\u{200d}');
    let parts: Vec<String> = seq
        .chars()
        .filter(|c| zwj || *c != '\u{fe0f}')
        .map(|c| format!("{:x}", c as u32))
        .collect();
    format!("{}/{}.png", api::emoji_cdn(), parts.join("-"))
}

fn custom_emoji(s: &str) -> Option<(usize, String, Id, bool)> {
    let animated = s.starts_with("<a:");
    let body = s.strip_prefix(if animated { "<a:" } else { "<:" })?;
    let end = body.find('>')?;
    let (name, id) = body[..end].split_once(':')?;
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let id: Id = id.parse().ok()?;
    Some((end + if animated { 4 } else { 3 }, name.to_string(), id, animated))
}

pub fn emoji_url(id: Id, animated: bool) -> String {
    let gif = animated && view().animate_emoji;
    // 96 like discord's jumbo: a 48pt emoji at 150% or 200% scale would blur from 48
    format!("{}/emojis/{}.{}?size=96", api::cdn(), id, if gif { "gif" } else { "png" })
}

fn is_invite(url: &str) -> bool {
    ["discord.gg/", "discord.com/invite/", "discordapp.com/invite/"].iter().any(|p| url.contains(p))
}

fn is_jumbo(s: &str) -> bool {
    let mut rest = s.trim();
    let mut n = 0;
    while !rest.is_empty() {
        if let Some((len, ..)) = custom_emoji(rest) {
            rest = rest[len..].trim_start();
        } else if let Some(len) = take_emoji(rest) {
            rest = rest[len..].trim_start();
        } else {
            return false;
        }
        n += 1;
    }
    n > 0 && n <= 30
}

fn url_len(s: &str) -> usize {
    let mut end = s.find(|c: char| c.is_whitespace() || c == '<').unwrap_or(s.len());
    let opens = s[..end].matches('(').count();
    while end > 0 {
        let c = s.as_bytes()[end - 1];
        let strip = matches!(c, b'.' | b',' | b'!' | b'?' | b':' | b';' | b'\'' | b'"')
            || (c == b')' && s[..end].matches(')').count() > opens);
        if !strip {
            break;
        }
        end -= 1;
    }
    end
}

pub static CLOCK24: AtomicBool = AtomicBool::new(false);
// 0 month first, 1 day first, 2 year first: how the account's language writes a numeric date
static DATE_ORDER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_date_order(locale: &str) {
    let order = match locale {
        "en-US" => 0,
        "ja" | "zh-CN" | "zh-TW" | "ko" | "hu" | "lt" | "sv-SE" => 2,
        _ => 1,
    };
    DATE_ORDER.store(order, Ordering::Relaxed);
}

/// strftime for a numeric date, `year` being the pattern's year (`%y` or `%Y`) and `pad` whether
/// day and month keep their leading zero
pub fn short_date(year: &str, pad: bool) -> String {
    let (m, d) = if pad { ("%m", "%d") } else { ("%-m", "%-d") };
    match DATE_ORDER.load(Ordering::Relaxed) {
        0 => format!("{m}/{d}/{year}"),
        1 => format!("{d}/{m}/{year}"),
        _ => format!("{year}/{m}/{d}"),
    }
}

/// strftime for an hour:minute, honouring the 24h setting
pub fn hm() -> &'static str {
    if CLOCK24.load(Ordering::Relaxed) { "%H:%M" } else { "%-I:%M %p" }
}

pub fn fmt_ts(secs: i64, style: char) -> String {
    let Some(dt) = Local.timestamp_opt(secs, 0).single() else {
        return String::new();
    };
    let numeric = short_date("%Y", true);
    let f = match style {
        't' => hm(),
        'T' => if CLOCK24.load(Ordering::Relaxed) { "%H:%M:%S" } else { "%-I:%M:%S %p" },
        'd' => numeric.as_str(),
        'D' => "%B %-d, %Y",
        'F' => return format!("{} {}", dt.format("%A, %B %-d, %Y"), dt.format(hm())),
        'R' => {
            let d = secs - Local::now().timestamp();
            let a = d.abs();
            let (n, unit) = match a {
                0..60 => (a, "second"),
                60..3600 => (a / 60, "minute"),
                3600..86400 => (a / 3600, "hour"),
                86400..2_592_000 => (a / 86400, "day"),
                2_592_000..31_536_000 => (a / 2_592_000, "month"),
                _ => (a / 31_536_000, "year"),
            };
            let s = if n == 1 { "" } else { "s" };
            return if d < 0 { format!("{n} {unit}{s} ago") } else { format!("in {n} {unit}{s}") };
        }
        _ => return format!("{} {}", dt.format("%B %-d, %Y"), dt.format(hm())),
    };
    dt.format(f).to_string()
}

fn closing(s: &str, delim: &str) -> Option<usize> {
    let inner = &s[delim.len()..];
    let mut from = 0;
    while let Some(p) = inner[from..].find(delim) {
        let at = from + p;
        if at > 0 {
            return Some(at);
        }
        from = at + 1;
        if from >= inner.len() {
            break;
        }
    }
    None
}

fn inline(b: &mut B, s: &str, st: St) {
    let mut plain = 0;
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        let prev = s[..i].chars().next_back();

        // discord's emoticon rule: the shrug keeps its backslash
        if rest.starts_with(r"¯\_(ツ)_/¯") {
            i += r"¯\_(ツ)_/¯".len();
            continue;
        }
        if let Some(after) = rest.strip_prefix('\\')
            && let Some(c) = after.chars().next()
            && c.is_ascii_punctuation()
        {
            b.text(&s[plain..i], st);
            b.text(&after[..1], st);
            i += 2;
            plain = i;
            continue;
        }

        let took = if rest.starts_with('`') {
            let d = if rest.starts_with("``") { "``" } else { "`" };
            closing(rest, d).map(|end| {
                b.text(&s[plain..i], st);
                let mut s2 = st;
                s2.code = true;
                b.text(&rest[d.len()..d.len() + end], s2);
                end + d.len() * 2
            })
        } else if rest.starts_with("**") {
            wrap(b, s, plain, i, st, "**", |s| s.bold = true)
        } else if rest.starts_with("__") {
            wrap(b, s, plain, i, st, "__", |s| s.under = true)
        } else if rest.starts_with("~~") {
            wrap(b, s, plain, i, st, "~~", |s| s.strike = true)
        } else if rest.starts_with("||") && closing(rest, "||").is_some() {
            b.text(&s[plain..i], st);
            let start = b.chars;
            let r = wrap(b, s, i, i, st, "||", |s| s.spoiler = true);
            b.links.push((start..b.chars, Link::Spoiler));
            r
        } else if (rest.starts_with('*') && !rest[1..].starts_with(char::is_whitespace))
            || (rest.starts_with('_') && !prev.is_some_and(char::is_alphanumeric))
        {
            let d = &rest[..1];
            wrap(b, s, plain, i, st, d, |s| s.italic = true)
        } else if rest.starts_with('<') {
            angle(b, s, plain, i, st)
        } else if rest.starts_with("https://") || rest.starts_with("http://") {
            let n = url_len(rest);
            b.text(&s[plain..i], st);
            let url = &rest[..n];
            let v = view();
            if v.hide_invites && is_invite(url) {
                // streamer mode: never show an invite code on stream
                let mut s2 = st;
                s2.color = pal().text_muted;
                s2.italic = true;
                b.text("invite link hidden", s2);
            } else {
                let mut s2 = st;
                s2.color = pal().link;
                s2.under |= v.underline_links;
                b.linked(url, s2, Link::Url(url.to_string()));
            }
            Some(n)
        } else if rest.starts_with('[') {
            masked(rest).map(|(n, text, url)| {
                b.text(&s[plain..i], st);
                let mut s2 = st;
                s2.color = pal().link;
                s2.under |= view().underline_links;
                let link = if text == url { Link::Url(url.to_string()) } else { Link::Masked(url.to_string()) };
                b.linked(text, s2, link);
                n
            })
        } else if rest.starts_with("@everyone") || rest.starts_with("@here") {
            let n = if rest.starts_with("@here") { 5 } else { 9 };
            b.text(&s[plain..i], st);
            let mut s2 = st;
            s2.color = pal().mention_fg;
            b.pill(&rest[..n], s2, pal().mention_bg, None);
            Some(n)
        } else if let Some(n) = take_emoji(rest) {
            b.text(&s[plain..i], st);
            b.emoji(twemoji(&rest[..n]), st);
            Some(n)
        } else {
            None
        };

        match took {
            Some(n) => {
                i += n;
                plain = i;
            }
            None => i += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    b.text(&s[plain..], st);
}

fn wrap(b: &mut B, s: &str, plain: usize, i: usize, st: St, delim: &str, f: fn(&mut St)) -> Option<usize> {
    let rest = &s[i..];
    let end = closing(rest, delim)?;
    let inner = &rest[delim.len()..delim.len() + end];
    b.text(&s[plain..i], st);
    let mut s2 = st;
    f(&mut s2);
    inline(b, inner, s2);
    Some(end + delim.len() * 2)
}

fn masked(s: &str) -> Option<(usize, &str, &str)> {
    let close = s.find("](")?;
    let text = &s[1..close];
    if text.is_empty() || text.contains('\n') {
        return None;
    }
    let after = &s[close + 2..];
    if !(after.starts_with("https://") || after.starts_with("http://")) {
        return None;
    }
    let end = after.find(')')?;
    let url = &after[..end];
    if url.contains(char::is_whitespace) {
        return None;
    }
    Some((close + 2 + end + 1, text, url))
}

fn angle(b: &mut B, s: &str, plain: usize, i: usize, st: St) -> Option<usize> {
    let rest = &s[i..];
    if let Some((n, name, id, animated)) = custom_emoji(rest) {
        b.text(&s[plain..i], st);
        let start = b.chars;
        b.emoji(emoji_url(id, animated), st);
        b.links.push((start..b.chars, Link::Emoji(id, name, animated)));
        return Some(n);
    }
    let end = rest.find('>')?;
    let inner = &rest[1..end];
    let n = end + 1;
    let pill = |b: &mut B, text: &str, link: Option<Link>, color: Color32, bg: Color32| {
        b.text(&s[plain..i], st);
        let mut s2 = st;
        s2.color = color;
        match link {
            Some(l) if bg == Color32::TRANSPARENT => b.linked(text, s2, l),
            _ => b.pill(text, s2, bg, link),
        }
    };
    if let Some(id) = inner.strip_prefix("@&").and_then(|x| x.parse::<Id>().ok()) {
        let role = b.cx.store.guilds.get(&b.cx.guild).and_then(|g| g.roles.get(&id));
        let name = role.map(|r| format!("@{}", r.name)).unwrap_or_else(|| "@unknown-role".into());
        let (fg, bg) = match role.filter(|r| r.color != 0) {
            Some(r) => {
                let c = role_color(r.color);
                (c, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 30))
            }
            None => (pal().mention_fg, pal().mention_bg),
        };
        pill(b, &name, None, fg, bg);
        return Some(n);
    }
    if let Some(id) = inner.strip_prefix('@').map(|x| x.trim_start_matches('!')).and_then(|x| x.parse::<Id>().ok()) {
        let known = b.cx.store.users.contains_key(&id) || b.cx.store.members.get(&b.cx.guild).is_some_and(|m| m.contains_key(&id));
        let name = if b.cx.embed && !known { format!("<@{id}>") } else { format!("@{}", b.cx.store.name_in(b.cx.guild, id)) };
        pill(b, &name, Some(Link::User(id)), pal().mention_fg, pal().mention_bg);
        return Some(n);
    }
    if let Some(id) = inner.strip_prefix('#').and_then(|x| x.parse::<Id>().ok()) {
        let name = b
            .cx
            .store
            .channels
            .get(&id)
            .map(|c| format!("#{}", c.name))
            .unwrap_or_else(|| "#unknown".into());
        pill(b, &name, Some(Link::Channel(id)), pal().mention_fg, pal().mention_bg);
        return Some(n);
    }
    if let Some(t) = inner.strip_prefix("t:") {
        let (secs, style) = t.split_once(':').unwrap_or((t, "f"));
        let secs: i64 = secs.parse().ok()?;
        let text = fmt_ts(secs, style.chars().next().unwrap_or('f'));
        pill(b, &text, None, st.color, Color32::from_white_alpha(15));
        return Some(n);
    }
    if (inner.starts_with("https://") || inner.starts_with("http://")) && !inner.contains(char::is_whitespace) {
        pill(b, inner, Some(Link::Url(inner.to_string())), pal().link, Color32::TRANSPARENT);
        return Some(n);
    }
    None
}

fn base(cx: &Cx) -> St {
    St {
        bold: false,
        italic: false,
        under: false,
        strike: false,
        code: false,
        spoiler: false,
        size: cx.size,
        color: cx.color,
    }
}

fn new_b<'a, 'b>(cx: &'b Cx<'a>, jumbo: bool) -> B<'a, 'b> {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    B { cx, job, chars: 0, emojis: Vec::new(), links: Vec::new(), pills: Vec::new(), pad: 0.0, jumbo }
}

fn finish(b: B) -> Block {
    Block::Text(Inline { job: b.job, emojis: b.emojis, links: b.links, pills: b.pills })
}

fn text_lines(cx: &Cx, text: &str, jumbo: bool, out: &mut Vec<Block>) {
    let mut b = new_b(cx, jumbo);
    let mut quote: Vec<&str> = Vec::new();
    let mut lines = text.split('\n').peekable();
    let mut first = true;
    let flush_quote = |quote: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !quote.is_empty() {
            let inner = quote.join("\n");
            let mut q = Vec::new();
            text_lines(cx, &inner, false, &mut q);
            out.push(Block::Quote(q));
            quote.clear();
        }
    };
    while let Some(line) = lines.next() {
        if let Some(q) = line.strip_prefix(">>> ") {
            if b.chars > 0 {
                out.push(finish(std::mem::replace(&mut b, new_b(cx, jumbo))));
            }
            flush_quote(&mut quote, out);
            quote.push(q);
            quote.extend(lines.by_ref());
            break;
        }
        if let Some(q) = line.strip_prefix("> ").or(if line == ">" { Some("") } else { None }) {
            if b.chars > 0 {
                out.push(finish(std::mem::replace(&mut b, new_b(cx, jumbo))));
                first = true;
            }
            quote.push(q);
            continue;
        }
        flush_quote(&mut quote, out);
        if !first {
            b.text("\n", base(cx));
        }
        first = false;
        let mut st = base(cx);
        let mut body = line;
        for (p, size) in [("### ", 16.0), ("## ", 20.0), ("# ", 24.0)] {
            if let Some(rest) = line.strip_prefix(p) {
                st.size = size;
                st.bold = true;
                st.color = pal().header_primary;
                body = rest;
                break;
            }
        }
        let trimmed = body.trim_start();
        let indent = body.len() - trimmed.len();
        if let Some(item) = trimmed.strip_prefix("- ").or(trimmed.strip_prefix("* ")) {
            b.text(&format!("{}• ", "    ".repeat(indent / 2 + 1)), st);
            body = item;
        } else if let Some(rest) = trimmed.strip_prefix("-# ") {
            st.size = 13.0;
            st.color = pal().text_muted;
            body = rest;
        }
        inline(&mut b, body, st);
    }
    flush_quote(&mut quote, out);
    if b.chars > 0 {
        out.push(finish(b));
    }
}

pub fn parse(src: &str, cx: &Cx) -> Vec<Block> {
    crate::theme::note_text(src);
    let src = src.trim();
    let jumbo = is_jumbo(src);
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("```") else { break };
        let before = rest[..start].trim_end_matches('\n');
        if !before.is_empty() {
            text_lines(cx, before, false, &mut out);
        }
        let mut code = &after[..end];
        if let Some(nl) = code.find('\n') {
            let first = &code[..nl];
            if !first.is_empty() && !first.contains(char::is_whitespace) {
                code = &code[nl + 1..];
            } else if first.is_empty() {
                code = &code[1..];
            }
        }
        out.push(Block::Code(code.trim_end_matches('\n').to_string()));
        rest = after[end + 3..].trim_start_matches('\n');
    }
    if !rest.is_empty() {
        text_lines(cx, rest, jumbo, &mut out);
    }
    if cx.edited {
        let mut st = base(cx);
        st.size = 10.0;
        st.color = pal().text_muted;
        if let Some(Block::Text(t)) = out.last_mut() {
            let f = TextFormat {
                font_id: reg(10.0),
                color: pal().text_muted,
                // its own height, so it sits on the line rather than at the top of it
                valign: Align::Center,
                ..Default::default()
            };
            t.job.append(" (edited)", 0.0, f);
        } else {
            let mut b = new_b(cx, false);
            b.text("(edited)", st);
            out.push(finish(b));
        }
    }
    out
}

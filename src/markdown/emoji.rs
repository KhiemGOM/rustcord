use crate::api;
use crate::state::Id;
use crate::theme::view;

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

pub fn twemoji(seq: &str) -> String {
    let zwj = seq.contains('\u{200d}');
    let parts: Vec<String> = seq
        .chars()
        .filter(|c| zwj || *c != '\u{fe0f}')
        .map(|c| format!("{:x}", c as u32))
        .collect();
    format!("{}/{}.png", api::emoji_cdn(), parts.join("-"))
}

pub(super) fn custom_emoji(s: &str) -> Option<(usize, String, Id, bool)> {
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

pub(super) fn is_jumbo(s: &str) -> bool {
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

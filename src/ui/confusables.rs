//! Confusable, homoglyph, and punycode host checks for external links.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    BidiOverride,
    Punycode,
    MixedScript,
    Lookalike(&'static str),
}

impl Risk {
    pub fn notice(self) -> String {
        match self {
            Risk::BidiOverride => "This link contains hidden text-direction control characters that can disguise the real destination.".into(),
            Risk::Punycode => "This link uses an internationalized (punycode) domain name. Double-check the address before continuing.".into(),
            Risk::MixedScript => "This domain mixes characters from different alphabets, a common technique used to spoof trusted sites.".into(),
            Risk::Lookalike(brand) => format!("This domain closely resembles {brand} and may be a phishing link."),
        }
    }
}

const PROTECTED: &[&str] = &[
    "discord.com",
    "discord.gg",
    "discordapp.com",
    "discordapp.net",
    "discord.gift",
    "discord.new",
    "steamcommunity.com",
    "steampowered.com",
];

fn confusable_ascii(c: char) -> char {
    match c {
        // Cyrillic lookalikes
        '\u{0430}' | '\u{0410}' => 'a',
        '\u{0435}' | '\u{0415}' => 'e',
        '\u{043E}' | '\u{041E}' => 'o',
        '\u{0440}' | '\u{0420}' => 'p',
        '\u{0441}' | '\u{0421}' => 'c',
        '\u{0443}' | '\u{0423}' => 'y',
        '\u{0445}' | '\u{0425}' => 'x',
        '\u{0456}' | '\u{0406}' => 'i',
        '\u{0455}' | '\u{0405}' => 's',
        '\u{0458}' | '\u{0408}' => 'j',
        '\u{04BB}' => 'h',
        '\u{0501}' => 'd',
        '\u{043C}' | '\u{041C}' => 'm',
        '\u{0442}' | '\u{0422}' => 't',
        '\u{043A}' | '\u{041A}' => 'k',
        '\u{0432}' | '\u{0412}' => 'b',
        // Greek lookalikes
        '\u{03B1}' | '\u{0391}' => 'a',
        '\u{03B5}' | '\u{0395}' => 'e',
        '\u{03BF}' | '\u{039F}' => 'o',
        '\u{03C1}' | '\u{03A1}' => 'p',
        '\u{03B9}' | '\u{0399}' => 'i',
        '\u{03BA}' | '\u{039A}' => 'k',
        '\u{03BD}' => 'v',
        '\u{03C7}' | '\u{03A7}' => 'x',
        '\u{03C4}' | '\u{03A4}' => 't',
        // Common ASCII / digit confusables
        '0' => 'o',
        '3' => 'e',
        '5' => 's',
        // Fullwidth Latin
        '\u{FF21}'..='\u{FF3A}' => char::from_u32(c as u32 - 0xFF21 + b'a' as u32).unwrap_or(c),
        '\u{FF41}'..='\u{FF5A}' => char::from_u32(c as u32 - 0xFF41 + b'a' as u32).unwrap_or(c),
        _ => c.to_ascii_lowercase(),
    }
}

fn skeleton(host: &str) -> String {
    let raw: String = host.chars().map(confusable_ascii).collect();
    raw.replace("rn", "m")
        .replace("vv", "w")
        .replace("cl", "d")
        .chars()
        .map(|c| if matches!(c, '1' | 'l') { 'i' } else { c })
        .collect()
}

fn is_cyrillic(c: char) -> bool {
    matches!(c as u32, 0x0400..=0x052F)
}

fn is_greek(c: char) -> bool {
    matches!(c as u32, 0x0370..=0x03FF)
}

fn has_mixed_script(host: &str) -> bool {
    for label in host.split('.') {
        let mut latin = false;
        let mut cyr = false;
        let mut greek = false;
        for c in label.chars() {
            if c.is_ascii_alphabetic() {
                latin = true;
            } else if is_cyrillic(c) {
                cyr = true;
            } else if is_greek(c) {
                greek = true;
            }
        }
        let scripts = u8::from(latin) + u8::from(cyr) + u8::from(greek);
        if scripts > 1 {
            return true;
        }
    }
    false
}

fn base_domain(host: &str) -> &str {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() <= 2 {
        return host;
    }
    let start = host.len() - parts[parts.len() - 2].len() - parts[parts.len() - 1].len() - 1;
    &host[start..]
}

fn has_bidi_control(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069))
}

/// Decodes an RFC 3492 punycode label (the part after `xn--`).
fn decode_punycode(input: &str) -> Option<String> {
    const BASE: u32 = 36;
    const TMIN: u32 = 1;
    const TMAX: u32 = 26;
    const SKEW: u32 = 38;
    const DAMP: u32 = 700;
    let bytes = input.as_bytes();
    let delim = bytes.iter().rposition(|&b| b == b'-');
    let mut out: Vec<char> = match delim {
        Some(pos) => bytes[..pos].iter().map(|&b| b as char).collect(),
        None => Vec::new(),
    };
    let mut idx = delim.map_or(0, |p| p + 1);
    let mut n = 128u32;
    let mut i = 0u32;
    let mut bias = 72u32;
    while idx < bytes.len() {
        let oldi = i;
        let mut w = 1u32;
        let mut k = BASE;
        loop {
            if idx >= bytes.len() {
                return None;
            }
            let b = bytes[idx];
            idx += 1;
            let digit = match b {
                b'a'..=b'z' => (b - b'a') as u32,
                b'A'..=b'Z' => (b - b'A') as u32,
                b'0'..=b'9' => (b - b'0' + 26) as u32,
                _ => return None,
            };
            i = i.checked_add(digit.checked_mul(w)?)?;
            let t = if k <= bias { TMIN } else if k >= bias + TMAX { TMAX } else { k - bias };
            if digit < t {
                break;
            }
            w = w.checked_mul(BASE - t)?;
            k += BASE;
        }
        let len = (out.len() + 1) as u32;
        let mut delta = if oldi == 0 { (i - oldi) / DAMP } else { (i - oldi) >> 1 };
        delta += delta / len;
        let mut k2 = 0u32;
        while delta > ((BASE - TMIN) * TMAX) / 2 {
            delta /= BASE - TMIN;
            k2 += BASE;
        }
        bias = k2 + ((BASE - TMIN + 1) * delta) / (delta + SKEW);
        n = n.checked_add(i / len)?;
        i %= len;
        let ch = char::from_u32(n)?;
        out.insert(i as usize, ch);
        i += 1;
    }
    Some(out.into_iter().collect())
}

fn decode_idn_host(host: &str) -> (String, bool) {
    let mut any_puny = false;
    let decoded: Vec<String> = host
        .split('.')
        .map(|label| {
            if let Some(rest) = label.strip_prefix("xn--") {
                any_puny = true;
                decode_punycode(rest).unwrap_or_else(|| label.to_string())
            } else {
                label.to_string()
            }
        })
        .collect();
    (decoded.join("."), any_puny)
}

pub fn check_url(raw_url: &str, raw_host: &str) -> Option<Risk> {
    if has_bidi_control(raw_url) || has_bidi_control(raw_host) {
        return Some(Risk::BidiOverride);
    }
    check_host(raw_host)
}

pub fn check_host(raw_host: &str) -> Option<Risk> {
    let h = raw_host.trim().trim_end_matches('.').to_lowercase();
    if h.is_empty() {
        return None;
    }
    if has_bidi_control(&h) {
        return Some(Risk::BidiOverride);
    }
    if PROTECTED.iter().any(|p| h == *p || h.ends_with(&format!(".{p}"))) {
        return None;
    }
    let (decoded, is_puny) = decode_idn_host(&h);
    if has_bidi_control(&decoded) {
        return Some(Risk::BidiOverride);
    }
    let skel = skeleton(base_domain(&decoded));
    for &brand in PROTECTED {
        if skel == skeleton(brand) {
            return Some(Risk::Lookalike(brand));
        }
    }
    if has_mixed_script(&decoded) {
        return Some(Risk::MixedScript);
    }
    if is_puny {
        return Some(Risk::Punycode);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_legitimate_domains() {
        assert_eq!(check_host("discord.com"), None);
        assert_eq!(check_host("cdn.discordapp.com"), None);
        assert_eq!(check_host("github.com"), None);
        assert_eq!(check_host("example.org"), None);
    }

    #[test]
    fn catches_punycode_mixed_script_bidi_and_lookalikes() {
        assert_eq!(decode_punycode("discrd-zxa"), Some("discörd".to_string()));
        assert_eq!(decode_punycode("discrd-zqf"), Some("disc\u{043E}rd".to_string()));
        assert_eq!(check_host("xn--discrd-zqf.com"), Some(Risk::Lookalike("discord.com")));
        assert_eq!(check_host("xn--exmple-4nf.com"), Some(Risk::MixedScript));
        assert_eq!(check_host("xn--bcher-kva.example"), Some(Risk::Punycode));
        assert_eq!(check_host("disc\u{043E}rd.com"), Some(Risk::Lookalike("discord.com")));
        assert_eq!(check_host("ex\u{0430}mple.com"), Some(Risk::MixedScript));
        assert_eq!(check_url("https://example.com/\u{202E}exe.png", "example.com"), Some(Risk::BidiOverride));
        assert_eq!(check_host("d1scord.com"), Some(Risk::Lookalike("discord.com")));
        assert_eq!(check_host("discorcl.com"), Some(Risk::Lookalike("discord.com")));
        assert_eq!(check_host("dlscord.gg"), Some(Risk::Lookalike("discord.gg")));
        assert_eq!(check_host("steamcornmunity.com"), Some(Risk::Lookalike("steamcommunity.com")));
    }
}

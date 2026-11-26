//! Confusable, homoglyph, and punycode host checks for external links.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    Punycode,
    MixedScript,
    Lookalike(&'static str),
}

impl Risk {
    pub fn notice(self) -> String {
        match self {
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

pub fn check_host(raw_host: &str) -> Option<Risk> {
    let h = raw_host.trim().trim_end_matches('.').to_lowercase();
    if h.is_empty() {
        return None;
    }
    if PROTECTED.iter().any(|p| h == *p || h.ends_with(&format!(".{p}"))) {
        return None;
    }
    if has_mixed_script(&h) {
        return Some(Risk::MixedScript);
    }
    let skel = skeleton(base_domain(&h));
    for &brand in PROTECTED {
        if skel == skeleton(brand) {
            return Some(Risk::Lookalike(brand));
        }
    }
    if h.split('.').any(|l| l.starts_with("xn--")) {
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
    fn catches_punycode_mixed_script_and_lookalikes() {
        assert_eq!(check_host("xn--discrd-9za.com"), Some(Risk::Punycode));
        assert_eq!(check_host("disc\u{043E}rd.com"), Some(Risk::MixedScript));
        assert_eq!(check_host("d1scord.com"), Some(Risk::Lookalike("discord.com")));
        assert_eq!(check_host("discorcl.com"), Some(Risk::Lookalike("discord.com")));
        assert_eq!(check_host("dlscord.gg"), Some(Risk::Lookalike("discord.gg")));
        assert_eq!(check_host("steamcornmunity.com"), Some(Risk::Lookalike("steamcommunity.com")));
    }
}

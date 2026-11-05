//! Spellcheck for the composer, through Windows' own spell checker as discord's desktop app does
//! through Chromium's. Words go to a worker thread that owns the checker and are answered
//! later; RUSTCORD_SPELL_WORDS=<file> swaps in a plain word list, which is what the e2e uses.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};

enum Req {
    Check(Vec<String>),
    Suggest(String),
}

enum Reply {
    Checked(Vec<(String, bool)>),
    Suggested(String, Vec<String>),
}

trait Backend {
    fn misspelled(&self, word: &str) -> bool;
    fn suggest(&self, word: &str) -> Vec<String>;
}

struct List(HashSet<String>);

impl Backend for List {
    fn misspelled(&self, word: &str) -> bool {
        !self.0.contains(&word.to_lowercase())
    }

    fn suggest(&self, word: &str) -> Vec<String> {
        let w = word.to_lowercase();
        let mut near: Vec<(usize, &String)> = self.0.iter().filter_map(|c| Some((distance(&w, c)?, c))).collect();
        near.sort();
        near.into_iter().take(5).map(|(_, c)| c.clone()).collect()
    }
}

fn distance(a: &str, b: &str) -> Option<usize> {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 2 {
        return None;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + (ca != cb) as usize).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    Some(row[b.len()]).filter(|d| *d <= 2)
}

struct Windows(windows::Win32::Globalization::ISpellChecker);

impl Windows {
    fn open(tag: &str) -> Option<Self> {
        use windows::Win32::Globalization::{ISpellCheckerFactory, SpellCheckerFactory};
        use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let f: ISpellCheckerFactory = CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_ALL).ok()?;
            let tag = windows::core::HSTRING::from(tag);
            if !f.IsSupported(&tag).ok()?.as_bool() {
                return None;
            }
            f.CreateSpellChecker(&tag).ok().map(Windows)
        }
    }
}

impl Backend for Windows {
    fn misspelled(&self, word: &str) -> bool {
        let w = windows::core::HSTRING::from(word);
        unsafe {
            let Ok(e) = self.0.Check(&w) else { return false };
            let mut first = None;
            e.Next(&mut first).is_ok() && first.is_some()
        }
    }

    fn suggest(&self, word: &str) -> Vec<String> {
        use windows::Win32::System::Com::CoTaskMemFree;
        let w = windows::core::HSTRING::from(word);
        let mut out = Vec::new();
        unsafe {
            let Ok(e) = self.0.Suggest(&w) else { return out };
            loop {
                let mut s = [windows::core::PWSTR::null()];
                let mut got = 0u32;
                if e.Next(&mut s, Some(&mut got)).is_err() || got == 0 || s[0].is_null() {
                    break;
                }
                out.push(s[0].to_string().unwrap_or_default());
                CoTaskMemFree(Some(s[0].0 as _));
                if out.len() == 5 {
                    break;
                }
            }
        }
        out
    }
}

fn backend(tag: &str) -> Option<Box<dyn Backend>> {
    if let Ok(p) = std::env::var("RUSTCORD_SPELL_WORDS") {
        let words = std::fs::read_to_string(p).ok()?.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect();
        return Some(Box::new(List(words)));
    }
    Windows::open(tag).map(|w| Box::new(w) as Box<dyn Backend>)
}

pub struct Speller {
    tx: Sender<Req>,
    rx: Receiver<Reply>,
    pub tag: String,
    known: HashMap<String, bool>,
    asked: HashSet<String>,
    suggested: Option<(String, Vec<String>)>,
}

impl Speller {
    pub fn new(tag: &str, ctx: &egui::Context) -> Self {
        let (tx, jobs) = channel::<Req>();
        let (out, rx) = channel::<Reply>();
        let (t, ctx) = (tag.to_string(), ctx.clone());
        let _ = std::thread::Builder::new().name("spellcheck".into()).spawn(move || {
            let Some(b) = backend(&t) else { return };
            while let Ok(job) = jobs.recv() {
                let reply = match job {
                    Req::Check(words) => Reply::Checked(words.into_iter().map(|w| { let bad = b.misspelled(&w); (w, bad) }).collect()),
                    Req::Suggest(w) => Reply::Suggested(w.clone(), b.suggest(&w)),
                };
                if out.send(reply).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Speller { tx, rx, tag: tag.into(), known: HashMap::new(), asked: HashSet::new(), suggested: None }
    }

    pub fn poll(&mut self) {
        while let Ok(r) = self.rx.try_recv() {
            match r {
                Reply::Checked(v) => self.known.extend(v),
                Reply::Suggested(w, s) => self.suggested = Some((w, s)),
            }
        }
        if self.known.len() > 4000 {
            self.known.clear();
            self.asked.clear();
        }
    }

    pub fn ask<'a>(&mut self, words: impl Iterator<Item = &'a str>) {
        let new: Vec<String> = words.filter(|w| !self.known.contains_key(*w) && self.asked.insert(w.to_string())).map(String::from).collect();
        if !new.is_empty() {
            let _ = self.tx.send(Req::Check(new));
        }
    }

    pub fn bad(&self, word: &str) -> bool {
        self.known.get(word).copied().unwrap_or(false)
    }

    pub fn want_suggestions(&mut self, word: &str) {
        if self.suggested.as_ref().is_none_or(|s| s.0 != word) {
            self.suggested = None;
            let _ = self.tx.send(Req::Suggest(word.to_string()));
        }
    }

    pub fn suggestions(&self, word: &str) -> &[String] {
        self.suggested.as_ref().filter(|s| s.0 == word).map_or(&[], |s| &s.1)
    }
}

/// Char ranges of the words worth checking: not in code, links, mentions, emoji or names.
pub fn words(text: &str) -> Vec<(usize, usize, String)> {
    let c: Vec<char> = text.chars().collect();
    if c.first() == Some(&'/') {
        return Vec::new();
    }
    let mut skip = vec![false; c.len()];
    let mut i = 0;
    while i < c.len() {
        let end = match c[i] {
            '`' => {
                let n = c[i..].iter().take_while(|x| **x == '`').count();
                let close = (i + n..c.len()).find(|j| c[*j] == '`' && c[*j..].iter().take_while(|x| **x == '`').count() == n);
                match close {
                    Some(j) => j + n,
                    None if n >= 3 => c.len(),
                    None => i + n,
                }
            }
            '<' => match c[i..].iter().position(|x| x.is_whitespace() || *x == '>') {
                Some(p) if c[i + p] == '>' => i + p + 1,
                _ => i + 1,
            },
            ':' => {
                let n = c[i + 1..].iter().take_while(|x| x.is_alphanumeric() || **x == '_' || **x == '-' || **x == '+').count();
                if n >= 2 && c.get(i + 1 + n) == Some(&':') { i + n + 2 } else { i + 1 }
            }
            'h' | 'w' if (i == 0 || c[i - 1].is_whitespace()) && ["http://", "https://", "www."].iter().any(|p| c[i..].iter().take(p.len()).copied().eq(p.chars())) => {
                c[i..].iter().position(|x| x.is_whitespace()).map_or(c.len(), |p| i + p)
            }
            _ => i + 1,
        };
        if end > i + 1 {
            skip[i..end].fill(true);
        }
        i = end.max(i + 1);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if !c[i].is_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < c.len() && (c[i].is_alphabetic() || (matches!(c[i], '\'' | '\u{2019}') && c.get(i + 1).is_some_and(|x| x.is_alphabetic()))) {
            i += 1;
        }
        let glued = (start > 0 && (c[start - 1].is_numeric() || matches!(c[start - 1], '_' | '@' | '#' | '/' | '.'))) || c.get(i).is_some_and(|x| x.is_numeric() || *x == '_');
        if i - start >= 2 && !glued && !skip[start..i].iter().any(|s| *s) {
            out.push((start, i, c[start..i].iter().collect::<String>().replace('\u{2019}', "'")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(t: &str) -> Vec<String> {
        words(t).into_iter().map(|w| w.2).collect()
    }

    #[test]
    fn plain_words() {
        assert_eq!(found("hello wrld, don't stop"), ["hello", "wrld", "don't", "stop"]);
    }

    #[test]
    fn skips_code_links_mentions_emoji() {
        assert_eq!(found("see `foo bar` and ```\nbaz qux\n``` ok"), ["see", "and", "ok"]);
        assert_eq!(found("go to https://exmple.com/pth now"), ["go", "to", "now"]);
        assert_eq!(found("hi <@123> <#456> <:smle:789> <a:dnce:1> :thmbsup: yo"), ["hi", "yo"]);
        assert!(found("@someone #chanel abc123 snake_case").is_empty());
        assert!(found("/giphy cats").is_empty());
        assert_eq!(found("open ```code"), ["open"]);
    }

    #[test]
    fn list_backend_suggests() {
        let b = List(["hello", "help", "world"].iter().map(|s| s.to_string()).collect());
        assert!(b.misspelled("helo") && !b.misspelled("Hello"));
        assert_eq!(b.suggest("helo")[0], "hello");
    }
}

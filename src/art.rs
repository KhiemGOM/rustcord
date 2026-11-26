//! Discord's icons, fetched from discord.com the way the web client gets them. assets/art.tsv has
//! each icon's layout with its paths as fingerprints (tools/art_table.py makes it); the paths come
//! out of Discord's bundle on the first start and the finished svgs live in the cache dir after.

use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use tokio::runtime::Runtime;

use crate::{api, cache};

const TABLE: &str = include_str!("../assets/art.tsv");

static ART: RwLock<Option<HashMap<&'static str, &'static [u8]>>> = RwLock::new(None);
static FETCHED: AtomicUsize = AtomicUsize::new(0);

fn rows() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    TABLE.lines().filter_map(|l| {
        let mut f = l.splitn(3, '\t');
        Some((f.next()?, f.next()?, f.next()?))
    })
}

fn origin() -> String {
    std::env::var("RUSTCORD_ART").unwrap_or_else(|_| "https://discord.com".into())
}

fn path(name: &str) -> PathBuf {
    cache::dir().join("art").join(format!("{name}.svg"))
}

/// The icon's svg once it's here.
pub fn get(name: &str) -> Option<&'static [u8]> {
    ART.read().unwrap().as_ref()?.get(name).copied()
}

/// How many icons this run downloaded, for the e2e run.
pub fn fetched() -> usize {
    FETCHED.load(Relaxed)
}

fn put(name: &'static str, svg: Vec<u8>) {
    ART.write().unwrap().get_or_insert_with(HashMap::new).insert(name, Vec::leak(svg));
}

fn valid_svg(b: &[u8]) -> bool {
    resvg::usvg::Tree::from_data(b, &resvg::usvg::Options::default()).is_ok()
}

/// Reads the cached icons, then downloads whatever isn't cached yet, all in the background.
pub fn fetch(rt: &Runtime, ctx: egui::Context) {
    rt.spawn(async move {
        let mut wants: HashMap<&'static str, Vec<(&'static str, &'static str)>> = HashMap::new();
        for (name, seed, tpl) in rows() {
            match std::fs::read(path(name)).ok().filter(|b| valid_svg(b)) {
                Some(b) => put(name, b),
                None if seed.is_empty() => put(name, tpl.as_bytes().to_vec()),
                None => wants.entry(seed).or_default().push((name, tpl)),
            }
        }
        ctx.request_repaint();
        if wants.is_empty() {
            return;
        }
        let http = api::http();
        let base = origin();
        let mut found: Option<HashMap<String, String>> = None;
        for (seed, icons) in wants {
            let mut paths = fetch_text(&http, &format!("{base}/assets/{seed}.js")).await.map(|js| paths(&js)).unwrap_or_default();
            if icons.iter().any(|(_, tpl)| fill(tpl, &paths).is_none()) {
                if found.is_none() {
                    found = Some(discover(&http, &base).await);
                }
                paths.extend(found.iter().flatten().map(|(k, v)| (k.clone(), v.clone())));
            }
            for (name, tpl) in icons {
                let Some(svg) = fill(tpl, &paths) else { continue };
                save(name, svg.as_bytes());
                put(name, svg.into_bytes());
                FETCHED.fetch_add(1, Relaxed);
            }
        }
        ctx.request_repaint();
    });
}

fn save(name: &str, svg: &[u8]) {
    let p = path(name);
    let Some(dir) = p.parent() else { return };
    let tmp = p.with_extension("part");
    if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&tmp, svg).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

async fn fetch_text(http: &reqwest::Client, url: &str) -> Option<String> {
    let res = http.get(url).send().await.ok()?.error_for_status().ok()?;
    res.text().await.ok()
}

fn fnv(s: &str) -> String {
    let h = s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("{h:016x}")
}

/// Every `d:"..."` in a script, by fingerprint.
fn paths(js: &str) -> HashMap<String, String> {
    js.split("d:\"").skip(1).filter_map(|s| s.split('"').next()).map(|d| (fnv(d), d.to_string())).collect()
}

/// The template with each `{fingerprint}` swapped for its path; None if one is missing.
fn fill(tpl: &str, paths: &HashMap<String, String>) -> Option<String> {
    let mut out = String::with_capacity(tpl.len() * 4);
    let mut rest = tpl;
    while let Some(at) = rest.find('{') {
        let end = rest[at..].find('}')? + at;
        out += &rest[..at];
        out += paths.get(&rest[at + 1..end])?;
        rest = &rest[end + 1..];
    }
    out += rest;
    Some(out)
}

/// The paths in /app's scripts, for when a deploy has retired a seed.
async fn discover(http: &reqwest::Client, base: &str) -> HashMap<String, String> {
    let Some(page) = fetch_text(http, &format!("{base}/app")).await else { return HashMap::new() };
    let scripts: Vec<String> = page
        .split("src=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .filter(|u| u.starts_with("/assets/") && u.ends_with(".js"))
        .map(|u| format!("{base}{u}"))
        .collect();
    let mut all = HashMap::new();
    let mut jobs = futures_util::stream::iter(scripts)
        .map(|u| {
            let http = http.clone();
            async move { fetch_text(&http, &u).await }
        })
        .buffer_unordered(6);
    while let Some(js) = jobs.next().await {
        all.extend(js.map(|js| paths(&js)).unwrap_or_default());
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_a_template_that_fills() {
        let mut n = 0;
        for (name, seed, tpl) in rows() {
            assert!(tpl.starts_with("<svg") && tpl.ends_with("</svg>"), "{name}");
            let fakes: HashMap<String, String> =
                tpl.split('{').skip(1).filter_map(|s| s.split('}').next()).map(|k| (k.to_string(), "M0 0h1v1z".to_string())).collect();
            assert_eq!(fakes.is_empty(), seed.is_empty(), "{name}");
            assert!(fill(tpl, &fakes).is_some_and(|s| !s.contains('{')), "{name}");
            n += 1;
        }
        assert!(n > 200);
    }

    #[test]
    fn every_row_parses_as_well_formed_svg() {
        for (name, _, tpl) in rows() {
            let fakes: HashMap<String, String> =
                tpl.split('{').skip(1).filter_map(|s| s.split('}').next()).map(|k| (k.to_string(), "M0 0h1v1z".to_string())).collect();
            let filled = fill(tpl, &fakes).unwrap_or_else(|| panic!("{name}: fill failed"));
            assert!(valid_svg(filled.as_bytes()), "{name}: malformed SVG: {filled}");
        }
        assert!(!valid_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h1"></svg>"#));
    }

    #[test]
    fn paths_are_found_by_fingerprint() {
        let js = r#"x("path",{fill:o,d:"M1 2h3Z",className:d}),y("path",{d:"M4 4v1Z"})"#;
        let p = paths(js);
        let tpl = format!(r#"<svg><path d="{{{}}}"/><path d="{{{}}}"/></svg>"#, fnv("M4 4v1Z"), fnv("M1 2h3Z"));
        assert_eq!(fill(&tpl, &p).unwrap(), r#"<svg><path d="M4 4v1Z"/><path d="M1 2h3Z"/></svg>"#);
        assert!(fill(r#"<svg d="{0000000000000000}"/>"#, &p).is_none());
    }
}

//! gg sans and gg mono, fetched from discord.com the same way the web client gets them.
//! Nothing is bundled: the woff2 files are decoded once into the cache dir and picked up
//! from there on the next start (or mid-run, the first time).

use futures_util::StreamExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use tokio::runtime::Runtime;

use crate::{api, cache};

/// Discord's ui faces: the cache file (also the egui font key), the @font-face it comes from
/// (family, italic, weight), and the asset. Asset names change whenever discord ships a new
/// build; these only save the discovery round trip while they're still current.
pub const FACES: [(&str, &str, bool, u16, &str); 10] = [
    ("ggsans-400", "gg sans", false, 400, "66d715454104d24e"),
    ("ggsans-500", "gg sans", false, 500, "b272b33815319bae"),
    ("ggsans-600", "gg sans", false, 600, "2df2c3ff74408972"),
    ("ggsans-700", "gg sans", false, 700, "189422196a4f8b53"),
    ("ggsans-400i", "gg sans", true, 400, "dd24010f3cf7def7"),
    ("ggsans-600i", "gg sans", true, 600, "d5d789aeb6282532"),
    ("ggsans-700i", "gg sans", true, 700, "ce3b8055f5114434"),
    ("ggmono-400", "gg mono", false, 400, "249d0a057895c668"),
    ("ggmono-700", "gg mono", false, 700, "45efa6936fdfb918"),
    ("ginto-nord-700", "ABC Ginto Discord Nord", false, 700, "f83f3e7b5185847e"),
];

/// Nitro display name fonts: discord's font_id, the family its stylesheet registers, and the
/// asset. The other ids (default, and a few retired ones) are plain gg sans.
pub const NAME_FONTS: [(u8, &str, &str); 11] = [
    (3, "Sakura", "33d4f12a85e1f736"),
    (4, "Jellybean", "e6f5f44abb520735"),
    (6, "Modern", "c560709c3470bb66"),
    (7, "Medieval", "52b541f86401a5b6"),
    (8, "8Bit", "69c735ca5c604de7"),
    (10, "Vampyre", "8f20cb550d739cea"),
    (12, "Tempo", "27b34a77ba5d693e"),
    (13, "Monkey Bars", "45ef50dce38a931a"),
    (14, "Mainframe", "4841dace333e9054"),
    (15, "Headbang", "c00df4f25667809b"),
    (16, "Journal", "aa59ae54ce7e540a"),
];

static FRESH: AtomicBool = AtomicBool::new(false);

fn origin() -> String {
    std::env::var("RUSTCORD_FONTS").unwrap_or_else(|_| "https://discord.com".into())
}

fn path(stem: &str) -> PathBuf {
    cache::dir().join("fonts").join(format!("{stem}.ttf"))
}

/// A font file mapped once for the life of the process. egui keeps a second copy of any font
/// it's handed as owned bytes, and the system fallbacks run to 13-20 MB each; mapped, they cost
/// only the pages glyphs are actually read from.
pub fn mapped(path: &std::path::Path) -> Option<&'static [u8]> {
    static MAPS: std::sync::Mutex<Vec<(PathBuf, &'static [u8])>> = std::sync::Mutex::new(Vec::new());
    let mut maps = MAPS.lock().unwrap();
    if let Some((_, b)) = maps.iter().find(|(p, _)| p == path) {
        return Some(b);
    }
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: font files are only ever written before they exist (see `fetch`), never while mapped
    let map: &'static memmap2::Mmap = Box::leak(Box::new(unsafe { memmap2::Mmap::map(&file) }.ok()?));
    maps.push((path.to_path_buf(), map));
    Some(map)
}

pub fn cached(stem: &str) -> Option<&'static [u8]> {
    mapped(&path(stem))
}

fn name_path(id: u8) -> PathBuf {
    cache::dir().join("fonts").join(format!("name-{id}.ttf"))
}

pub fn cached_name(id: u8) -> Option<&'static [u8]> {
    mapped(&name_path(id))
}

/// True once after a download finished, so the UI knows to rebuild its fonts.
pub fn take_fresh() -> bool {
    FRESH.swap(false, Relaxed)
}

/// A file to fetch: where it goes, its seed asset, and the @font-face to look for when the
/// seed is gone (family, plus italic and weight where the family has several faces).
type Want = (PathBuf, &'static str, &'static str, Option<(bool, u16)>);

pub fn fetch(rt: &Runtime, ctx: egui::Context) {
    let faces: Vec<Want> = FACES.iter().map(|&(stem, fam, it, w, seed)| (path(stem), seed, fam, Some((it, w)))).filter(|f| !f.0.exists()).collect();
    let names: Vec<Want> = NAME_FONTS.iter().map(|&(id, fam, seed)| (name_path(id), seed, fam, None)).filter(|f| !f.0.exists()).collect();
    if faces.is_empty() && names.is_empty() {
        return;
    }
    rt.spawn(async move {
        let wait = std::time::Duration::from_secs(120);
        let a = !faces.is_empty() && tokio::time::timeout(wait, download(&faces, "font-family:gg sans;")).await.ok().flatten().is_some();
        let b = !names.is_empty() && tokio::time::timeout(wait, download(&names, "font-family:Sakura;")).await.ok().flatten().is_some();
        if a || b {
            FRESH.store(true, Relaxed);
            ctx.request_repaint();
        }
    });
}

async fn save(b: &[u8], p: &PathBuf) -> Option<()> {
    let ttf = wuff::decompress_woff2(b).ok()?;
    std::fs::create_dir_all(p.parent()?).ok()?;
    let tmp = p.with_extension("part");
    std::fs::write(&tmp, ttf).ok()?;
    std::fs::rename(tmp, p).ok()
}

/// A deploy that renamed a seed sends us to the stylesheet with `sheet` in it, once.
async fn download(wants: &[Want], sheet: &str) -> Option<()> {
    let http = api::http();
    let base = origin();
    let mut saved = false;
    let mut found: Option<Vec<(String, bool, u16, String)>> = None;
    for (p, seed, family, face) in wants {
        let mut bytes = if found.is_none() { get(&http, &format!("{base}/assets/{seed}.woff2")).await } else { None };
        if bytes.is_none() {
            if found.is_none() {
                found = Some(discover(&http, &base, sheet).await.unwrap_or_default());
            }
            let hit = found.iter().flatten().find(|(f, it, w, _)| f == family && face.is_none_or(|x| x == (*it, *w)));
            if let Some((.., h)) = hit {
                bytes = get(&http, &format!("{base}/assets/{h}.woff2")).await;
            }
        }
        if let Some(b) = bytes
            && save(&b, p).await.is_some()
        {
            saved = true;
        }
    }
    saved.then_some(())
}

async fn get(http: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let res = http.get(url).send().await.ok()?.error_for_status().ok()?;
    Some(res.bytes().await.ok()?.to_vec())
}

/// Every @font-face (family, italic, weight, asset) in the first of /app's few hundred
/// stylesheets that has `sheet` in it.
async fn discover(http: &reqwest::Client, base: &str, sheet: &str) -> Option<Vec<(String, bool, u16, String)>> {
    let page = String::from_utf8(get(http, &format!("{base}/app")).await?).ok()?;
    let sheets: Vec<String> = page
        .split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .filter(|u| u.starts_with("/assets/") && u.ends_with(".css"))
        .map(|u| format!("{base}{u}"))
        .collect();
    let mut found = futures_util::stream::iter(sheets)
        .map(|u| {
            let http = http.clone();
            async move { get(&http, &u).await }
        })
        .buffer_unordered(12);
    while let Some(css) = found.next().await {
        let Some(css) = css.and_then(|b| String::from_utf8(b).ok()) else { continue };
        if css.contains(sheet) {
            return Some(faces(&css));
        }
    }
    None
}

fn faces(css: &str) -> Vec<(String, bool, u16, String)> {
    css.split("@font-face{")
        .skip(1)
        .filter_map(|f| {
            let f = &f[..f.find('}')?];
            let prop = |k: &str| f.split(k).nth(1).and_then(|x| x.split(';').next());
            let family = prop("font-family:")?.trim_matches('"');
            let weight = prop("font-weight:").and_then(|w| w.parse().ok()).unwrap_or(400);
            let h = f.split("/assets/").nth(1)?.split(".woff2").next()?;
            Some((family.to_string(), prop("font-style:") == Some("italic"), weight, h.to_string()))
        })
        .collect()
}

//! gg sans, fetched from discord.com the same way the web client gets it. Nothing
//! is bundled: the woff2 files are decoded once into the cache dir and picked up
//! from there on the next start (or mid-run, the first time).

use futures_util::StreamExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use tokio::runtime::Runtime;

use crate::{api, cache};

pub const WEIGHTS: [u16; 4] = [400, 500, 600, 700];

// asset names change whenever discord ships a new build; these only save the
// discovery round trip while they're still current
const SEEDS: [(u16, &str); 4] =
    [(400, "66d715454104d24e"), (500, "b272b33815319bae"), (600, "2df2c3ff74408972"), (700, "189422196a4f8b53")];

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
    std::env::var("FASTCORD_FONTS").unwrap_or_else(|_| "https://discord.com".into())
}

fn path(w: u16) -> PathBuf {
    cache::dir().join("fonts").join(format!("ggsans-{w}.ttf"))
}

pub fn cached(w: u16) -> Option<Vec<u8>> {
    std::fs::read(path(w)).ok()
}

fn name_path(id: u8) -> PathBuf {
    cache::dir().join("fonts").join(format!("name-{id}.ttf"))
}

pub fn cached_name(id: u8) -> Option<Vec<u8>> {
    std::fs::read(name_path(id)).ok()
}

/// True once after a download finished, so the UI knows to rebuild its fonts.
pub fn take_fresh() -> bool {
    FRESH.swap(false, Relaxed)
}

pub fn fetch(rt: &Runtime, ctx: egui::Context) {
    let gg = WEIGHTS.iter().all(|w| path(*w).exists());
    let names = NAME_FONTS.iter().all(|(id, ..)| name_path(*id).exists());
    if gg && names {
        return;
    }
    rt.spawn(async move {
        let wait = std::time::Duration::from_secs(120);
        let a = !gg && tokio::time::timeout(wait, download()).await.ok().flatten().is_some();
        let b = !names && tokio::time::timeout(wait, download_names()).await.ok().flatten().is_some();
        if a || b {
            FRESH.store(true, Relaxed);
            ctx.request_repaint();
        }
    });
}

async fn save(b: &[u8], p: PathBuf) -> Option<()> {
    let ttf = wuff::decompress_woff2(b).ok()?;
    std::fs::create_dir_all(p.parent()?).ok()?;
    let tmp = p.with_extension("part");
    std::fs::write(&tmp, ttf).ok()?;
    std::fs::rename(tmp, p).ok()
}

/// The display name fonts. A seed that's gone (a deploy renamed it) is looked up by family.
async fn download_names() -> Option<()> {
    let http = api::http();
    let base = origin();
    let mut saved = false;
    let mut found: Option<Vec<(String, String)>> = None;
    for (id, family, seed) in NAME_FONTS {
        if name_path(id).exists() {
            continue;
        }
        let mut bytes = get(&http, &format!("{base}/assets/{seed}.woff2")).await;
        if bytes.is_none() {
            if found.is_none() {
                found = Some(discover_faces(&http, &base, |css| css.contains("font-family:Sakura;")).await.unwrap_or_default());
            }
            if let Some((_, h)) = found.iter().flatten().find(|(f, _)| f == family) {
                bytes = get(&http, &format!("{base}/assets/{h}.woff2")).await;
            }
        }
        if let Some(b) = bytes
            && save(&b, name_path(id)).await.is_some()
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

async fn download() -> Option<()> {
    let http = api::http();
    let base = origin();
    let mut saved = false;
    let mut hashes = SEEDS.to_vec().into_iter().map(|(w, h)| (w, h.to_string())).collect::<Vec<_>>();
    let mut retried = false;
    let mut i = 0;
    while i < hashes.len() {
        let (w, h) = hashes[i].clone();
        if path(w).exists() {
            i += 1;
            continue;
        }
        match get(&http, &format!("{base}/assets/{h}.woff2")).await {
            Some(b) => {
                save(&b, path(w)).await?;
                saved = true;
                i += 1;
            }
            // a deploy renamed the files; read the current names off the page
            None if !retried => {
                retried = true;
                hashes = discover(&http, &base).await?;
                i = 0;
            }
            None => break,
        }
    }
    saved.then_some(())
}

/// The stylesheet with gg sans in it is one of a few hundred linked from /app.
async fn discover(http: &reqwest::Client, base: &str) -> Option<Vec<(u16, String)>> {
    let faces = discover_faces(http, base, |css| {
        let gg = gg_faces(css);
        WEIGHTS.iter().all(|w| gg.iter().any(|(fw, _)| fw == w))
    })
    .await?;
    Some(faces.into_iter().filter(|(f, _)| f.starts_with("gg sans@")).filter_map(|(f, h)| Some((f.strip_prefix("gg sans@")?.parse().ok()?, h))).collect())
}

/// Every upright @font-face (family, asset) in the first of /app's stylesheets that `wanted`
/// says has what we're after. gg sans faces come back as "gg sans@weight".
async fn discover_faces(http: &reqwest::Client, base: &str, wanted: impl Fn(&str) -> bool) -> Option<Vec<(String, String)>> {
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
        if !wanted(&css) {
            continue;
        }
        let mut faces: Vec<(String, String)> = gg_faces(&css).into_iter().map(|(w, h)| (format!("gg sans@{w}"), h)).collect();
        for f in css.split("@font-face{").skip(1) {
            let Some(f) = f.split('}').next() else { continue };
            let fam = f.split("font-family:").nth(1).and_then(|x| x.split(';').next()).map(|x| x.trim_matches('"'));
            let h = f.split("/assets/").nth(1).and_then(|x| x.split(".woff2").next());
            if let (Some(fam), Some(h)) = (fam, h)
                && NAME_FONTS.iter().any(|(_, n, _)| *n == fam)
            {
                faces.push((fam.to_string(), h.to_string()));
            }
        }
        return Some(faces);
    }
    None
}

/// (weight, asset name) of every upright gg sans face in a stylesheet.
fn gg_faces(css: &str) -> Vec<(u16, String)> {
    css.split("@font-face{")
        .skip(1)
        .filter_map(|f| {
            let f = &f[..f.find('}')?];
            if !f.contains("font-family:gg sans;") || !f.contains("font-style:normal") {
                return None;
            }
            let w = f.split("font-weight:").nth(1)?.split(';').next()?.parse().ok()?;
            let h = f.split("/assets/").nth(1)?.split(".woff2").next()?;
            Some((w, h.to_string()))
        })
        .collect()
}

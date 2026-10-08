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

/// True once after a download finished, so the UI knows to rebuild its fonts.
pub fn take_fresh() -> bool {
    FRESH.swap(false, Relaxed)
}

pub fn fetch(rt: &Runtime, ctx: egui::Context) {
    if WEIGHTS.iter().all(|w| path(*w).exists()) {
        return;
    }
    rt.spawn(async move {
        if tokio::time::timeout(std::time::Duration::from_secs(120), download()).await.ok().flatten().is_some() {
            FRESH.store(true, Relaxed);
            ctx.request_repaint();
        }
    });
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
                let ttf = wuff::decompress_woff2(&b).ok()?;
                let p = path(w);
                std::fs::create_dir_all(p.parent()?).ok()?;
                let tmp = p.with_extension("part");
                std::fs::write(&tmp, ttf).ok()?;
                std::fs::rename(tmp, p).ok()?;
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
        let faces = gg_faces(&css);
        if WEIGHTS.iter().all(|w| faces.iter().any(|(fw, _)| fw == w)) {
            return Some(faces);
        }
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

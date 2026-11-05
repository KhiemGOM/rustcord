//! The clips on disk: each `name.mp4` has a `name.json` with what the gallery shows and a
//! `name.jpg` for its card.

use egui::ColorImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Clip {
    pub id: String,
    pub title: String,
    pub app: String,
    pub created: i64,
    pub length_ms: i64,
    pub trim_start_ms: i64,
    pub users: Vec<u64>,
    pub guild: u64,
    pub channel: u64,
    pub favorite: bool,
    pub size: u64,
    #[serde(skip)]
    pub path: PathBuf,
}

impl Clip {
    pub fn thumb(&self) -> PathBuf {
        self.path.with_extension("jpg")
    }

    fn meta(&self) -> PathBuf {
        self.path.with_extension("json")
    }

    pub fn name(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        let when = chrono::DateTime::from_timestamp_millis(self.created).map(|d| d.with_timezone(&chrono::Local).format("%m/%d/%Y, %I:%M:%S %p").to_string());
        format!("Clip - {}", when.unwrap_or_default())
    }
}

pub fn write_meta(c: &Clip) {
    if let Ok(b) = serde_json::to_vec_pretty(c) {
        let _ = std::fs::write(c.meta(), b);
    }
}

pub fn thumbnail(img: &ColorImage, to: &Path) {
    let [w, h] = img.size;
    let rgb: Vec<u8> = img.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
    let Some(full) = image::RgbImage::from_raw(w as u32, h as u32, rgb) else { return };
    let small = image::imageops::thumbnail(&full, 480, (480 * h / w.max(1)).max(1) as u32);
    let _ = small.save(to);
}

/// Newest first. A video with no metadata beside it still lists, as an unnamed clip.
pub fn scan(dir: &Path) -> Vec<Clip> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Clip> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")))
        .filter_map(|path| {
            let md = std::fs::metadata(&path).ok()?;
            let mut c: Clip = std::fs::read(path.with_extension("json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| {
                let made = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64);
                Clip { id: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), created: made, ..Clip::default() }
            });
            c.size = md.len();
            c.path = path;
            Some(c)
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.created));
    out
}

pub fn delete(c: &Clip) {
    for p in [c.path.clone(), c.meta(), c.thumb()] {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_reads_metadata_and_lists_bare_videos_too() {
        let dir = std::env::temp_dir().join(format!("rustcord-lib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = Clip { id: "1".into(), title: "ace".into(), created: 5, path: dir.join("a.mp4"), favorite: true, ..Clip::default() };
        std::fs::write(&a.path, b"x").unwrap();
        write_meta(&a);
        std::fs::write(dir.join("b.mp4"), b"yy").unwrap();
        std::fs::write(dir.join("note.txt"), b"z").unwrap();
        let list = scan(&dir);
        assert_eq!(list.len(), 2);
        let found = list.iter().find(|c| c.id == "1").unwrap();
        assert_eq!((found.title.as_str(), found.favorite, found.size), ("ace", true, 1));
        assert_eq!(list.iter().find(|c| c.id == "b").unwrap().size, 2);
        delete(found);
        assert_eq!(scan(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

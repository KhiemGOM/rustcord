//! On-demand Windows game library scanner for Steam, Epic Games, Riot, and Xbox/PC Game Pass.
//! Runs only when explicitly invoked so startup and idle memory remain unaffected.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    Steam,
    Epic,
    Riot,
    Xbox,
}

impl StoreKind {
    pub fn label(self) -> &'static str {
        match self {
            StoreKind::Steam => "Steam",
            StoreKind::Epic => "Epic",
            StoreKind::Riot => "Riot",
            StoreKind::Xbox => "Xbox",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledGame {
    pub store: StoreKind,
    pub id: String,
    pub name: String,
    pub install_dir: Option<String>,
}

fn vdf_kv(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split('"');
    let _pre = parts.next()?;
    let k = parts.next()?;
    let _mid = parts.next()?;
    let v = parts.next()?;
    Some((k, v))
}

pub fn parse_steam_libraries(vdf: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in vdf.lines() {
        if let Some((k, v)) = vdf_kv(line.trim())
            && k.eq_ignore_ascii_case("path")
            && !v.is_empty()
        {
            out.push(v.replace("\\\\", "\\"));
        }
    }
    out
}

pub fn parse_steam_manifest(acf: &str) -> Option<InstalledGame> {
    let (mut id, mut name, mut dir) = (String::new(), String::new(), None);
    for line in acf.lines() {
        let Some((k, v)) = vdf_kv(line.trim()) else { continue };
        match k {
            "appid" => id = v.to_string(),
            "name" => name = v.to_string(),
            "installdir" if !v.is_empty() => dir = Some(v.to_string()),
            _ => {}
        }
    }
    if id.is_empty() || name.is_empty() || id == "228980" || name.starts_with("Proton ") || name.starts_with("Steam Linux Runtime") {
        return None;
    }
    Some(InstalledGame { store: StoreKind::Steam, id, name, install_dir: dir })
}

pub fn parse_epic_manifest(text: &str) -> Option<InstalledGame> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let app = v["AppName"].as_str()?.trim();
    let main = v["MainGameAppName"].as_str().unwrap_or(app).trim();
    if app.is_empty() || main != app {
        return None;
    }
    let name = v["DisplayName"].as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let dir = v["InstallLocation"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    Some(InstalledGame { store: StoreKind::Epic, id: app.to_string(), name: name.to_string(), install_dir: dir })
}

pub fn parse_riot_installs(text: &str) -> Vec<InstalledGame> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let mut out = Vec::new();
    for (path, _) in v["associated_client"].as_object().into_iter().flatten() {
        let trimmed = path.trim_end_matches(['\\', '/']);
        let name = trimmed.rsplit(['\\', '/']).next().unwrap_or("").trim();
        if name.is_empty() || name.eq_ignore_ascii_case("live") {
            let parent = trimmed.rsplit(['\\', '/']).nth(1).unwrap_or("").trim();
            if !parent.is_empty() {
                out.push(InstalledGame { store: StoreKind::Riot, id: parent.to_lowercase(), name: parent.to_string(), install_dir: Some(trimmed.to_string()) });
            }
        } else {
            out.push(InstalledGame { store: StoreKind::Riot, id: name.to_lowercase(), name: name.to_string(), install_dir: Some(trimmed.to_string()) });
        }
    }
    out
}

fn steam_root() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    let key = crate::winsys::wide("Software\\Valve\\Steam");
    let val = crate::winsys::wide("SteamPath");
    let mut buf = [0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
    if ok == 0 && len > 2 {
        let s = String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]);
        let p = PathBuf::from(s);
        if p.is_dir() {
            return Some(p);
        }
    }
    let fallback = PathBuf::from(r"C:\Program Files (x86)\Steam");
    fallback.is_dir().then_some(fallback)
}

fn scan_steam(out: &mut Vec<InstalledGame>) {
    let Some(root) = steam_root() else { return };
    let mut libs = vec![root.clone()];
    if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for p in parse_steam_libraries(&vdf) {
            let pb = PathBuf::from(p);
            if pb.is_dir() && !libs.contains(&pb) {
                libs.push(pb);
            }
        }
    }
    for lib in libs {
        let apps = lib.join("steamapps");
        let Ok(rd) = std::fs::read_dir(&apps) else { continue };
        for entry in rd.flatten() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if s.starts_with("appmanifest_") && s.ends_with(".acf")
                && let Ok(text) = std::fs::read_to_string(entry.path())
                && let Some(g) = parse_steam_manifest(&text)
            {
                out.push(g);
            }
        }
    }
}

fn scan_epic(out: &mut Vec<InstalledGame>) {
    let dir = Path::new(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        if entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("item"))
            && let Ok(text) = std::fs::read_to_string(entry.path())
            && let Some(g) = parse_epic_manifest(&text)
        {
            out.push(g);
        }
    }
}

fn scan_riot(out: &mut Vec<InstalledGame>) {
    let path = Path::new(r"C:\ProgramData\Riot Games\RiotClientInstalls.json");
    if let Ok(text) = std::fs::read_to_string(path) {
        out.extend(parse_riot_installs(&text));
    }
}

fn scan_xbox(out: &mut Vec<InstalledGame>) {
    let dir = Path::new(r"C:\XboxGames");
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("Content").join("MicrosoftGame.config").is_file() {
            let name = entry.file_name().to_string_lossy().into_owned();
            out.push(InstalledGame {
                store: StoreKind::Xbox,
                id: name.to_lowercase(),
                name,
                install_dir: Some(p.to_string_lossy().into_owned()),
            });
        }
    }
}

pub fn scan_installed() -> Vec<InstalledGame> {
    let mut out = Vec::new();
    scan_steam(&mut out);
    scan_epic(&mut out);
    scan_riot(&mut out);
    scan_xbox(&mut out);
    let mut seen = HashSet::new();
    out.retain(|g| seen.insert((g.store as u8, g.name.to_lowercase())));
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_steam_vdf_and_manifest_skipping_redistributables() {
        let vdf = r#"
"libraryfolders"
{
    "0"
    {
        "path" "C:\\Program Files (x86)\\Steam"
    }
    "1"
    {
        "path" "D:\\SteamLibrary"
    }
}
"#;
        assert_eq!(parse_steam_libraries(vdf), vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
        let acf = r#"
"AppState"
{
    "appid" "730"
    "name" "Counter-Strike 2"
    "installdir" "Counter-Strike Global Offensive"
}
"#;
        let g = parse_steam_manifest(acf).unwrap();
        assert_eq!(g.store, StoreKind::Steam);
        assert_eq!(g.id, "730");
        assert_eq!(g.name, "Counter-Strike 2");
        let redist = "\"appid\" \"228980\"\n\"name\" \"Steamworks Common Redistributables\"";
        assert!(parse_steam_manifest(redist).is_none());
    }

    #[test]
    fn parses_epic_and_riot_manifests() {
        let epic = r#"{ "AppName": "Fortnite", "MainGameAppName": "Fortnite", "DisplayName": "Fortnite", "InstallLocation": "C:\\Games\\Fortnite" }"#;
        let g = parse_epic_manifest(epic).unwrap();
        assert_eq!(g.store, StoreKind::Epic);
        assert_eq!(g.name, "Fortnite");
        let dlc = r#"{ "AppName": "FortnitePack", "MainGameAppName": "Fortnite", "DisplayName": "Pack" }"#;
        assert!(parse_epic_manifest(dlc).is_none());

        let riot = r#"{ "associated_client": { "C:/Riot Games/VALORANT/live/": "C:/Riot Games/Riot Client/RiotClientServices.exe" } }"#;
        let rg = parse_riot_installs(riot);
        assert_eq!(rg.len(), 1);
        assert_eq!(rg[0].name, "VALORANT");
    }
}

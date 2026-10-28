//! Where Vencord's code comes from: the source map Vencord's installer leaves next to its
//! build, which carries every plugin and the core they import, and the userplugins folders,
//! read straight off disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Sources {
    files: HashMap<String, String>,
}

const EXTS: [&str; 9] = ["", ".ts", ".tsx", ".js", ".jsx", "/index.ts", "/index.tsx", "/index.js", "/index.jsx"];

pub fn vencord_dir() -> PathBuf {
    std::env::var_os("FASTCORD_VENCORD").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Vencord"))
}

impl Sources {
    pub fn load(extra: &Path, vencord: &Path) -> Sources {
        let mut files = HashMap::new();
        let dist = vencord.join("dist");
        let map = ["vencordDesktopRenderer.js.map", "renderer.js.map"].iter().map(|f| dist.join(f)).find(|p| p.exists());
        if let Some(v) = map.and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) {
            let names = v["sources"].as_array().cloned().unwrap_or_default();
            let bodies = v["sourcesContent"].as_array().cloned().unwrap_or_default();
            for (n, b) in names.iter().zip(&bodies) {
                let (Some(n), Some(b)) = (n.as_str(), b.as_str()) else { continue };
                let n = n.trim_start_matches("../");
                if (n.starts_with("src/") || n.starts_with("packages/")) && !n.contains(':') && code(n) {
                    files.insert(n.to_string(), b.to_string());
                }
            }
        }
        for dir in [vencord.join("userplugins"), extra.to_path_buf()] {
            walk(&dir, &dir, &mut files);
        }
        Sources { files }
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    /// The canonical path a specifier names, from the module at `from`; `None` when nothing
    /// by that name exists here.
    pub fn resolve(&self, from: &str, spec: &str, shim: impl Fn(&str) -> bool) -> Option<String> {
        let spec = spec.split('?').next().unwrap_or(spec);
        let base = if spec.starts_with('.') {
            let mut parts: Vec<&str> = from.split('/').collect();
            parts.pop();
            for seg in spec.split('/') {
                match seg {
                    "." => {}
                    ".." => {
                        parts.pop();
                    }
                    s => parts.push(s),
                }
            }
            parts.join("/")
        } else if let Some(rest) = spec.strip_prefix('@') {
            let (scope, tail) = rest.split_once('/').unwrap_or((rest, ""));
            let dir = match scope {
                "api" | "components" | "utils" | "shared" | "plugins" | "main" | "webpack" => format!("src/{scope}"),
                "vencord" => return (tail == "discord-types/enums").then(|| "packages/discord-types/enums/index.ts".into()),
                _ => return None,
            };
            if tail.is_empty() { dir } else { format!("{dir}/{tail}") }
        } else {
            return shim(spec).then(|| spec.to_string());
        };
        if shim(&base) {
            return Some(base);
        }
        EXTS.iter().map(|e| format!("{base}{e}")).find(|p| self.files.contains_key(p))
    }

    /// Every plugin's entry file: official ones, then userplugins, by folder.
    pub fn plugins(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for root in ["src/plugins/", "src/userplugins/"] {
            let mut dirs: Vec<&str> = self
                .files
                .keys()
                .filter_map(|k| k.strip_prefix(root))
                .filter_map(|k| k.split_once('/').map(|(d, _)| d))
                .filter(|d| !d.starts_with('_'))
                .collect();
            dirs.sort_unstable();
            dirs.dedup();
            for d in dirs {
                if let Some(entry) = ["index.ts", "index.tsx", "index.js", "index.jsx"].iter().map(|e| format!("{root}{d}/{e}")).find(|p| self.files.contains_key(p)) {
                    out.push(entry);
                }
            }
        }
        out
    }
}

fn code(path: &str) -> bool {
    [".ts", ".tsx", ".js", ".jsx"].iter().any(|e| path.ends_with(e))
}

fn walk(root: &Path, dir: &Path, files: &mut HashMap<String, String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "node_modules") {
                walk(root, &p, files);
            }
        } else if code(&p.to_string_lossy())
            && let (Ok(rel), Ok(body)) = (p.strip_prefix(root), std::fs::read_to_string(&p))
        {
            let rel = rel.to_string_lossy().replace('\\', "/");
            files.insert(format!("src/userplugins/{rel}"), body);
        }
    }
}

#[cfg(test)]
mod tests {
    /// Every file of the installed Vencord goes through the transpiler. Needs Vencord installed,
    /// so it only runs when asked for.
    #[test]
    #[ignore]
    fn installed_vencord_transpiles() {
        let s = super::Sources::load(std::path::Path::new("."), &super::vencord_dir());
        let mut bad = Vec::new();
        for (k, v) in &s.files {
            if let Err(e) = super::super::transpile::transpile(k, v) {
                bad.push(e);
            }
        }
        assert!(s.files.len() > 300, "{}", s.files.len());
        assert!(bad.is_empty(), "{} of {}:
{}", bad.len(), s.files.len(), bad.join("
"));
    }
}

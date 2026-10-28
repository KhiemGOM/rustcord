//! The network side of Vencord's own settings pages: its updater (GitHub releases, written into
//! Vencord's dist folder as its own updater does), Vencord Cloud settings sync (api.vencord.dev,
//! same protocol and payload) and online themes.

use base64::Engine;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, Sender, channel};

use super::App;
use crate::net::Tag;

// what Vencord's updater replaces for Discord's install, and the maps Fastcord reads plugins from
const FILES: [&str; 8] = ["patcher.js", "preload.js", "renderer.js", "renderer.css", "patcher.js.map", "preload.js.map", "renderer.js.map", "renderer.css.map"];

#[derive(Default)]
pub struct VcNet {
    jobs: Option<(Sender<(&'static str, Result<Value, String>)>, Receiver<(&'static str, Result<Value, String>)>)>,
    // the latest release and the commits since the installed one, once checked
    pub updates: Option<Result<Value, String>>,
    pub busy: Option<&'static str>,
    // the check at startup, which applies what it finds when autoUpdate is on
    pub auto: bool,
    auto_applied: bool,
    // online themes by link, once fetched
    pub themes: std::collections::HashMap<String, String>,
    // Vencord Cloud's oauth app, while its authorize prompt is up
    pub oauth: Option<Value>,
}

fn send(tx: &Sender<(&'static str, Result<Value, String>)>, kind: &'static str, r: Result<Value, String>) {
    let _ = tx.send((kind, r));
}

async fn get_json(url: &str, headers: &[(&str, String)]) -> Result<(u16, reqwest::header::HeaderMap, Vec<u8>), String> {
    let mut req = crate::api::http().get(url);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let r = req.send().await.map_err(|e| e.to_string())?;
    let status = r.status().as_u16();
    let h = r.headers().clone();
    Ok((status, h, r.bytes().await.map_err(|e| e.to_string())?.to_vec()))
}

impl App {
    fn vc_job(&mut self, kind: &'static str, fut: impl std::future::Future<Output = Result<Value, String>> + Send + 'static) {
        let (tx, _) = self.vcnet.jobs.get_or_insert_with(channel);
        let tx = tx.clone();
        self.vcnet.busy = Some(kind);
        self.net.rt.spawn(async move { send(&tx, kind, fut.await) });
    }

    /// Finished jobs, each handled where it lands.
    pub(super) fn vcnet_tick(&mut self) {
        let done: Vec<_> = self.vcnet.jobs.as_ref().map(|(_, rx)| rx.try_iter().collect()).unwrap_or_default();
        for (kind, r) in done {
            self.vcnet.busy = None;
            match kind {
                "updates" => {
                    let outdated = matches!(&r, Ok(v) if v["outdated"] == true);
                    self.vcnet.updates = Some(r);
                    if std::mem::take(&mut self.vcnet.auto) && outdated {
                        self.vcnet.auto_applied = true;
                        self.vc_update();
                    }
                }
                "updated" => match r {
                    Ok(_) => {
                        if !std::mem::take(&mut self.vcnet.auto_applied) || self.vc.settings["autoUpdateNotification"] == true {
                            self.vc_note("Vencord Updater", "Updated! Fastcord's plugins reloaded; restart Discord to update Vencord there too.");
                        }
                        self.vcnet.updates = None;
                        self.vc_restart();
                    }
                    Err(e) => self.vc_note("Vencord Updater", &format!("Update failed: {e}")),
                },
                "theme" => {
                    if let Ok(v) = r {
                        self.vcnet.themes.insert(v["url"].as_str().unwrap_or("").to_string(), v["css"].as_str().unwrap_or("").to_string());
                        self.vc.restyle = true;
                    }
                }
                "oauth" => match r {
                    Ok(v) => self.vcnet.oauth = Some(v),
                    Err(e) => self.vc_note("Cloud Integration", &format!("Setup failed (couldn't retrieve OAuth configuration): {e}")),
                },
                "secret" => match r {
                    Ok(v) => {
                        let key = format!("{}:{}", self.vc_cloud_origin(), self.store.me);
                        let mut secrets = self.vc.data_values.get("Vencord_cloudSecret").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
                        secrets[key] = v["secret"].clone();
                        self.vc_call("dataSet", json!({ "key": "Vencord_cloudSecret", "value": secrets }));
                        self.vc.data_values.insert("Vencord_cloudSecret".into(), secrets);
                        self.vc_call("setPath", json!({ "path": ["cloud", "authenticated"], "value": true }));
                        self.vc_note("Cloud Integration", "Cloud integrations enabled!");
                    }
                    Err(e) => {
                        self.vc_call("setPath", json!({ "path": ["cloud", "authenticated"], "value": false }));
                        self.vc_note("Cloud Integration", &format!("Setup failed: {e}"));
                    }
                },
                "push" => match r {
                    Ok(v) => {
                        self.vc_call("setPath", json!({ "path": ["cloud", "settingsSyncVersion"], "value": v["written"] }));
                        if v["manual"] == true {
                            self.vc_note("Cloud Settings", "Synchronized settings to the cloud!");
                        }
                    }
                    Err(e) => self.vc_note("Cloud Settings", &format!("Could not synchronize settings to the cloud ({e}).")),
                },
                "pull" => match r {
                    Ok(v) if v["settings"].is_object() => {
                        let mut settings = v["settings"].clone();
                        settings["cloud"]["settingsSyncVersion"] = v["written"].clone();
                        self.vc_import(settings, v["quickCss"].as_str().unwrap_or(""));
                        self.vc_note("Cloud Settings", "Your settings have been updated!");
                    }
                    Ok(v) => self.vc_note("Cloud Settings", v["message"].as_str().unwrap_or("Your settings are up to date.")),
                    Err(e) => self.vc_note("Cloud Settings", &format!("Could not synchronize settings from the cloud ({e}).")),
                },
                "deleted" => self.vc_note("Cloud Settings", if r.is_ok() { "Settings deleted from cloud!" } else { "Could not delete settings." }),
                "erased" => match r {
                    Ok(_) => {
                        self.vc_cloud_forget();
                        self.vc_note("Cloud Integrations", "Cloud integrations have been disabled and your data erased.");
                    }
                    Err(e) => self.vc_note("Cloud Integrations", &format!("Could not erase all data ({e}), please contact support.")),
                },
                _ => {}
            }
        }
    }

    /// Compares the installed build with the latest release, listing the commits between.
    pub fn vc_check_updates(&mut self) {
        let (hash, remote) = self.vc.host.as_ref().map(|h| h.version()).unwrap_or_default();
        let remote = remote.unwrap_or_else(|| "Vendicated/Vencord".into());
        let hash = hash.unwrap_or_default();
        self.vc_job("updates", async move {
            let (status, _, body) = get_json(&format!("https://api.github.com/repos/{remote}/releases/latest"), &[]).await?;
            if status != 200 {
                return Err(format!("GitHub returned {status}"));
            }
            let rel: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
            let latest = rel["name"].as_str().and_then(|n| n.split_whitespace().last()).unwrap_or("").to_string();
            let mut commits = Vec::new();
            if !latest.is_empty() && !hash.is_empty() && !latest.starts_with(&hash) && !hash.starts_with(&latest) {
                let (status, _, body) = get_json(&format!("https://api.github.com/repos/{remote}/compare/{hash}...{latest}"), &[]).await?;
                if status == 200 {
                    let cmp: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
                    for c in cmp["commits"].as_array().into_iter().flatten().rev() {
                        let msg = c["commit"]["message"].as_str().unwrap_or("").lines().next().unwrap_or("").to_string();
                        commits.push(json!({ "hash": &c["sha"].as_str().unwrap_or("")[..7.min(c["sha"].as_str().unwrap_or("").len())], "message": msg, "author": c["commit"]["author"]["name"] }));
                    }
                }
            }
            let assets: Vec<Value> = rel["assets"].as_array().cloned().unwrap_or_default();
            Ok(json!({ "latest": latest, "commits": commits, "assets": assets, "outdated": !commits.is_empty() || (!latest.is_empty() && !latest.starts_with(&hash)) }))
        });
    }

    /// Vencord's update: the release's files into its dist folder, each written whole or not at all.
    pub fn vc_update(&mut self) {
        let Some(Ok(info)) = &self.vcnet.updates else { return };
        let assets: Vec<(String, String)> = info["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| Some((a["name"].as_str()?.to_string(), a["browser_download_url"].as_str()?.to_string())))
            .filter(|(n, _)| FILES.contains(&n.as_str()))
            .collect();
        let dist = crate::vencord::vencord_dir().join("dist");
        self.vc_job("updated", async move {
            if assets.is_empty() {
                return Err("the release has none of Vencord's files".into());
            }
            for (name, url) in assets {
                let (status, _, body) = get_json(&url, &[]).await?;
                if status != 200 {
                    return Err(format!("{name}: GitHub returned {status}"));
                }
                let tmp = dist.join(format!("{name}.fastcord-new"));
                std::fs::write(&tmp, &body).map_err(|e| e.to_string())?;
                std::fs::rename(&tmp, dist.join(&name)).map_err(|e| e.to_string())?;
            }
            Ok(Value::Null)
        });
    }

    pub fn vc_fetch_theme(&mut self, url: String) {
        self.vcnet.themes.insert(url.clone(), String::new());
        self.vc_job("theme", async move {
            let (status, _, body) = get_json(&url, &[]).await?;
            if status != 200 {
                return Err(format!("{status}"));
            }
            Ok(json!({ "url": url, "css": String::from_utf8_lossy(&body) }))
        });
    }

    pub fn vc_cloud_url(&self) -> String {
        self.vc.settings["cloud"]["url"].as_str().unwrap_or("https://api.vencord.dev/").to_string()
    }

    fn vc_cloud_origin(&self) -> String {
        let u = self.vc_cloud_url();
        let rest = u.split_once("://").map_or(u.as_str(), |(_, r)| r);
        let host = rest.split('/').next().unwrap_or("");
        format!("{}://{host}", u.split("://").next().unwrap_or("https"))
    }

    fn vc_cloud_auth(&self) -> Option<String> {
        let key = format!("{}:{}", self.vc_cloud_origin(), self.store.me);
        let secret = self.vc.data_values.get("Vencord_cloudSecret")?[key].as_str()?.to_string();
        Some(base64::engine::general_purpose::STANDARD.encode(format!("{secret}:{}", self.store.me)))
    }

    fn vc_endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.vc_cloud_url().trim_end_matches('/'), path.trim_start_matches('/'))
    }

    /// Turning Cloud Integrations on: Vencord Cloud's oauth app, then Discord's authorize prompt.
    pub fn vc_cloud_setup(&mut self) {
        if self.vc_cloud_auth().is_some() {
            self.vc_call("setPath", json!({ "path": ["cloud", "authenticated"], "value": true }));
            return;
        }
        let url = self.vc_endpoint("/v1/oauth/settings");
        self.vc_job("oauth", async move {
            let (status, _, body) = get_json(&url, &[]).await?;
            if status != 200 {
                return Err(format!("{status}"));
            }
            serde_json::from_slice(&body).map_err(|e| e.to_string())
        });
    }

    /// The authorize prompt's Authorize: discord makes the code, Vencord Cloud trades it for a secret.
    pub fn vc_cloud_authorize(&mut self) {
        let Some(o) = self.vcnet.oauth.take() else { return };
        let (client, redirect) = (o["clientId"].as_str().unwrap_or(""), o["redirectUri"].as_str().unwrap_or(""));
        let enc = |s: &str| s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
        let path = format!("/oauth2/authorize?client_id={}&response_type=code&redirect_uri={}&scope=identify", enc(client), enc(redirect));
        self.rest("POST", path, Some(json!({ "permissions": "0", "authorize": true, "integration_type": 0 })), Tag::Remote("vc_oauth"));
    }

    pub(super) fn vc_oauth_done(&mut self, r: Result<Value, String>) {
        let Some(location) = r.ok().and_then(|v| v["location"].as_str().map(str::to_string)) else {
            self.vc_call("setPath", json!({ "path": ["cloud", "authenticated"], "value": false }));
            return self.vc_note("Cloud Integration", "Setup failed (Discord didn't authorize it).");
        };
        self.vc_job("secret", async move {
            let (status, _, body) = get_json(&location, &[("Accept", "application/json".into())]).await?;
            let v: Value = serde_json::from_slice(&body).map_err(|e| format!("{status}: {e}"))?;
            v["secret"].as_str().ok_or_else(|| "no secret returned".to_string())?;
            Ok(v)
        });
    }

    pub fn vc_cloud_forget(&mut self) {
        let key = format!("{}:{}", self.vc_cloud_origin(), self.store.me);
        if let Some(Value::Object(m)) = self.vc.data_values.get_mut("Vencord_cloudSecret") {
            m.remove(&key);
            let v = Value::Object(m.clone());
            self.vc_call("dataSet", json!({ "key": "Vencord_cloudSecret", "value": v }));
        }
        self.vc_call("setPath", json!({ "path": ["cloud", "authenticated"], "value": false }));
    }

    /// Vencord's export: settings and QuickCSS, minified and raw-deflated for the cloud.
    pub fn vc_export(&self, pretty: bool) -> String {
        let v = json!({ "settings": self.vc.settings, "quickCss": super::vencord::quick_css() });
        if !pretty {
            return v.to_string();
        }
        let mut out = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut out, serde_json::ser::PrettyFormatter::with_indent(b"    "));
        serde::Serialize::serialize(&v, &mut ser).ok();
        String::from_utf8(out).unwrap_or_default()
    }

    pub fn vc_cloud_push(&mut self, manual: bool) {
        let Some(auth) = self.vc_cloud_auth() else { return };
        let url = self.vc_endpoint("/v1/settings");
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = z.write_all(self.vc_export(false).as_bytes());
        let body = z.finish().unwrap_or_default();
        self.vc_job("push", async move {
            let r = crate::api::http().put(&url).header("Authorization", auth).header("Content-Type", "application/octet-stream").body(body).send().await.map_err(|e| e.to_string())?;
            let status = r.status().as_u16();
            if !(200..300).contains(&status) {
                return Err(format!("API returned {status}"));
            }
            let v: Value = r.json().await.map_err(|e| e.to_string())?;
            Ok(json!({ "written": v["written"], "manual": manual }))
        });
    }

    pub fn vc_cloud_pull(&mut self, force: bool) {
        let Some(auth) = self.vc_cloud_auth() else { return };
        let url = self.vc_endpoint("/v1/settings");
        let local = self.vc.settings["cloud"]["settingsSyncVersion"].as_i64().unwrap_or(0);
        self.vc_job("pull", async move {
            let (status, h, body) = get_json(&url, &[("Authorization", auth), ("Accept", "application/octet-stream".into()), ("If-None-Match", local.to_string())]).await?;
            match status {
                404 => return Ok(json!({ "message": "There are no settings in the cloud." })),
                304 => return Ok(json!({ "message": "Your settings are up to date." })),
                s if !(200..300).contains(&s) => return Err(format!("API returned {s}")),
                _ => {}
            }
            let written: i64 = h.get("etag").and_then(|v| v.to_str().ok()).and_then(|v| v.trim_matches('"').parse().ok()).unwrap_or(0);
            if !force && written < local {
                return Ok(json!({ "message": "Your local settings are newer than the cloud ones." }));
            }
            let mut text = String::new();
            flate2::read::DeflateDecoder::new(&body[..]).read_to_string(&mut text).map_err(|e| e.to_string())?;
            let mut v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            v["written"] = written.into();
            Ok(v)
        });
    }

    pub fn vc_cloud_delete(&mut self, everything: bool) {
        let Some(auth) = self.vc_cloud_auth() else { return };
        let url = self.vc_endpoint(if everything { "/v1/" } else { "/v1/settings" });
        self.vc_job(if everything { "erased" } else { "deleted" }, async move {
            let r = crate::api::http().delete(&url).header("Authorization", auth).send().await.map_err(|e| e.to_string())?;
            let s = r.status().as_u16();
            if (200..300).contains(&s) { Ok(Value::Null) } else { Err(format!("API returned {s}")) }
        });
    }
}

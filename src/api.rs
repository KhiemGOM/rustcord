use base64::Engine;
use reqwest::{Method, StatusCode, multipart};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use crate::state::{Id, Page};

pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) discord/1.0.9205 Chrome/134.0.6998.205 Electron/35.3.0 Safari/537.36";

fn env_or(key: &str, def: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| def.to_string())
}

static LOCALE: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

pub fn set_locale(l: &str) {
    if let Ok(mut s) = LOCALE.write() {
        *s = l.to_string();
    }
}

fn locale() -> String {
    LOCALE.read().ok().map(|s| s.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "en-US".into())
}

pub fn base() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| env_or("RUSTCORD_API", "https://discord.com/api/v9"))
}

pub fn cdn() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| env_or("RUSTCORD_CDN", "https://cdn.discordapp.com"))
}

pub fn emoji_cdn() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| env_or("RUSTCORD_EMOJI_CDN", "https://cdn.jsdelivr.net/gh/jdecked/twemoji@latest/assets/72x72"))
}

pub fn gateway() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| env_or("RUSTCORD_GATEWAY", "wss://gateway.discord.gg"))
}

pub fn props() -> Value {
    json!({
        "os": "Windows",
        "browser": "Discord Client",
        "release_channel": "stable",
        "client_version": "1.0.9205",
        "os_version": "10.0.26200",
        "os_arch": "x64",
        "app_arch": "x64",
        "system_locale": "en-US",
        "has_client_mods": false,
        "browser_user_agent": UA,
        "browser_version": "35.3.0",
        "client_build_number": 455000,
        "native_build_number": 66000,
        "client_event_source": null
    })
}

fn super_props() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| base64::engine::general_purpose::STANDARD.encode(props().to_string()))
}

/// Whether an error means discord couldn't be reached at all, rather than that it said no.
pub fn offline(err: &str) -> bool {
    err.starts_with("error sending request")
}

pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(UA)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("http client")
}

fn esc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub struct Api {
    http: reqwest::Client,
    token: String,
}

impl Api {
    pub fn new(http: reqwest::Client, token: String) -> Self {
        Api { http, token }
    }

    fn build(&self, m: Method, path: &str) -> reqwest::RequestBuilder {
        let mut r = self
            .http
            .request(m, format!("{}{}", base(), path))
            .header("X-Super-Properties", super_props())
            .header("X-Discord-Locale", locale())
            .header("X-Discord-Timezone", "UTC");
        if !self.token.is_empty() {
            r = r.header("Authorization", &self.token);
        }
        r
    }

    pub async fn call(&self, m: Method, path: &str, body: Option<Value>) -> Result<Value, String> {
        for _ in 0..4 {
            let mut req = self.build(m.clone(), path);
            if let Some(b) = &body {
                req = req.json(b);
            }
            let res = req.send().await.map_err(|e| e.to_string())?;
            let status = res.status();
            if status == StatusCode::TOO_MANY_REQUESTS {
                let v: Value = res.json().await.unwrap_or_default();
                let wait = v["retry_after"].as_f64().unwrap_or(1.0);
                tokio::time::sleep(Duration::from_secs_f64(wait.min(30.0))).await;
                continue;
            }
            if status == StatusCode::NO_CONTENT {
                return Ok(Value::Null);
            }
            let v: Value = res.json().await.unwrap_or(Value::Null);
            if status.is_success() {
                return Ok(v);
            }
            return Err(v["message"].as_str().map(str::to_string).unwrap_or_else(|| status.to_string()));
        }
        Err("rate limited".into())
    }

    pub async fn messages(&self, ch: Id, page: Page) -> Result<Value, String> {
        let mut p = format!("/channels/{ch}/messages?limit=50");
        match page {
            Page::Latest => {}
            Page::Before(id) => p += &format!("&before={id}"),
            Page::After(id) => p += &format!("&after={id}"),
            Page::Around(id) => p += &format!("&around={id}"),
        }
        self.call(Method::GET, &p, None).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send(
        &self,
        ch: Id,
        content: &str,
        nonce: &str,
        reply: Option<(Id, bool)>,
        files: &[(PathBuf, String)],
        stickers: &[Id],
        tts: bool,
    ) -> Result<Value, String> {
        let mut body = json!({ "content": content, "nonce": nonce, "tts": tts, "flags": 0 });
        if !stickers.is_empty() {
            body["sticker_ids"] = json!(stickers.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        }
        if let Some((r, ping)) = reply {
            body["message_reference"] = json!({ "message_id": r.to_string(), "channel_id": ch.to_string() });
            body["allowed_mentions"] = json!({ "parse": ["users", "roles", "everyone"], "replied_user": ping });
        }
        let path = format!("/channels/{ch}/messages");
        if files.is_empty() {
            return self.call(Method::POST, &path, Some(body)).await;
        }
        let mut atts = Vec::new();
        let mut form = multipart::Form::new();
        for (i, (f, name)) in files.iter().enumerate() {
            let bytes = tokio::fs::read(f).await.map_err(|e| e.to_string())?;
            let name = name.clone();
            atts.push(json!({ "id": i, "filename": name }));
            form = form.part(format!("files[{i}]"), multipart::Part::bytes(bytes).file_name(name));
        }
        body["attachments"] = Value::Array(atts);
        form = form.text("payload_json", body.to_string());
        let res = self
            .build(Method::POST, &path)
            .multipart(form)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let ok = res.status().is_success();
        let v: Value = res.json().await.unwrap_or_default();
        if ok { Ok(v) } else { Err(v["message"].as_str().unwrap_or("upload failed").to_string()) }
    }

    pub async fn sticker(&self, gid: Id, fields: [(&'static str, String); 3], file: PathBuf) -> Result<Value, String> {
        let bytes = tokio::fs::read(&file).await.map_err(|e| e.to_string())?;
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mime = match file.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
            Some("gif") => "image/gif",
            Some("json") => "application/json",
            _ => "image/png",
        };
        let mut form = multipart::Form::new();
        for (k, v) in fields {
            form = form.text(k, v);
        }
        form = form.part("file", multipart::Part::bytes(bytes).file_name(name).mime_str(mime).map_err(|e| e.to_string())?);
        let res = self.build(Method::POST, &format!("/guilds/{gid}/stickers")).multipart(form).send().await.map_err(|e| e.to_string())?;
        let ok = res.status().is_success();
        let v: Value = res.json().await.unwrap_or_default();
        if ok { Ok(v) } else { Err(v["message"].as_str().unwrap_or("upload failed").to_string()) }
    }

    // the official client posts interactions as multipart with a payload_json part
    pub async fn interact(&self, payload: Value) -> Result<Value, String> {
        let form = multipart::Form::new().text("payload_json", payload.to_string());
        let res = self.build(Method::POST, "/interactions").multipart(form).send().await.map_err(|e| e.to_string())?;
        if res.status().is_success() {
            return Ok(Value::Null);
        }
        let v: Value = res.json().await.unwrap_or_default();
        Err(v["message"].as_str().unwrap_or("the application did not respond").to_string())
    }

    pub async fn edit(&self, ch: Id, id: Id, content: &str) -> Result<Value, String> {
        self.call(Method::PATCH, &format!("/channels/{ch}/messages/{id}"), Some(json!({ "content": content })))
            .await
    }

    pub async fn delete(&self, ch: Id, id: Id) -> Result<Value, String> {
        self.call(Method::DELETE, &format!("/channels/{ch}/messages/{id}"), None).await
    }

    pub async fn react(&self, ch: Id, id: Id, emoji: &str, add: bool) -> Result<Value, String> {
        let p = format!("/channels/{ch}/messages/{id}/reactions/{}/@me?location=Message&type=0", esc(emoji));
        self.call(if add { Method::PUT } else { Method::DELETE }, &p, None).await
    }

    pub async fn ack(&self, ch: Id, id: Id) -> Result<Value, String> {
        self.call(Method::POST, &format!("/channels/{ch}/messages/{id}/ack"), Some(json!({ "token": null })))
            .await
    }

    pub async fn typing(&self, ch: Id) -> Result<Value, String> {
        self.call(Method::POST, &format!("/channels/{ch}/typing"), None).await
    }

    // auth endpoints put the interesting bits (captcha, mfa ticket) in error bodies
    async fn raw(&self, path: &str, body: Value) -> Result<Value, String> {
        let res = self.build(Method::POST, path).json(&body).send().await.map_err(|e| e.to_string())?;
        Ok(res.json().await.unwrap_or_default())
    }

    pub async fn login(&self, login: &str, password: &str) -> Result<Value, String> {
        self.raw("/auth/login", json!({ "login": login, "password": password, "undelete": false })).await
    }

    pub async fn mfa(&self, ticket: &str, code: &str) -> Result<Value, String> {
        self.raw("/auth/mfa/totp", json!({ "code": code, "ticket": ticket })).await
    }
}

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

pub fn remote_auth() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| env_or("RUSTCORD_REMOTE_AUTH", "wss://remote-auth-gateway.discord.gg"))
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

pub fn esc(s: &str) -> String {
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

pub struct Reply {
    pub status: u16,
    pub body: Value,
}

pub struct Captcha {
    pub key: String,
    pub rqtoken: Option<String>,
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
        self.call_mfa(m, path, body, None).await
    }

    /// `call` for the requests discord guards with a second factor: the token from /mfa/finish
    /// rides in X-Discord-MFA-Authorization, and a 60003 answer comes back as `mfa:<ticket and methods>`.
    pub async fn call_mfa(&self, m: Method, path: &str, body: Option<Value>, mfa: Option<&str>) -> Result<Value, String> {
        for _ in 0..4 {
            let mut req = self.build(m.clone(), path);
            if let Some(t) = mfa {
                req = req.header("X-Discord-MFA-Authorization", t);
            }
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
            if v["code"].as_u64() == Some(60003) && v["mfa"].is_object() {
                return Err(format!("mfa:{}", v["mfa"]));
            }
            return Err(v["message"].as_str().map(str::to_string).unwrap_or_else(|| status.to_string()));
        }
        Err("rate limited".into())
    }

    pub async fn bearer(&self, path: &str, token: &str) -> Result<Value, String> {
        let res = self.http.get(format!("{}{}", base(), path)).header("Authorization", format!("Bearer {token}")).send().await.map_err(|e| e.to_string())?;
        let status = res.status();
        let v: Value = res.json().await.unwrap_or(Value::Null);
        if status.is_success() { Ok(v) } else { Err(v["message"].as_str().map(str::to_string).unwrap_or_else(|| status.to_string())) }
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
        files: &[(PathBuf, String, String)],
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
        let job = crate::upload::get(nonce);
        let stopped = || job.as_ref().is_some_and(|j| j.cancelled());
        let mut asks = Vec::new();
        let mut atts = Vec::new();
        for (i, (f, name, alt)) in files.iter().enumerate() {
            let size = tokio::fs::metadata(f).await.map_err(|e| e.to_string())?.len();
            let mut att = json!({ "id": i, "filename": name });
            if !alt.is_empty() {
                att["description"] = json!(alt);
            }
            let mut ask = json!({ "filename": name, "file_size": size, "id": i.to_string(), "is_clip": false, "original_content_type": crate::upload::mime(name) });
            if let Some(c) = crate::clips::library::read(f) {
                ask["is_clip"] = json!(true);
                att["is_clip"] = json!(true);
                if !c.title.is_empty() {
                    att["title"] = json!(c.title);
                }
                att["clip_created_at"] = json!(chrono::DateTime::from_timestamp_millis(c.created).map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)));
                att["clip_participant_ids"] = json!(c.users.iter().map(|u| u.to_string()).collect::<Vec<_>>());
            }
            asks.push(ask);
            atts.push(att);
        }
        let slots = self.call(Method::POST, &format!("/channels/{ch}/attachments"), Some(json!({ "files": asks }))).await?;
        let slots = slots["attachments"].as_array().ok_or("upload refused")?;
        for (i, (f, name, _)) in files.iter().enumerate() {
            if stopped() {
                return Err("cancelled".into());
            }
            let slot = slots.iter().find(|s| s["id"].as_u64().or_else(|| s["id"].as_str().and_then(|x| x.parse().ok())) == Some(i as u64)).ok_or("upload refused")?;
            let url = slot["upload_url"].as_str().ok_or("upload refused")?;
            self.put_file(url, f, name, job.as_ref()).await?;
            atts[i]["uploaded_filename"] = slot["upload_filename"].clone();
        }
        body["attachments"] = Value::Array(atts);
        self.call(Method::POST, &path, Some(body)).await
    }

    /// One file to the storage url discord handed out, read off the disk as it goes so a big one
    /// costs no memory, counting what's gone for the progress bar and stopping when cancelled.
    async fn put_file(&self, url: &str, file: &std::path::Path, name: &str, job: Option<&std::sync::Arc<crate::upload::Job>>) -> Result<(), String> {
        use tokio::io::AsyncReadExt;
        let f = tokio::fs::File::open(file).await.map_err(|e| e.to_string())?;
        let len = f.metadata().await.map_err(|e| e.to_string())?.len();
        let job = job.cloned();
        let seen = job.clone();
        let body = reqwest::Body::wrap_stream(futures_util::stream::unfold((f, job), |(mut f, job)| async move {
            if job.as_ref().is_some_and(|j| j.cancelled()) {
                return Some((Err(std::io::Error::other("cancelled")), (f, job)));
            }
            let mut buf = vec![0u8; 64 << 10];
            match f.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    buf.truncate(n);
                    if let Some(j) = &job {
                        j.sent.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                    }
                    Some((Ok(buf), (f, job)))
                }
                Err(e) => Some((Err(e), (f, job))),
            }
        }));
        let res = self.http.put(url).header("Content-Length", len).header("Content-Type", crate::upload::mime(name)).body(body).send().await;
        if seen.as_ref().is_some_and(|j| j.cancelled()) {
            return Err("cancelled".into());
        }
        let res = res.map_err(|e| e.to_string())?;
        if res.status().is_success() { Ok(()) } else { Err(format!("upload failed ({})", res.status())) }
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

    async fn auth_post(&self, path: &str, mut body: Value, cap: Option<Captcha>) -> Result<Reply, String> {
        let mut req = self.build(Method::POST, path);
        if let Some(c) = cap {
            body["captcha_key"] = json!(c.key);
            if let Some(rq) = c.rqtoken {
                req = req.header("X-Captcha-Rqtoken", rq);
            }
        }
        let res = req.json(&body).send().await.map_err(|e| e.to_string())?;
        let status = res.status().as_u16();
        Ok(Reply { status, body: res.json().await.unwrap_or(Value::Null) })
    }

    pub async fn login(&self, login: &str, password: &str, undelete: bool, cap: Option<Captcha>) -> Result<Reply, String> {
        self.auth_post("/auth/login", json!({ "login": login, "password": password, "undelete": undelete }), cap).await
    }

    pub async fn mfa(&self, kind: &str, ticket: &str, code: &str) -> Result<Reply, String> {
        self.auth_post(&format!("/auth/mfa/{kind}"), json!({ "code": code, "ticket": ticket }), None).await
    }

    pub async fn sms(&self, ticket: &str) -> Result<Reply, String> {
        self.auth_post("/auth/mfa/sms/send", json!({ "ticket": ticket }), None).await
    }

    pub async fn register(&self, body: &Value, cap: Option<Captcha>) -> Result<Reply, String> {
        self.auth_post("/auth/register", body.clone(), cap).await
    }

    pub async fn username_taken(&self, name: &str) -> Result<Reply, String> {
        self.auth_post("/unique-username/username-attempt-unauthed", json!({ "username": name }), None).await
    }

    pub async fn forgot(&self, login: &str, cap: Option<Captcha>) -> Result<Reply, String> {
        self.auth_post("/auth/forgot", json!({ "login": login }), cap).await
    }

    pub async fn remote_login(&self, ticket: &str) -> Result<Reply, String> {
        self.auth_post("/users/@me/remote-auth/login", json!({ "ticket": ticket }), None).await
    }
}

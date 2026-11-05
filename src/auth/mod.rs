mod captcha;
mod remote;
mod rsa;

use serde_json::{Value, json};
use std::future::Future;
use std::sync::Arc;

use crate::api::{Api, Captcha, Reply};
use crate::net::{Ev as NetEv, Tx};

pub use captcha::solve_captcha;
pub use remote::run as qr_login;

#[derive(Clone, Default)]
pub struct Challenge {
    pub ticket: String,
    pub totp: bool,
    pub sms: bool,
    pub backup: bool,
    pub webauthn: Option<String>,
}

pub struct Register {
    pub email: String,
    pub global_name: String,
    pub username: String,
    pub password: String,
    pub birthday: String,
    pub consent: bool,
    pub promo: bool,
}

pub enum Cmd {
    Login { login: String, password: String, undelete: bool },
    Mfa { kind: &'static str, ticket: String, code: String },
    Sms(String),
    Register(Register),
    Name(String),
    Forgot(String),
}

pub enum Fail {
    Fields(Vec<(String, String)>),
    Msg(String),
    Disabled(String),
    Deletion,
}

#[derive(Default)]
pub enum Qr {
    #[default]
    Init,
    Ready(String),
    Ticket { id: String, avatar: Option<String>, name: String },
    Working,
}

pub enum Ev {
    Token(String),
    Mfa(Challenge),
    Fail(Fail),
    Captcha,
    Qr(Qr),
    Sms(Result<String, String>),
    Name { name: String, taken: bool },
    Forgot,
}

pub fn webauthn_ready() -> bool {
    false
}

pub fn webauthn_assert(_challenge: &str) -> Result<String, String> {
    Err("Security keys aren't available yet.".into())
}

fn messages(v: &Value, field: &str, out: &mut Vec<(String, String, String)>) {
    let Some(o) = v.as_object() else { return };
    if let Some(list) = o.get("_errors").and_then(|e| e.as_array()) {
        let first = &list[0];
        out.push((field.to_string(), first["code"].as_str().unwrap_or("").to_string(), first["message"].as_str().unwrap_or("").to_string()));
    }
    for (k, child) in o {
        if k != "_errors" {
            messages(child, k, out);
        }
    }
}

fn fail(r: &Reply) -> Fail {
    let b = &r.body;
    match b["code"].as_u64() {
        Some(20013) => return Fail::Disabled(b["message"].as_str().unwrap_or("").to_string()),
        Some(20011) => return Fail::Deletion,
        _ => {}
    }
    let mut found = Vec::new();
    messages(&b["errors"], "", &mut found);
    if !found.is_empty() {
        return Fail::Fields(found.into_iter().map(|(f, _, m)| (f, m)).collect());
    }
    match b["message"].as_str() {
        Some(m) if r.status == 429 => Fail::Msg(format!("{m} Try again in a moment.")),
        Some(m) => Fail::Msg(m.to_string()),
        None => Fail::Msg("An error occurred. Please try again.".into()),
    }
}

async fn solving<F, Fut>(tx: &Tx, mut send: F) -> Result<Reply, String>
where
    F: FnMut(Option<Captcha>) -> Fut,
    Fut: Future<Output = Result<Reply, String>>,
{
    let mut cap = None;
    for _ in 0..3 {
        let r = send(cap.take()).await?;
        let (Some(sitekey), true) = (r.body["captcha_sitekey"].as_str(), r.body["captcha_key"].is_array()) else { return Ok(r) };
        tx.send(NetEv::Auth(Ev::Captcha));
        let (sitekey, rqdata) = (sitekey.to_string(), r.body["captcha_rqdata"].as_str().map(str::to_string));
        let key = tokio::task::spawn_blocking(move || solve_captcha(&sitekey, rqdata.as_deref())).await.ok().flatten();
        let Some(key) = key else { return Err("The captcha wasn't completed. Try again.".into()) };
        cap = Some(Captcha { key, rqtoken: r.body["captcha_rqtoken"].as_str().map(str::to_string) });
    }
    Err("The captcha failed. Please try again.".into())
}

fn reply(tx: &Tx, r: Result<Reply, String>, ok: impl FnOnce(&Reply) -> Option<Ev>) {
    let ev = match r {
        Ok(r) => ok(&r).unwrap_or_else(|| Ev::Fail(fail(&r))),
        Err(e) => Ev::Fail(Fail::Msg(e)),
    };
    tx.send(NetEv::Auth(ev));
}

fn token(r: &Reply) -> Option<Ev> {
    r.body["token"].as_str().map(|t| Ev::Token(t.to_string()))
}

pub async fn run(cmd: Cmd, api: Arc<Api>, tx: Tx) {
    match cmd {
        Cmd::Login { login, password, undelete } => {
            let r = solving(&tx, |cap| api.login(&login, &password, undelete, cap)).await;
            reply(&tx, r, |r| {
                if r.body["mfa"].as_bool() == Some(true) {
                    let on = |k: &str| r.body[k].as_bool() == Some(true);
                    return Some(Ev::Mfa(Challenge {
                        ticket: r.body["ticket"].as_str().unwrap_or("").to_string(),
                        totp: on("totp"),
                        sms: on("sms"),
                        backup: on("backup"),
                        webauthn: r.body["webauthn"].as_str().map(str::to_string),
                    }));
                }
                token(r)
            });
        }
        Cmd::Mfa { kind, ticket, code } => reply(&tx, api.mfa(kind, &ticket, &code).await, token),
        Cmd::Sms(ticket) => {
            let r = api.sms(&ticket).await;
            let ev = match r {
                Ok(r) if r.status < 300 => Ev::Sms(Ok(r.body["phone"].as_str().unwrap_or("").to_string())),
                Ok(r) => Ev::Sms(Err(r.body["message"].as_str().unwrap_or("Couldn't send the code.").to_string())),
                Err(e) => Ev::Sms(Err(e)),
            };
            tx.send(NetEv::Auth(ev));
        }
        Cmd::Register(form) => {
            let body = json!({
                "email": form.email, "username": form.username, "global_name": form.global_name, "password": form.password,
                "consent": form.consent, "date_of_birth": form.birthday, "promotional_email_opt_in": form.promo,
            });
            let r = solving(&tx, |cap| api.register(&body, cap)).await;
            reply(&tx, r, token);
        }
        Cmd::Name(name) => {
            if let Ok(r) = api.username_taken(&name).await {
                tx.send(NetEv::Auth(Ev::Name { name, taken: r.body["taken"].as_bool() == Some(true) }));
            }
        }
        Cmd::Forgot(login) => {
            let r = solving(&tx, |cap| api.forgot(&login, cap)).await;
            reply(&tx, r, |r| (r.status < 300).then_some(Ev::Forgot));
        }
    }
}

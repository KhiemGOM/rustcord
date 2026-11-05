//! AUTHORIZE and AUTHENTICATE. A game gets a code or a token only after the person says yes in
//! the window, for scopes Rustcord has a line of text for, and only for the application it
//! connected as. A denial is final for that connection.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio::time::timeout;

use super::{Client, Fail};
use crate::rpc::{Msg, call, session};

pub const SCOPES: [(&str, &str); 4] = [
    ("identify", "Access your username, avatar, and banner"),
    ("email", "Access your email address"),
    ("connections", "Access your third-party connections"),
    ("guilds", "Access your server and channel membership details"),
];

static ASKING: AtomicBool = AtomicBool::new(false);

struct Asking;

impl Asking {
    fn take() -> Option<Self> {
        (!ASKING.swap(true, Relaxed)).then_some(Asking)
    }
}

impl Drop for Asking {
    fn drop(&mut self) {
        ASKING.store(false, Relaxed);
    }
}

fn denied() -> Fail {
    (5000, "OAuth2 Error: access_denied: The resource owner or authorization server denied the request.".into())
}

fn query(client: &str, kind: &str, scopes: &[String]) -> String {
    let mut u = reqwest::Url::parse("http://x/").unwrap();
    u.query_pairs_mut().append_pair("client_id", client).append_pair("response_type", kind).append_pair("scope", &scopes.join(" ")).append_pair("integration_type", "0");
    u.query().unwrap_or("").to_string()
}

fn scopes_of(v: &Value) -> Result<Vec<String>, Fail> {
    let list: Vec<String> = match v {
        Value::String(s) => s.split(' ').filter(|s| !s.is_empty()).map(str::to_string).collect(),
        Value::Array(a) => a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect(),
        _ => vec![],
    };
    if list.is_empty() {
        return Err((5000, "OAuth2 Error: invalid_scope: No scopes provided".into()));
    }
    match list.iter().find(|s| !SCOPES.iter().any(|(n, _)| n == s)) {
        Some(s) => Err((5000, format!("OAuth2 Error: invalid_scope: Rustcord doesn't grant the {s} scope"))),
        None => Ok(list),
    }
}

impl Client {
    pub async fn authorize(&mut self, args: &Value) -> Result<Value, Fail> {
        let Some(cid) = args["client_id"].as_str().filter(|c| !c.is_empty()) else { return Err((4007, "No client id provided".into())) };
        if self.authed {
            return Err((4002, "Already authenticated".into()));
        }
        if cid.parse() != Ok(self.app) {
            return Err((4007, "Application does not match the connection's".into()));
        }
        if args["response_type"].as_str().is_some_and(|t| t != "code") {
            return Err((4002, "Authorization response_type \"token\" is not supported".into()));
        }
        if !args["redirect_uri"].is_null() {
            return Err((5000, "Redirect URI cannot be used in the RPC OAuth2 Authorization flow".into()));
        }
        let scopes = scopes_of(if args["scopes"].is_null() { &args["scope"] } else { &args["scopes"] })?;
        let location = self.consent("code", scopes).await?;
        let code = reqwest::Url::parse(&location).ok().and_then(|u| u.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.into_owned()));
        code.map(|c| json!({ "code": c })).ok_or((5000, "OAuth2 Error: Unable to find auth code".into()))
    }

    pub async fn authenticate(&mut self, args: &Value) -> Result<Value, Fail> {
        if self.authed {
            return Err((4002, "Already authenticated".into()));
        }
        let token = match args["access_token"].as_str() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => {
                let location = self.consent("token", vec!["identify".into()]).await?;
                let frag = reqwest::Url::parse(&location).ok().and_then(|u| u.fragment().map(str::to_string)).unwrap_or_default();
                let frag = reqwest::Url::parse(&format!("http://x/?{frag}")).map_err(|_| (4009, "No access token provided".to_string()))?;
                frag.query_pairs().find(|(k, _)| k == "access_token").map(|(_, v)| v.into_owned()).ok_or((4009, "No access token provided".to_string()))?
            }
        };
        let invalid = || (4009, "Invalid access token".to_string());
        if token.len() > 256 || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(invalid());
        }
        let http = crate::api::http();
        let got = crate::api::Api::new(http, format!("Bearer {token}")).call(reqwest::Method::GET, "/oauth2/@me", None).await.map_err(|_| invalid())?;
        if got["application"]["id"].as_str().and_then(|a| a.parse().ok()) != Some(self.app) {
            return Err((4007, "Application does not match the connection's".into()));
        }
        let (me, _) = session().ok_or((4010, "User logged out".to_string()))?;
        if got["user"]["id"] != me["id"] {
            return Err((4009, "Token does not match current user".into()));
        }
        self.authed = true;
        Ok(json!({ "application": got["application"], "user": got["user"], "access_token": token, "expires": got["expires"], "scopes": got["scopes"] }))
    }

    async fn consent(&mut self, kind: &str, scopes: Vec<String>) -> Result<String, Fail> {
        if self.denied {
            return Err(denied());
        }
        let _asking = Asking::take().ok_or((4002, "Already authing".to_string()))?;
        let cid = self.app.to_string();
        let q = query(&cid, kind, &scopes);
        call("GET", &format!("/oauth2/authorize?{q}"), None).await.map_err(|e| (5000, format!("OAuth2 Authorization Error: {e}")))?;
        let (reply, answer) = oneshot::channel();
        crate::rpc::send(Msg::Consent { app: self.app, name: self.name.clone(), icon: self.icon.clone(), scopes, reply });
        if !timeout(Duration::from_secs(120), answer).await.is_ok_and(|a| a == Ok(true)) {
            self.denied = true;
            return Err(denied());
        }
        let body = json!({ "permissions": "0", "authorize": true, "integration_type": 0 });
        let done = call("POST", &format!("/oauth2/authorize?{q}"), Some(body)).await.map_err(|e| (5000, format!("OAuth2 Authorize Error: {e}")))?;
        done["location"].as_str().map(str::to_string).ok_or((5000, "OAuth2 Error: Unknown error occurred".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_scopes_with_a_line_of_text_are_asked_for() {
        assert_eq!(scopes_of(&json!("identify email")).unwrap(), ["identify", "email"]);
        assert_eq!(scopes_of(&json!(["identify"])).unwrap(), ["identify"]);
        for bad in [json!(["identify", "guilds.join"]), json!("bot"), json!("rpc"), json!([]), json!(null)] {
            assert_eq!(scopes_of(&bad).unwrap_err().0, 5000);
        }
    }

    #[test]
    fn the_query_names_the_grant() {
        assert_eq!(query("7", "token", &["identify".into(), "email".into()]), "client_id=7&response_type=token&scope=identify+email&integration_type=0");
    }
}

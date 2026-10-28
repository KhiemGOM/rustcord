use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::App;
use crate::net::Tag;
use crate::plugins;

#[derive(Default)]
pub struct Rpc {
    start: i64,
    assets: HashMap<String, String>,
    asking: Option<(String, bool)>,
    pub sent: Value,
    checked: Option<Instant>,
}

fn ignored(a: &Value) -> bool {
    const P: &str = "IgnoreActivities";
    if !plugins::on(P) {
        return false;
    }
    let id = a["application_id"].as_str().unwrap_or("");
    let name = a["name"].as_str().unwrap_or("");
    let listed = plugins::text(P, "idsList").split(',').map(str::trim).filter(|e| !e.is_empty()).any(|e| e == id || e.eq_ignore_ascii_case(name));
    if listed {
        return plugins::text(P, "listMode") == "1";
    }
    match a["type"].as_u64() {
        Some(0) => plugins::flag(P, "ignorePlaying"),
        Some(1) => plugins::flag(P, "ignoreStreaming"),
        Some(2) => plugins::flag(P, "ignoreListening"),
        Some(3) => plugins::flag(P, "ignoreWatching"),
        Some(5) => plugins::flag(P, "ignoreCompeting"),
        _ => false,
    }
}

fn asset(app: &App, key: &str) -> Option<String> {
    let id = plugins::text("CustomRPC", "appID");
    app.ext.rpc.assets.get(&format!("{id}|{key}")).filter(|v| !v.is_empty()).cloned()
}

fn custom(app: &mut App) -> Option<Value> {
    const P: &str = "CustomRPC";
    let name = plugins::text(P, "appName");
    if !plugins::flag(P, "enablePresence") || name.is_empty() {
        return None;
    }
    let t = |k: &str| plugins::text(P, k);
    let kind = plugins::num(P, "type") as u64;
    let id = t("appID");
    let now = chrono::Utc::now().timestamp_millis();
    let mut a = json!({
        "application_id": if id.is_empty() { "0".to_string() } else { id },
        "name": name,
        "state": t("state"),
        "details": t("details"),
        "type": kind,
        "flags": 1,
        "state_url": t("stateURL"),
        "details_url": t("detailsURL"),
    });
    if kind == 1 {
        a["url"] = t("streamLink").into();
    }
    match plugins::num(P, "timestampMode") as u8 {
        1 => {
            if app.ext.rpc.start == 0 {
                app.ext.rpc.start = now;
            }
            a["timestamps"] = json!({ "start": app.ext.rpc.start });
        }
        2 => {
            use chrono::TimeZone;
            let midnight = chrono::Local::now().date_naive().and_hms_opt(0, 0, 0).and_then(|d| chrono::Local.from_local_datetime(&d).single());
            a["timestamps"] = json!({ "start": midnight.map_or(now, |d| d.timestamp_millis()) });
        }
        3 => {
            let (s, e) = (plugins::num(P, "startTime") as i64, plugins::num(P, "endTime") as i64);
            let mut ts = json!({});
            if s != 0 {
                ts["start"] = s.into();
            }
            if e != 0 {
                ts["end"] = e.into();
            }
            if s != 0 || e != 0 {
                a["timestamps"] = ts;
            }
        }
        _ => {}
    }
    if !t("buttonOneText").is_empty() {
        let both = [(t("buttonOneText"), t("buttonOneURL")), (t("buttonTwoText"), t("buttonTwoURL"))];
        a["buttons"] = both.iter().map(|b| b.0.clone()).filter(|s| !s.is_empty()).collect::<Vec<_>>().into();
        a["metadata"] = json!({ "button_urls": both.iter().map(|b| b.1.clone()).filter(|s| !s.is_empty()).collect::<Vec<_>>() });
    }
    let mut assets = json!({});
    for (key, slot, text, url) in [("imageBig", "large", "imageBigTooltip", "imageBigURL"), ("imageSmall", "small", "imageSmallTooltip", "imageSmallURL")] {
        let k = t(key);
        if k.is_empty() {
            continue;
        }
        if let Some(img) = asset(app, &k) {
            assets[format!("{slot}_image")] = img.into();
            assets[format!("{slot}_text")] = t(text).into();
            assets[format!("{slot}_url")] = t(url).into();
        }
    }
    a["assets"] = assets;
    let (size, max) = (plugins::num(P, "partySize") as u64, plugins::num(P, "partyMaxSize") as u64);
    if size != 0 && max != 0 {
        a["party"] = json!({ "size": [size, max] });
    }
    let obj = a.as_object_mut()?;
    obj.retain(|k, v| k == "type" || !(v.is_null() || v.as_str() == Some("") || v.as_array().is_some_and(|a| a.is_empty()) || v.as_object().is_some_and(|o| o.values().all(|x| x.as_str() == Some("")))));
    Some(a)
}

/// Everything the presence update carries: the detected game and CustomRPC's activity, less
/// whatever IgnoreActivities filters, and nothing while activity sharing is off.
pub fn activities(app: &mut App) -> Value {
    let mut list = Vec::new();
    if !app.store.hide_game {
        if let Some(g) = &app.game {
            list.push(json!({ "name": g.name, "type": 0, "application_id": g.id.to_string(), "timestamps": { "start": g.start } }));
        }
        list.extend(custom(app));
    }
    list.retain(|a| !ignored(a));
    Value::Array(list)
}

/// Called every frame: looks up the images CustomRPC points at, and sends the presence again
/// when what it would carry has changed.
pub fn tick(app: &mut App) {
    if app.conn != crate::net::Conn::Online || app.store.me == 0 {
        return;
    }
    if app.ext.rpc.checked.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
        return;
    }
    app.ext.rpc.checked = Some(Instant::now());
    if matches!(app.remote.get("rpc_asset"), Some(Some(_))) {
        let res = app.remote.remove("rpc_asset").flatten().unwrap_or(Ok(Value::Null));
        if let Some((key, external)) = app.ext.rpc.asking.take() {
            let v = res.ok().and_then(|v| {
                if external {
                    v[0]["external_asset_path"].as_str().map(|p| format!("mp:{p}"))
                } else {
                    let name = key.split_once('|')?.1;
                    v.as_array()?.iter().find(|a| a["name"] == name).and_then(|a| a["id"].as_str().map(str::to_string))
                }
            });
            let fallback = key.split_once('|').map(|k| k.1.to_string()).unwrap_or_default();
            app.ext.rpc.assets.insert(key, v.unwrap_or(if fallback.starts_with("http") { String::new() } else { fallback }));
        }
    }
    const P: &str = "CustomRPC";
    if app.ext.rpc.asking.is_none() && plugins::flag(P, "enablePresence") {
        let id = plugins::text(P, "appID");
        let want = ["imageBig", "imageSmall"].map(|k| plugins::text(P, k)).into_iter().find(|k| !k.is_empty() && !app.ext.rpc.assets.contains_key(&format!("{id}|{k}")));
        if let Some(k) = want.filter(|_| !id.is_empty()) {
            let external = k.starts_with("http");
            app.remote.insert("rpc_asset", None);
            app.ext.rpc.asking = Some((format!("{id}|{k}"), external));
            if external {
                app.rest("POST", format!("/applications/{id}/external-assets"), Some(json!({ "urls": [k] })), Tag::Remote("rpc_asset"));
            } else {
                app.rest("GET", format!("/oauth2/applications/{id}/assets"), None, Tag::Remote("rpc_asset"));
            }
        }
    }
    if activities(app) != app.ext.rpc.sent {
        app.send_presence();
    }
}

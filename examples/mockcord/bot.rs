use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::{ME, W, fresh, msg, payload_json, user};

const APP: u64 = 104;
const CITIES: [&str; 6] = ["Amsterdam", "Berlin", "Cairo", "Dublin", "Edinburgh", "Florence"];

pub fn applications() -> Value {
    json!([{ "id": APP.to_string(), "name": "Dyno", "icon": null, "bot_id": APP.to_string() }])
}

fn opt(kind: u64, name: &str, desc: &str, required: bool) -> Value {
    json!({ "type": kind, "name": name, "description": desc, "required": required })
}

fn with(mut o: Value, k: &str, v: Value) -> Value {
    o[k] = v;
    o
}

fn cmd(id: u64, name: &str, desc: &str, options: Vec<Value>) -> Value {
    json!({ "id": id.to_string(), "application_id": APP.to_string(), "version": "1", "type": 1, "name": name, "description": desc, "options": options })
}

pub fn commands() -> Vec<Value> {
    let ctype = |v: Vec<Value>| json!(v);
    vec![
        cmd(9001, "remind", "Set a reminder", vec![opt(3, "what", "What to remind you about", true), opt(4, "minutes", "In how many minutes", false)]),
        cmd(9002, "poll", "Start a poll", vec![opt(3, "question", "What to ask", true)]),
        cmd(
            9003,
            "showcase",
            "One option of every kind",
            vec![
                with(with(opt(3, "text", "A word or three", true), "min_length", json!(3)), "max_length", json!(20)),
                with(opt(3, "choice", "Pick a colour", false), "choices", json!([{ "name": "Red", "value": "r" }, { "name": "Green", "value": "g" }, { "name": "Blue", "value": "b" }])),
                with(with(opt(4, "count", "How many, 1 to 10", false), "min_value", json!(1)), "max_value", json!(10)),
                with(with(opt(10, "ratio", "A share between 0 and 1", false), "min_value", json!(0)), "max_value", json!(1)),
                opt(5, "flag", "On or off", false),
                opt(6, "person", "Someone in the server", false),
                opt(8, "rank", "A role", false),
                with(opt(7, "place", "A text channel", false), "channel_types", ctype(vec![json!(0)])),
                opt(9, "either", "A person or a role", false),
                opt(11, "file", "Something to attach", false),
                with(opt(3, "city", "Search for a city", false), "autocomplete", json!(true)),
            ],
        ),
        cmd(
            9004,
            "lookup",
            "Look a city up",
            vec![
                with(opt(3, "city", "Search for a city", true), "autocomplete", json!(true)),
                with(opt(4, "days", "How far ahead", false), "choices", json!([{ "name": "One day", "value": 1 }, { "name": "Three days", "value": 3 }])),
            ],
        ),
        cmd(9005, "feedback", "Open a form", vec![]),
        cmd(9006, "slow", "Takes a while to answer", vec![]),
        cmd(9007, "broken", "Fails on purpose", vec![]),
        cmd(9008, "cooldown", "Only once every few seconds", vec![]),
        cmd(9009, "menus", "Every kind of select menu", vec![]),
        cmd(9011, "report", "An embed and nothing else", vec![]),
        cmd(
            9010,
            "team",
            "Manage the team",
            vec![
                json!({ "type": 1, "name": "add", "description": "Add someone", "options": [opt(6, "who", "Who to add", true), opt(8, "role", "Which role", false)] }),
                json!({ "type": 1, "name": "list", "description": "Show the team", "options": [] }),
                json!({ "type": 2, "name": "config", "description": "Team settings", "options": [
                    { "type": 1, "name": "show", "description": "Show the settings", "options": [opt(5, "verbose", "Say more", false)] } ] }),
            ],
        ),
    ]
}

pub fn upload_slots(body: &[u8], port: u16) -> Value {
    let b: Value = serde_json::from_slice(body).unwrap_or_default();
    let slots: Vec<Value> = b["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let name = f["filename"].as_str().unwrap_or("file");
            json!({ "id": f["id"], "upload_url": format!("http://127.0.0.1:{port}/mock-upload/{name}"), "upload_filename": format!("mock/{}/{name}", fresh()) })
        })
        .collect();
    json!({ "attachments": slots })
}

pub fn index() -> Value {
    json!({ "applications": applications(), "application_commands": commands() })
}

fn bad(code: u16, message: &str, field: &str) -> (u16, &'static str, Vec<u8>) {
    let body = json!({ "message": "Invalid Form Body", "code": 50035, "errors": { "data": { field: { "_errors": [{ "code": "INTERACTION_APPLICATION_COMMAND_INVALID", "message": message }] } } } });
    (code, "application/json", body.to_string().into_bytes())
}

fn check(def: &Value, given: &Value, attachments: &[Value]) -> Result<(), String> {
    let name = given["name"].as_str().unwrap_or("");
    let Some(o) = def.as_array().into_iter().flatten().find(|o| o["name"] == name) else { return Err(format!("unknown option {name}")) };
    if given["type"] != o["type"] {
        return Err(format!("{name}: wrong type"));
    }
    let kind = o["type"].as_u64().unwrap_or(0);
    if matches!(kind, 1 | 2) {
        for g in given["options"].as_array().into_iter().flatten() {
            check(&o["options"], g, attachments)?;
        }
        return Ok(());
    }
    let v = &given["value"];
    match kind {
        3 => {
            let s = v.as_str().ok_or(format!("{name}: expected a string"))?;
            let n = s.chars().count() as u64;
            if o["min_length"].as_u64().is_some_and(|m| n < m) || o["max_length"].as_u64().is_some_and(|m| n > m) {
                return Err(format!("{name}: bad length"));
            }
            if let Some(ch) = o["choices"].as_array() {
                if !ch.iter().any(|c| c["value"] == *v) {
                    return Err(format!("{name}: not a choice"));
                }
            }
        }
        4 => {
            let n = v.as_i64().ok_or(format!("{name}: expected an integer"))?;
            if o["min_value"].as_i64().is_some_and(|m| n < m) || o["max_value"].as_i64().is_some_and(|m| n > m) {
                return Err(format!("{name}: out of range"));
            }
            if let Some(ch) = o["choices"].as_array() {
                if !ch.iter().any(|c| c["value"] == *v) {
                    return Err(format!("{name}: not a choice"));
                }
            }
        }
        10 => {
            let n = v.as_f64().ok_or(format!("{name}: expected a number"))?;
            if o["min_value"].as_f64().is_some_and(|m| n < m) || o["max_value"].as_f64().is_some_and(|m| n > m) {
                return Err(format!("{name}: out of range"));
            }
        }
        5 => {
            v.as_bool().ok_or(format!("{name}: expected a boolean"))?;
        }
        6 | 7 | 8 | 9 => {
            let s = v.as_str().ok_or(format!("{name}: expected an id"))?;
            if s.parse::<u64>().is_err() {
                return Err(format!("{name}: not a snowflake"));
            }
        }
        11 => {
            let i = v.as_u64().ok_or(format!("{name}: expected an attachment index"))?;
            let att = attachments.iter().find(|a| a["id"].as_str().and_then(|s| s.parse::<u64>().ok()) == Some(i)).ok_or(format!("{name}: no such attachment"))?;
            if att["uploaded_filename"].as_str().is_none_or(str::is_empty) {
                return Err(format!("{name}: attachment never uploaded"));
            }
        }
        _ => {}
    }
    Ok(())
}

fn required_present(def: &Value, given: &[Value]) -> Result<(), String> {
    for o in def.as_array().into_iter().flatten() {
        if o["required"].as_bool() == Some(true) && !given.iter().any(|g| g["name"] == o["name"]) {
            return Err(format!("{} is required", o["name"].as_str().unwrap_or("")));
        }
    }
    Ok(())
}

fn leaf<'a>(def: &'a Value, given: &'a [Value]) -> (&'a Value, &'a [Value], String) {
    if let Some(g) = given.first().filter(|g| matches!(g["type"].as_u64(), Some(1 | 2)))
        && let Some(o) = def.as_array().into_iter().flatten().find(|o| o["name"] == g["name"])
    {
        let (d, v, rest) = leaf(&o["options"], g["options"].as_array().map(Vec::as_slice).unwrap_or(&[]));
        return (d, v, format!("{} {rest}", g["name"].as_str().unwrap_or("")));
    }
    (def, given, String::new())
}

fn send(w: &W, v: Value) {
    let tx = w.lock().unwrap().events.clone();
    let _ = tx.send(v);
}

fn interaction(id: u64, kind: u64, name: &str) -> Value {
    let mut i = json!({ "id": id.to_string(), "type": kind, "user": user(ME) });
    if !name.is_empty() {
        i["name"] = json!(name);
    }
    i
}

fn reply(w: &W, ch: u64, nonce: &str, iid: u64, kind: u64, name: &str, content: &str, flags: u64, components: Value) -> Value {
    let mut m = msg(fresh(), ch, APP, content);
    m["type"] = json!(20);
    m["application_id"] = json!(APP.to_string());
    m["nonce"] = json!(nonce);
    m["flags"] = json!(flags);
    m["interaction"] = interaction(iid, kind, name);
    m["interaction_metadata"] = interaction(iid, kind, name);
    if !components.is_null() {
        m["components"] = components;
    }
    if name == "report" {
        m["embeds"] = json!([{ "type": "rich", "title": "Weekly report", "description": "3 tasks done, 2 still open", "color": 5793266 }]);
    }
    w.lock().unwrap().msgs.push(m.clone());
    send(w, json!({ "t": "MESSAGE_CREATE", "d": m.clone() }));
    m
}

fn text_row(placeholder: &str, kind: u64, id: &str) -> Value {
    json!({ "type": 1, "components": [{ "type": kind, "custom_id": id, "placeholder": placeholder }] })
}

pub fn menus() -> Value {
    json!([
        { "type": 1, "components": [{ "type": 3, "custom_id": "pick_color", "placeholder": "Pick two or three colours", "min_values": 2, "max_values": 3,
            "options": [{ "label": "Red", "value": "r", "description": "Like a rose" }, { "label": "Green", "value": "g", "description": "Like grass" }, { "label": "Blue", "value": "b", "description": "Like the sea" }] }] },
        text_row("Pick a person", 5, "pick_user"),
        text_row("Pick a role", 6, "pick_role"),
        { "type": 1, "components": [{ "type": 8, "custom_id": "pick_channel", "placeholder": "Pick a text channel", "channel_types": [0] }] },
        { "type": 1, "components": [{ "type": 7, "custom_id": "pick_either", "placeholder": "Pick a person or a role", "min_values": 1, "max_values": 3 }] },
        { "type": 1, "components": [{ "type": 3, "custom_id": "pick_off", "placeholder": "Not for you", "disabled": true, "options": [{ "label": "Nope", "value": "n" }] }] },
        { "type": 1, "components": [
            { "type": 2, "style": 1, "label": "Slow", "custom_id": "slow_btn" },
            { "type": 2, "style": 2, "label": "Fail", "custom_id": "fail_btn" },
            { "type": 2, "style": 3, "label": "Form", "custom_id": "form_btn" },
            { "type": 2, "style": 2, "label": "Off", "custom_id": "off_btn", "disabled": true },
            { "type": 2, "style": 5, "label": "Docs", "url": "https://example.com/docs" },
            { "type": 2, "style": 6, "sku_id": "7001" },
        ] },
    ])
}

fn modal(nonce: &str, ch: u64, iid: u64) -> Value {
    json!({ "t": "INTERACTION_MODAL_CREATE", "d": {
        "id": iid.to_string(), "channel_id": ch.to_string(), "custom_id": "feedback_modal", "nonce": nonce,
        "application": { "id": APP.to_string(), "name": "Dyno", "icon": null, "bot": user(APP) },
        "title": "Send feedback",
        "components": [
            { "type": 1, "components": [{ "type": 4, "custom_id": "subject", "style": 1, "label": "Subject", "min_length": 3, "max_length": 40, "required": true, "placeholder": "What is it about?" }] },
            { "type": 18, "label": "Details", "description": "Say as much as you like", "component": { "type": 4, "custom_id": "details", "style": 2, "max_length": 400, "required": false, "placeholder": "Tell us more" } },
            { "type": 18, "label": "How did it feel?", "component": { "type": 3, "custom_id": "mood", "placeholder": "Choose one", "required": true,
                "options": [{ "label": "Great", "value": "great" }, { "label": "Fine", "value": "fine" }, { "label": "Poor", "value": "poor" }] } },
        ],
    } })
}

fn last_cooldown() -> &'static Mutex<HashMap<String, Instant>> {
    static M: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

fn later(w: &W, ms: u64, f: impl FnOnce(&W) + Send + 'static) {
    let w = w.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(ms)).await;
        f(&w);
    });
}

fn answer(nonce: String, iid: u64, w: &W, ch: u64, kind: u64, name: String, text: String) {
    send(w, json!({ "t": "INTERACTION_CREATE", "d": { "id": iid.to_string(), "nonce": nonce, "type": kind } }));
    reply(w, ch, &nonce, iid, kind, &name, &text, 0, if name == "menus" { menus() } else if name == "remind" || name == "poll" { json!([{ "type": 1, "components": [
        { "type": 2, "style": 1, "label": "Snooze", "custom_id": "snooze", "emoji": { "name": "😴" } },
        { "type": 2, "style": 4, "label": "Cancel", "custom_id": "cancel" },
        { "type": 2, "style": 5, "label": "Docs", "url": "https://example.com/docs" } ] }]) } else { Value::Null });
    send(w, json!({ "t": "INTERACTION_SUCCESS", "d": { "id": iid.to_string(), "nonce": nonce } }));
}

pub fn handle(w: &W, body: &[u8]) -> (u16, &'static str, Vec<u8>) {
    let b = payload_json(body);
    let none = (204, "application/json", Vec::new());
    let need = |k: &str| b[k].as_str().is_some_and(|s| !s.is_empty());
    if !need("session_id") || !need("nonce") || !need("application_id") || !need("channel_id") {
        return bad(400, "session_id, nonce, application_id and channel_id are all required", "session_id");
    }
    let nonce = b["nonce"].as_str().unwrap_or("").to_string();
    let ch: u64 = b["channel_id"].as_str().unwrap_or("0").parse().unwrap_or(0);
    let iid = fresh();
    let kind = b["type"].as_u64().unwrap_or(0);
    let data = &b["data"];
    match kind {
        4 => {
            let focused = data["options"].as_array().into_iter().flatten().find(|o| o["focused"] == true).or_else(|| data["options"][0]["options"].as_array().and_then(|o| o.iter().find(|o| o["focused"] == true)));
            let q = focused.and_then(|o| o["value"].as_str()).unwrap_or("").to_lowercase();
            if q == "fail" {
                later(w, 100, move |w| send(w, json!({ "t": "INTERACTION_FAILURE", "d": { "id": iid.to_string(), "nonce": nonce } })));
                return none;
            }
            let choices: Vec<Value> = CITIES.iter().filter(|c| q.is_empty() || c.to_lowercase().contains(&q)).map(|c| json!({ "name": c, "value": c.to_lowercase() })).collect();
            later(w, 80, move |w| send(w, json!({ "t": "APPLICATION_COMMAND_AUTOCOMPLETE_RESPONSE", "d": { "nonce": nonce, "choices": choices } })));
            none
        }
        2 => {
            let id = data["id"].as_str().unwrap_or("");
            let Some(def) = commands().into_iter().find(|c| c["id"] == id) else { return bad(400, "unknown command", "id") };
            if data["version"] != def["version"] || data["name"] != def["name"] {
                return bad(400, "command is outdated", "version");
            }
            let given = data["options"].as_array().cloned().unwrap_or_default();
            let atts = data["attachments"].as_array().cloned().unwrap_or_default();
            for g in &given {
                if let Err(e) = check(&def["options"], g, &atts) {
                    return bad(400, &e, "options");
                }
            }
            let (ldef, lgiven, sub) = leaf(&def["options"], &given);
            if let Err(e) = required_present(ldef, lgiven) {
                return bad(400, &e, "options");
            }
            let name = format!("{} {sub}", def["name"].as_str().unwrap_or("")).trim().to_string();
            let root = def["name"].as_str().unwrap_or("").to_string();
            let shown: Vec<String> = lgiven.iter().map(|o| format!("{}={}", o["name"].as_str().unwrap_or(""), o["value"].as_str().map(str::to_string).unwrap_or_else(|| o["value"].to_string()))).collect();
            let files: Vec<String> = atts.iter().filter_map(|a| a["filename"].as_str().map(str::to_string)).collect();
            let w2 = w.clone();
            match root.as_str() {
                "feedback" => {
                    later(w, 150, move |w| {
                        send(w, json!({ "t": "INTERACTION_CREATE", "d": { "id": iid.to_string(), "nonce": nonce, "type": 2 } }));
                        send(w, modal(&nonce, ch, iid));
                    });
                }
                "slow" => {
                    later(w, 1200, move |w| {
                        send(w, json!({ "t": "INTERACTION_CREATE", "d": { "id": iid.to_string(), "nonce": nonce, "type": 2 } }));
                        later(w, 1500, move |w| {
                            let m = reply(w, ch, &nonce, iid, 2, &name, "", 128, Value::Null);
                            send(w, json!({ "t": "INTERACTION_SUCCESS", "d": { "id": iid.to_string(), "nonce": nonce } }));
                            let id = m["id"].clone();
                            later(w, 2500, move |w| {
                                let mut done = m.clone();
                                done["content"] = json!("Done thinking: the answer is 42");
                                done["flags"] = json!(0);
                                for x in w.lock().unwrap().msgs.iter_mut().filter(|x| x["id"] == id) {
                                    *x = done.clone();
                                }
                                send(w, json!({ "t": "MESSAGE_UPDATE", "d": done }));
                            });
                        });
                    });
                }
                "broken" => {
                    later(w, 100, move |w| {
                        send(w, json!({ "t": "INTERACTION_CREATE", "d": { "id": iid.to_string(), "nonce": nonce, "type": 2 } }));
                        later(w, 600, move |w| send(w, json!({ "t": "INTERACTION_FAILURE", "d": { "id": iid.to_string(), "nonce": nonce, "reason_code": 1 } })));
                    });
                }
                "cooldown" => {
                    let mut last = last_cooldown().lock().unwrap();
                    if last.get("cooldown").is_some_and(|t| t.elapsed() < Duration::from_secs(8)) {
                        return (429, "application/json", json!({ "message": "You are being rate limited.", "retry_after": 6.0, "global": false }).to_string().into_bytes());
                    }
                    last.insert("cooldown".into(), Instant::now());
                    drop(last);
                    later(w, 100, move |_| answer(nonce, iid, &w2, ch, 2, name, "Cooldown started: try again in 8 seconds".into()));
                }
                _ => {
                    let text = match root.as_str() {
                        "remind" => format!("⏰ Reminder set: **{}**", lgiven.iter().find(|o| o["name"] == "what").and_then(|o| o["value"].as_str()).unwrap_or("?")),
                        "poll" => format!("📊 Poll: **{}**", lgiven.iter().find(|o| o["name"] == "question").and_then(|o| o["value"].as_str()).unwrap_or("?")),
                        "menus" => "Here are the menus".to_string(),
                        "report" => String::new(),
                        _ => format!("`/{name}` {} {}", shown.join(" "), files.join(" ")).trim().to_string(),
                    };
                    later(w, 100, move |_| answer(nonce, iid, &w2, ch, 2, name, text));
                }
            }
            none
        }
        3 => {
            let id = data["custom_id"].as_str().unwrap_or("").to_string();
            let values: Vec<String> = data["values"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
            let comp = data["component_type"].as_u64().unwrap_or(0);
            if !b["message_id"].as_str().is_some_and(|m| !m.is_empty()) || comp == 0 {
                return bad(400, "message_id and component_type are required", "message_id");
            }
            if comp != 2 && (values.is_empty() || data["type"].as_u64() != Some(comp)) {
                return bad(400, "a select needs its type and values", "values");
            }
            let w2 = w.clone();
            match id.as_str() {
                "slow_btn" => {
                    let early = nonce.clone();
                    later(w, 100, move |w| send(w, json!({ "t": "INTERACTION_CREATE", "d": { "id": iid.to_string(), "nonce": early, "type": 3 } })));
                    later(w, 2000, move |w| {
                        reply(w, ch, &nonce, iid, 3, "", "Slow button answered", 0, Value::Null);
                        send(w, json!({ "t": "INTERACTION_SUCCESS", "d": { "id": iid.to_string(), "nonce": nonce } }));
                    });
                }
                "fail_btn" => later(w, 200, move |w| send(w, json!({ "t": "INTERACTION_FAILURE", "d": { "id": iid.to_string(), "nonce": nonce, "reason_code": 1 } }))),
                "form_btn" => later(w, 100, move |w| {
                    send(w, json!({ "t": "INTERACTION_CREATE", "d": { "id": iid.to_string(), "nonce": nonce, "type": 3 } }));
                    send(w, modal(&nonce, ch, iid));
                }),
                "snooze" => later(w, 100, move |_| answer(nonce, iid, &w2, ch, 3, String::new(), "😴 Snoozed for 10 minutes.".into())),
                _ if id.starts_with("pick_") => {
                    let text = format!("{id}: {}", values.join(", "));
                    later(w, 100, move |_| answer(nonce, iid, &w2, ch, 3, String::new(), text));
                }
                _ => later(w, 100, move |_| answer(nonce, iid, &w2, ch, 3, String::new(), format!("{id} pressed"))),
            }
            none
        }
        5 => {
            let rows = data["components"].as_array().cloned().unwrap_or_default();
            if data["custom_id"] != "feedback_modal" || rows.is_empty() {
                return bad(400, "unknown modal", "custom_id");
            }
            let mut out = Vec::new();
            let mut ok = true;
            fn walk(c: &Value, out: &mut Vec<String>, ok: &mut bool) {
                match c["type"].as_u64() {
                    Some(1) => c["components"].as_array().into_iter().flatten().for_each(|x| walk(x, out, ok)),
                    Some(18) => walk(&c["component"], out, ok),
                    Some(4) => match c["value"].as_str() {
                        Some(v) => out.push(format!("{}={v}", c["custom_id"].as_str().unwrap_or(""))),
                        None => *ok = false,
                    },
                    Some(3) => match c["values"].as_array() {
                        Some(v) => out.push(format!("{}={}", c["custom_id"].as_str().unwrap_or(""), v.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("+"))),
                        None => *ok = false,
                    },
                    _ => {}
                }
            }
            rows.iter().for_each(|r| walk(r, &mut out, &mut ok));
            if !ok {
                return bad(400, "a text input needs a value and a select needs values", "components");
            }
            let w2 = w.clone();
            let text = format!("Thanks for the feedback: {}", out.join("; "));
            later(w, 150, move |_| answer(nonce, iid, &w2, ch, 5, String::new(), text));
            none
        }
        _ => bad(400, "unknown interaction type", "type"),
    }
}

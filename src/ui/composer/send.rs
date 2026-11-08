use super::*;

impl App {
    pub fn ensure_commands(&mut self, chan: &Channel) {
        let scope = if chan.guild != 0 { chan.guild } else { chan.id };
        if self.commands.contains_key(&scope) {
            return;
        }
        self.commands.insert(scope, Commands::default());
        let path = if chan.guild != 0 {
            format!("/guilds/{}/application-command-index", chan.guild)
        } else {
            format!("/channels/{}/application-command-index", chan.id)
        };
        self.rest("GET", path, None, Tag::Commands(scope));
    }

    pub fn set_commands(&mut self, scope: Id, v: &Value) {
        let apps = v["applications"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| (sid(&a["id"]), a["name"].as_str().unwrap_or("").to_string(), a["icon"].as_str().map(str::to_string)))
            .collect();
        let list = v["application_commands"].as_array().cloned().unwrap_or_default();
        self.commands.insert(scope, Commands { apps, list });
    }

    /// `+:fire:` then Enter: a reaction on the newest message rather than a message.
    pub fn quick_react(&mut self, chan: &Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).map(|d| d.trim().to_string()).unwrap_or_default();
        let Some(rest) = draft.strip_prefix('+') else { return false };
        let raw = match rest.strip_prefix(':').and_then(|r| r.strip_suffix(':')) {
            Some(name) => self.emoji_named(chan.guild, name).map(|(raw, _)| raw),
            None => Some(rest.to_string()),
        };
        let Some(em) = raw.and_then(|r| react_emoji(&r)) else { return false };
        let Some(last) = self.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| !m.pending)).map(|m| m.id) else { return false };
        self.react(ch, last, em, true);
        self.drafts.remove(&ch);
        true
    }

    /// `s/old/new` then Enter: your newest message with its first "old" turned into "new".
    pub fn substitute(&mut self, chan: &Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        let Some((old, new)) = draft.trim().strip_prefix("s/").and_then(|r| r.split_once('/')) else { return false };
        if old.is_empty() {
            return false;
        }
        let new = new.strip_suffix('/').unwrap_or(new);
        let me = self.store.me;
        let Some(m) = self.store.msgs.get(&ch).and_then(|e| e.list.iter().rev().find(|m| m.author == me && !m.pending)) else { return false };
        if let Some(at) = m.content.find(old) {
            let mut text = m.content.clone();
            text.replace_range(at..at + old.len(), new);
            let id = m.id;
            self.edit_msg(ch, id, text);
        }
        self.drafts.remove(&ch);
        true
    }

    /// Discord's built-in commands, typed out: the text ones send what they'd send, the rest
    /// do their thing and say how it went.
    pub fn built_in(&mut self, chan: &Channel) -> bool {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        let Some(rest) = draft.trim().strip_prefix('/') else { return false };
        let (name, args) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(n, a)| (n, a.trim()));
        let Some(i) = BUILT_IN.iter().position(|(n, ..)| *n == name).filter(|_| built_in_here(self, chan, name)) else { return false };
        let args = self.resolve_aliases(ch, args);
        let (got, err) = build_options(&built_in_command(i)["options"], &args);
        if let Some(e) = err {
            self.toast(e);
            return true;
        }
        self.drafts.remove(&ch);
        self.aliases.remove(&ch);
        self.run_built_in(chan, name, &got);
        true
    }

    /// What a built-in does with the options it was given: the text ones send what they'd
    /// send, the rest do their thing and say how it went.
    pub fn run_built_in(&mut self, chan: &Channel, name: &str, got: &[Value]) {
        let ch = chan.id;
        let get = |k: &str| got.iter().find(|o| o["name"] == k).map(|o| o["value"].clone());
        let text = |k: &str| get(k).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let user = get("user").and_then(|v| v.as_str().and_then(|s| s.parse::<Id>().ok()));
        let who = user.map(|u| self.store.name_in(chan.guild, u)).unwrap_or_default();
        let g = chan.guild;
        let say = |app: &mut App, out: String, tts: bool| {
            app.drafts.insert(ch, out);
            app.post(ch, tts);
        };
        match name {
            "gif" => {
                let at = self.rects.get("gif_button").map_or(pos2(800.0, 600.0), |r| r.left_top());
                self.open_gifs(at);
                self.search_gifs(text("query"));
            }
            "sticker" => {
                self.sticker_q = Some(text("query"));
                self.gifs = None;
                self.picker = None;
                self.stickers = Some(crate::ui::picker::anchor(self));
            }
            "shrug" => say(self, format!("{} ¯\\_(ツ)_/¯", text("message")).trim().to_string(), false),
            "tableflip" => say(self, format!("{} (╯°□°)╯︵ ┻━┻", text("message")).trim().to_string(), false),
            "unflip" => say(self, format!("{} ┬─┬ノ( º _ ºノ)", text("message")).trim().to_string(), false),
            "me" => say(self, format!("_{}_", text("message")), false),
            "spoiler" => say(self, format!("||{}||", text("message")), false),
            "tts" => say(self, text("message"), true),
            "nick" => {
                let nick = text("new_nick");
                let ok = if nick.is_empty() { "Your nickname on this server has been reset.".to_string() } else { format!("Your nickname on this server has been changed to {nick}.") };
                self.rest("PATCH", format!("/guilds/{g}/members/@me"), Some(json!({ "nick": nick })), Tag::ok(ok));
            }
            "thread" => {
                self.rest("POST", format!("/channels/{ch}/threads"), Some(json!({ "name": text("name"), "type": 11, "auto_archive_duration": 4320 })), Tag::Thread(text("message")));
            }
            "kick" => {
                if let Some(u) = user {
                    let why = text("reason");
                    let q = if why.is_empty() { String::new() } else { format!("?reason={}", crate::api::esc(&why)) };
                    self.rest("DELETE", format!("/guilds/{g}/members/{u}{q}"), None, Tag::done(format!("{who} was kicked from the server."), format!("Whoops... {who} could not be kicked.  Please try again!")));
                }
            }
            "ban" => {
                if let Some(u) = user {
                    let secs = get("delete_messages").and_then(|v| v.as_i64()).unwrap_or(0);
                    self.audit("PUT", format!("/guilds/{g}/bans/{u}"), Some(json!({ "delete_message_seconds": secs })), &text("reason"), Tag::done(format!("{who} was banned from the server."), format!("Whoops... {who} could not be banned.  Please try again!")));
                }
            }
            "timeout" => {
                if let Some(u) = user {
                    let secs = get("duration").and_then(|v| v.as_i64()).unwrap_or(60);
                    self.time_out(g, u, secs, &text("reason"));
                    self.toast(format!("{who} timed out for {secs} seconds."));
                }
            }
            "msg" => {
                if let Some(u) = user {
                    self.rest("POST", "/users/@me/channels".into(), Some(json!({ "recipients": [u.to_string()] })), Tag::InviteDm(text("message")));
                }
            }
            "leave" => {
                let silent = get("silent").and_then(|v| v.as_bool()).unwrap_or(false);
                let at = self.store.dms.iter().position(|d| *d == ch);
                self.rest("DELETE", format!("/channels/{ch}?silent={silent}"), None, Tag::undo("Failed to close direct message", move |app: &mut App| {
                    if let Some(at) = at {
                        let n = app.store.dms.len();
                        app.store.dms.insert(at.min(n), ch);
                    }
                }));
                self.store.dms.retain(|d| *d != ch);
                self.friends = true;
            }
            _ => {}
        }
    }

    /// Text too long to send as a message goes up as message.txt, as discord does with a paste.
    pub fn attach_text(&mut self, text: &str) {
        let dir = std::env::temp_dir().join("rustcord").join(chrono::Utc::now().timestamp_millis().to_string());
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("message.txt");
        if std::fs::write(&path, text).is_ok() {
            self.attach_files(vec![path]);
        }
    }

    /// One of an app's message commands from a message's Apps submenu.
    pub fn message_command(&mut self, chan: &Channel, cmd: &Value, target: Id) {
        self.send_command(chan, cmd, 3, Vec::new(), Vec::new(), Some(target), "message_context_menu");
    }

    /// A member timed out until `secs` from now.
    pub fn time_out(&mut self, guild: Id, user: Id, secs: i64, reason: &str) {
        let until = (chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.audit("PATCH", format!("/guilds/{guild}/members/{user}"), Some(json!({ "communication_disabled_until": until })), reason, Tag::reason());
    }

    pub fn resolve_aliases(&self, ch: Id, text: &str) -> String {
        let Some(list) = self.aliases.get(&ch) else { return text.to_string() };
        // longest first so "@ann" never eats half of "@anna"
        let mut list = list.clone();
        list.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
        outside_code(text, |part| list.iter().fold(part.to_string(), |acc, (shown, raw)| acc.replace(shown, raw)))
    }

    /// What `:name:` turns into here: the custom emoji we may use (this server's first), else a
    /// unicode glyph. The flag says which of the two it was.
    pub fn emoji_named(&self, guild: Id, name: &str) -> Option<(String, bool)> {
        let custom = self.store.emojis_for(guild).into_iter().find(|(_, e, ok)| *ok && e.name.eq_ignore_ascii_case(name));
        if let Some((_, e, _)) = custom {
            return Some((format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id), true));
        }
        crate::ui::picker::unicode(name).map(|g| (g.to_string(), false))
    }

    /// Typed `:name:`s that name an emoji we may send become the real thing; anything else,
    /// and anything in code, stays as typed.
    pub fn convert_emoji(&self, guild: Id, text: &str) -> String {
        outside_code(text, |part| {
            let mut out = String::new();
            let mut i = 0;
            while i < part.len() {
                let rest = &part[i..];
                if let Some(n) = custom_len(rest) {
                    out.push_str(&rest[..n]);
                    i += n;
                    continue;
                }
                let c = rest.chars().next().unwrap_or(':');
                if c == ':'
                    && let Some(end) = rest[1..].find(':').map(|e| i + e + 2)
                    && let Some((start, name)) = typed_name(&part[..end])
                    && start == i
                    && let Some((raw, _)) = self.emoji_named(guild, name)
                {
                    out.push_str(&raw);
                    i = end;
                    continue;
                }
                out.push(c);
                i += c.len_utf8();
            }
            out
        })
    }
}

/// Map typed arguments onto command options. `key:value` pairs win, the rest fill
/// required options in order and the last string option soaks up what's left.
pub(super) fn build_options(opts: &Value, args: &str) -> (Vec<Value>, Option<String>) {
    let list: Vec<&Value> = opts.as_array().map(|a| a.iter().collect()).unwrap_or_default();
    if let Some(first) = list.first()
        && matches!(first["type"].as_u64(), Some(1) | Some(2))
    {
        let mut it = args.splitn(2, char::is_whitespace);
        let sub = it.next().unwrap_or("");
        let rest = it.next().unwrap_or("");
        let Some(s) = list.iter().find(|o| o["name"] == sub) else {
            let names: Vec<&str> = list.iter().filter_map(|o| o["name"].as_str()).collect();
            return (vec![], Some(format!("Pick a subcommand: {}", names.join(", "))));
        };
        let (inner, err) = build_options(&s["options"], rest);
        return (vec![json!({ "type": s["type"], "name": sub, "options": inner })], err);
    }
    let mut out = Vec::new();
    let mut loose: Vec<&str> = Vec::new();
    let mut used: Vec<&str> = Vec::new();
    let words: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if let Some((k, v)) = w.split_once(':')
            && let Some(o) = list.iter().find(|o| o["name"] == k)
        {
            // a key:value string option keeps going until the next key:
            let mut val = v.to_string();
            if o["type"].as_u64() == Some(3) {
                while i + 1 < words.len() && !words[i + 1].split_once(':').is_some_and(|(k2, _)| list.iter().any(|o| o["name"] == k2)) {
                    i += 1;
                    val.push(' ');
                    val.push_str(words[i]);
                }
            }
            if let Some(v) = convert(o, &val) {
                out.push(json!({ "type": o["type"], "name": k, "value": v }));
                used.push(o["name"].as_str().unwrap_or(""));
            }
        } else {
            loose.push(w);
        }
        i += 1;
    }
    let free: Vec<&&Value> = list.iter().filter(|o| !used.contains(&o["name"].as_str().unwrap_or(""))).collect();
    let mut li = 0;
    for (n, o) in free.iter().enumerate() {
        if li >= loose.len() {
            break;
        }
        let last_string = o["type"].as_u64() == Some(3) && free[n + 1..].iter().all(|x| x["required"].as_bool() != Some(true));
        let val = if last_string { loose[li..].join(" ") } else { loose[li].to_string() };
        li = if last_string { loose.len() } else { li + 1 };
        match convert(o, &val) {
            Some(v) => out.push(json!({ "type": o["type"], "name": o["name"], "value": v })),
            None => return (vec![], Some(format!("“{val}” isn't a valid {}", o["name"].as_str().unwrap_or("value")))),
        }
    }
    let missing: Vec<&str> = list
        .iter()
        .filter(|o| o["required"].as_bool() == Some(true) && !out.iter().any(|x| x["name"] == o["name"]))
        .filter_map(|o| o["name"].as_str())
        .collect();
    if !missing.is_empty() {
        return (vec![], Some(format!("Missing required option: {}", missing.join(", "))));
    }
    (out, None)
}

pub(super) fn convert(o: &Value, v: &str) -> Option<Value> {
    let digits = |s: &str| -> Option<String> {
        let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        (!d.is_empty()).then_some(d)
    };
    match o["type"].as_u64()? {
        4 => v.parse::<i64>().ok().map(Value::from),
        10 => v.parse::<f64>().ok().map(Value::from),
        5 => match v.to_lowercase().as_str() {
            "true" | "yes" | "y" | "on" | "1" => Some(json!(true)),
            "false" | "no" | "n" | "off" | "0" => Some(json!(false)),
            _ => None,
        },
        6..=9 => digits(v).map(Value::from),
        _ => Some(json!(v)),
    }
}

use super::*;
use crate::state::Map;
use crate::ui::selectmenu::{self, Lead, Opt};
use std::time::Duration;

const DEBOUNCE: Duration = Duration::from_millis(500);
const KEY_PAD: f32 = 14.8;

pub const STRING: u64 = 3;
pub const INTEGER: u64 = 4;
pub const BOOLEAN: u64 = 5;
pub const USER: u64 = 6;
pub const CHANNEL: u64 = 7;
pub const ROLE: u64 = 8;
pub const MENTIONABLE: u64 = 9;
pub const NUMBER: u64 = 10;
pub const ATTACHMENT: u64 = 11;

#[derive(Default)]
pub struct Slot {
    pub text: String,
    pub value: Option<Value>,
    pub file: Option<PathBuf>,
    pub shown: bool,
}

#[derive(Default)]
struct Auto {
    cache: Map<(String, String, String), Vec<(String, Value)>>,
    inflight: Map<String, (String, String, String)>,
    asked: Option<(String, String)>,
    edited: Option<Instant>,
    failed: bool,
}

pub struct Slash {
    pub cmd: Value,
    pub path: Vec<Value>,
    pub opts: Vec<Value>,
    pub slots: Vec<Slot>,
    pub active: usize,
    pub tried: bool,
    pub sel: usize,
    pub optional_open: bool,
    pub closed: bool,
    ac: Auto,
}

fn ty(o: &Value) -> u64 {
    o["type"].as_u64().unwrap_or(STRING)
}

fn name(o: &Value) -> &str {
    o["name"].as_str().unwrap_or("")
}

fn required(o: &Value) -> bool {
    o["required"].as_bool() == Some(true)
}

fn num(o: &Value, k: &str) -> Option<f64> {
    o[k].as_f64()
}

pub fn is_sub(o: &Value) -> bool {
    matches!(ty(o), 1 | 2)
}

/// Every runnable form of a command: itself, or each of its subcommands, as the option indices leading there.
pub fn forms(cmd: &Value) -> Vec<Vec<usize>> {
    fn walk(opts: &Value, at: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (i, o) in opts.as_array().into_iter().flatten().enumerate() {
            at.push(i);
            if ty(o) == 2 {
                walk(&o["options"], at, out);
            } else {
                out.push(at.clone());
            }
            at.pop();
        }
    }
    if cmd["options"].as_array().is_some_and(|o| o.first().is_some_and(is_sub)) {
        let mut out = Vec::new();
        walk(&cmd["options"], &mut Vec::new(), &mut out);
        out
    } else {
        vec![Vec::new()]
    }
}

pub fn form_name(cmd: &Value, path: &[usize]) -> String {
    let mut parts = vec![name(cmd).to_string()];
    let mut opts = &cmd["options"];
    for i in path {
        parts.push(name(&opts[*i]).to_string());
        opts = &opts[*i]["options"];
    }
    parts.join(" ")
}

impl Slash {
    pub fn new(cmd: &Value, path: &[usize]) -> Slash {
        let mut chain = Vec::new();
        let mut opts = &cmd["options"];
        for i in path {
            chain.push(opts[*i].clone());
            opts = &opts[*i]["options"];
        }
        let list: Vec<Value> = opts.as_array().cloned().unwrap_or_default();
        let slots = list.iter().map(|o| Slot { shown: required(o), ..Default::default() }).collect();
        let mut s = Slash { cmd: cmd.clone(), path: chain, opts: list, slots, active: 0, tried: false, sel: 0, optional_open: false, closed: false, ac: Auto::default() };
        s.active = s.visible().first().copied().unwrap_or(0);
        s
    }

    pub fn label(&self) -> String {
        std::iter::once(name(&self.cmd)).chain(self.path.iter().map(name)).collect::<Vec<_>>().join(" ")
    }

    fn visible(&self) -> Vec<usize> {
        (0..self.opts.len()).filter(|i| required(&self.opts[*i]) || self.slots[*i].shown).collect()
    }

    fn hidden(&self) -> Vec<usize> {
        (0..self.opts.len()).filter(|i| !required(&self.opts[*i]) && !self.slots[*i].shown).collect()
    }

    fn has_active(&self) -> bool {
        self.visible().contains(&self.active)
    }

    fn step(&mut self, back: bool) -> bool {
        let vis = self.visible();
        let Some(at) = vis.iter().position(|i| *i == self.active) else { return false };
        let to = if back { at.checked_sub(1) } else { (at + 1 < vis.len()).then_some(at + 1) };
        match to {
            Some(i) => {
                self.active = vis[i];
                self.sel = 0;
                self.closed = false;
                true
            }
            None => false,
        }
    }

    fn next_unfilled(&self) -> Option<usize> {
        self.visible().into_iter().find(|i| i > &self.active && self.slots[*i].text.is_empty() && self.slots[*i].file.is_none())
    }
}

fn choices(o: &Value) -> Vec<(String, Value)> {
    o["choices"].as_array().into_iter().flatten().map(|c| (c["name"].as_str().unwrap_or("").to_string(), c["value"].clone())).collect()
}

fn typed_range(o: &Value) -> String {
    let (min, max) = (num(o, "min_value"), num(o, "max_value"));
    let f = |v: f64| if v.fract() == 0.0 { format!("{}", v as i64) } else { v.to_string() };
    match (min, max) {
        (Some(a), Some(b)) => format!("Enter a number between {} and {}.", f(a), f(b)),
        (Some(a), None) => format!("Enter a number greater than or equal to {}.", f(a)),
        (None, Some(b)) => format!("Enter a number less than or equal to {}.", f(b)),
        _ => String::new(),
    }
}

fn in_range(o: &Value, v: f64) -> Result<(), String> {
    let bad = num(o, "min_value").is_some_and(|m| v < m) || num(o, "max_value").is_some_and(|m| v > m);
    if bad { Err(typed_range(o)) } else { Ok(()) }
}

fn chars_msg(o: &Value) -> Option<String> {
    match (o["min_length"].as_u64(), o["max_length"].as_u64()) {
        (Some(a), Some(b)) => Some(format!("Enter between {a} and {b} characters.")),
        (Some(a), None) => Some(format!("Enter {a} or more characters.")),
        (None, Some(b)) => Some(format!("Enter {b} or fewer characters.")),
        _ => None,
    }
}

fn invalid(o: &Value) -> String {
    if o["choices"].as_array().is_some_and(|c| !c.is_empty()) {
        return "Not a valid choice.".into();
    }
    match ty(o) {
        BOOLEAN => "Not a valid choice.",
        CHANNEL => "Not a valid channel.",
        INTEGER => "Input a valid integer.",
        NUMBER => "Input a valid number.",
        ROLE => "Not a valid role.",
        USER => "Not a valid user.",
        MENTIONABLE => "Not a valid user or role.",
        _ => "Not a valid value.",
    }
    .to_string()
}

fn digits(s: &str) -> Option<String> {
    let t = s.trim().trim_start_matches(['<', '@', '#', '&', '!']).trim_end_matches('>');
    (t.len() >= 15 && t.chars().all(|c| c.is_ascii_digit())).then(|| t.to_string())
}

impl App {
    fn people(&self, chan: &Channel, q: &str) -> Vec<Opt> {
        let q = q.trim_start_matches('@').to_lowercase();
        selectmenu::options_for(&self.store, chan.guild, chan.id, selectmenu::USER, &[]).into_iter().filter(|o| q.is_empty() || o.label.to_lowercase().contains(&q) || o.desc.as_deref().is_some_and(|d| d.to_lowercase().contains(&q))).collect()
    }

    fn picker_options(&self, chan: &Channel, s: &Slash) -> Vec<Opt> {
        let Some(o) = s.opts.get(s.active).filter(|_| !s.closed) else { return Vec::new() };
        let q = s.slots[s.active].text.to_lowercase();
        let mut out = Vec::new();
        let ch = choices(o);
        if !ch.is_empty() {
            out = ch.iter().filter(|(n, _)| q.is_empty() || n.to_lowercase().contains(&q)).map(|(n, v)| Opt { label: n.clone(), value: json_str(v), desc: None, lead: Lead::None }).collect();
        } else if o["autocomplete"].as_bool() == Some(true) {
            let key = (name(o).to_string(), self.context_key(s), s.slots[s.active].text.clone());
            if let Some(c) = s.ac.cache.get(&key) {
                out = c.iter().map(|(n, v)| Opt { label: n.clone(), value: json_str(v), desc: None, lead: Lead::None }).collect();
            }
        } else {
            match ty(o) {
                BOOLEAN => out = ["True", "False"].into_iter().filter(|b| q.is_empty() || b.to_lowercase().starts_with(&q)).map(|b| Opt { label: b.into(), value: b.to_lowercase(), desc: None, lead: Lead::None }).collect(),
                USER => out = self.people(chan, &q),
                ROLE => out = selectmenu::options_for(&self.store, chan.guild, chan.id, selectmenu::ROLE, &[]).into_iter().filter(|o| o.label.to_lowercase().contains(q.trim_start_matches('@'))).collect(),
                CHANNEL => {
                    let types: Vec<u8> = o["channel_types"].as_array().into_iter().flatten().filter_map(|t| t.as_u64().map(|t| t as u8)).collect();
                    out = selectmenu::options_for(&self.store, chan.guild, chan.id, selectmenu::CHANNEL, &types).into_iter().filter(|o| o.label.to_lowercase().contains(q.trim_start_matches('#'))).collect();
                }
                MENTIONABLE => {
                    out = self.people(chan, &q);
                    out.extend(selectmenu::options_for(&self.store, chan.guild, chan.id, selectmenu::ROLE, &[]).into_iter().filter(|o| o.label.to_lowercase().contains(q.trim_start_matches('@'))));
                }
                _ => {}
            }
        }
        out.truncate(25);
        out
    }

    fn context_key(&self, s: &Slash) -> String {
        s.opts.iter().enumerate().filter(|(i, _)| *i != s.active).filter_map(|(i, o)| (!s.slots[i].text.is_empty()).then(|| format!("{}={}\0", name(o), s.slots[i].text))).collect()
    }

    pub fn start_slash(&mut self, chan: &Channel, cmd: &Value, path: &[usize], text: &str) {
        let mut s = Slash::new(cmd, path);
        if let Some(sl) = s.slots.get_mut(s.active) {
            sl.text = text.to_string();
        }
        self.drafts.insert(chan.id, text.to_string());
        self.slash.insert(chan.id, s);
        self.caret_end = true;
        self.focus_composer = true;
    }

    pub fn leave_slash(&mut self, ch: Id, keep: bool) {
        if let Some(s) = self.slash.remove(&ch) {
            self.drafts.insert(ch, if keep { format!("/{}", s.label()) } else { String::new() });
            self.caret_end = true;
        }
    }

    /// `/name something` typed out in full becomes the command with `something` in its first option.
    pub fn slash_from_draft(&mut self, chan: &Channel) {
        let ch = chan.id;
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        let Some(rest) = draft.strip_prefix('/') else { return };
        let Some((word, tail)) = rest.split_once(char::is_whitespace) else { return };
        let scope = if chan.guild != 0 { chan.guild } else { chan.id };
        let app_cmd = self.commands.get(&scope).and_then(|c| c.list.iter().find(|c| c["type"].as_u64().unwrap_or(1) == 1 && c["name"] == word)).cloned();
        let Some(cmd) = app_cmd.or_else(|| BUILT_IN.iter().position(|(n, ..)| *n == word).filter(|_| built_in_here(self, chan, word)).map(built_in_command)) else { return };
        let forms = forms(&cmd);
        let (path, tail) = if forms.len() == 1 && forms[0].is_empty() {
            (Vec::new(), tail.to_string())
        } else {
            let sub = tail.split_whitespace().next().unwrap_or("");
            let Some(p) = forms.iter().find(|p| form_name(&cmd, p).ends_with(&format!(" {sub}")) && !sub.is_empty()) else { return };
            (p.clone(), tail.trim_start()[sub.len()..].trim_start().to_string())
        };
        self.start_slash(chan, &cmd, &path, &tail);
    }

    /// Enter in a slash command: the highlighted choice if a list is open, else send.
    pub fn slash_submit(&mut self, chan: &Channel) {
        let ch = chan.id;
        self.commit(ch);
        let Some(s) = self.slash.get(&ch) else { return };
        let mut options = Vec::new();
        let mut files = Vec::new();
        let mut first_bad = None;
        let mut errs = Vec::new();
        for (i, o) in s.opts.iter().enumerate() {
            let sl = &s.slots[i];
            let empty = sl.text.trim().is_empty() && sl.file.is_none();
            if empty {
                if required(o) {
                    errs.push((i, "This option is required. Specify a value.".to_string()));
                }
                continue;
            }
            match self.resolve(chan, o, sl) {
                Ok(v) => {
                    let v = if ty(o) == ATTACHMENT {
                        files.push(sl.file.clone().unwrap_or_default());
                        json!(files.len() - 1)
                    } else {
                        v
                    };
                    options.push(json!({ "type": ty(o), "name": name(o), "value": v }));
                }
                Err(e) => errs.push((i, e)),
            }
        }
        if let Some((i, _)) = errs.first() {
            first_bad = Some(*i);
        }
        if let Some(i) = first_bad {
            if let Some(s) = self.slash.get_mut(&ch) {
                s.tried = true;
                s.active = i;
                s.slots[i].shown = true;
                self.drafts.insert(ch, s.slots[i].text.clone());
                self.caret_end = true;
            }
            return;
        }
        let s = self.slash.get(&ch).unwrap();
        let mut nested = options;
        for p in s.path.iter().rev() {
            nested = vec![json!({ "type": ty(p), "name": name(p), "options": nested })];
        }
        let cmd = s.cmd.clone();
        if cmd["builtin"] == true {
            self.slash.remove(&ch);
            self.drafts.remove(&ch);
            self.aliases.remove(&ch);
            self.run_built_in(chan, cmd["name"].as_str().unwrap_or_default(), &nested);
            return;
        }
        self.send_command(chan, &cmd, 1, nested, files, None, "slash_ui");
        self.slash.remove(&ch);
        self.drafts.remove(&ch);
        self.aliases.remove(&ch);
        self.jump_to_present(ch);
    }

    fn commit(&mut self, ch: Id) {
        let draft = self.drafts.get(&ch).cloned().unwrap_or_default();
        if let Some(s) = self.slash.get_mut(&ch)
            && let Some(sl) = s.slots.get_mut(s.active)
            && s.opts.get(s.active).is_some_and(|o| ty(o) != ATTACHMENT)
        {
            sl.text = draft;
        }
    }

    fn activate(&mut self, ch: Id, i: usize) {
        self.commit(ch);
        if let Some(s) = self.slash.get_mut(&ch) {
            s.active = i;
            s.sel = 0;
            s.closed = false;
            s.optional_open = false;
            let t = s.slots[i].text.clone();
            self.drafts.insert(ch, t);
            self.caret_end = true;
        }
    }

    fn resolve(&self, chan: &Channel, o: &Value, sl: &Slot) -> Result<Value, String> {
        let text = sl.text.trim();
        if let Some(v) = &sl.value {
            return Ok(v.clone());
        }
        let ch = choices(o);
        if !ch.is_empty() {
            return ch.iter().find(|(n, v)| n.eq_ignore_ascii_case(text) || json_str(v) == text).map(|(_, v)| v.clone()).ok_or_else(|| invalid(o));
        }
        match ty(o) {
            STRING => {
                let n = text.chars().count() as u64;
                if o["min_length"].as_u64().is_some_and(|m| n < m) || o["max_length"].as_u64().is_some_and(|m| n > m) {
                    return Err(chars_msg(o).unwrap_or_else(|| invalid(o)));
                }
                Ok(json!(text))
            }
            INTEGER => {
                let v: i64 = text.parse().map_err(|_| invalid(o))?;
                in_range(o, v as f64)?;
                Ok(json!(v))
            }
            NUMBER => {
                let v: f64 = text.parse().map_err(|_| invalid(o))?;
                in_range(o, v)?;
                Ok(json!(v))
            }
            BOOLEAN => match text.to_lowercase().as_str() {
                "true" | "yes" | "on" => Ok(json!(true)),
                "false" | "no" | "off" => Ok(json!(false)),
                _ => Err(invalid(o)),
            },
            USER | CHANNEL | ROLE | MENTIONABLE => {
                if let Some(d) = digits(text) {
                    return Ok(json!(d));
                }
                let t = text.trim_start_matches(['@', '#']).to_lowercase();
                let want: &[u8] = match ty(o) {
                    USER => &[selectmenu::USER],
                    CHANNEL => &[selectmenu::CHANNEL],
                    ROLE => &[selectmenu::ROLE],
                    _ => &[selectmenu::USER, selectmenu::ROLE],
                };
                for k in want {
                    let found = selectmenu::options_for(&self.store, chan.guild, chan.id, *k, &[]).into_iter().find(|o| o.label.to_lowercase() == t || o.desc.as_deref().is_some_and(|d| d.to_lowercase() == t));
                    if let Some(f) = found {
                        return Ok(json!(f.value));
                    }
                }
                Err(invalid(o))
            }
            ATTACHMENT => sl.file.as_ref().map(|_| Value::Null).ok_or_else(|| invalid(o)),
            _ => Ok(json!(text)),
        }
    }

    fn ask(&mut self, chan: &Channel) {
        let ch = chan.id;
        let Some(s) = self.slash.get(&ch) else { return };
        let Some(o) = s.opts.get(s.active) else { return };
        if o["autocomplete"].as_bool() != Some(true) {
            return;
        }
        let text = s.slots[s.active].text.clone();
        let key = (name(o).to_string(), self.context_key(s), text.clone());
        let same = s.ac.asked.as_ref().is_some_and(|(n, t)| *n == key.0 && *t == text);
        if same || s.ac.cache.contains_key(&key) {
            return;
        }
        if s.ac.edited.is_some_and(|t| t.elapsed() < DEBOUNCE) && s.ac.asked.as_ref().is_some_and(|(n, _)| *n == key.0) {
            return;
        }
        let nonce = make_nonce();
        let mut opts: Vec<Value> = Vec::new();
        for (i, o2) in s.opts.iter().enumerate() {
            if i == s.active {
                opts.push(json!({ "type": ty(o2), "name": name(o2), "value": text, "focused": true }));
            } else if !s.slots[i].text.is_empty()
                && let Ok(v) = self.resolve(chan, o2, &s.slots[i])
                && ty(o2) != ATTACHMENT
            {
                opts.push(json!({ "type": ty(o2), "name": name(o2), "value": v }));
            }
        }
        let mut nested = opts;
        for p in s.path.iter().rev() {
            nested = vec![json!({ "type": ty(p), "name": name(p), "options": nested })];
        }
        let mut payload = json!({
            "type": 4,
            "application_id": s.cmd["application_id"],
            "channel_id": ch.to_string(),
            "session_id": self.store.session,
            "data": {
                "version": s.cmd["version"],
                "id": s.cmd["id"],
                "name": s.cmd["name"],
                "type": 1,
                "options": nested,
                "application_command": s.cmd,
            },
            "nonce": nonce,
        });
        if chan.guild != 0 {
            payload["guild_id"] = json!(chan.guild.to_string());
        }
        if s.cmd["guild_id"].is_string() {
            payload["data"]["guild_id"] = s.cmd["guild_id"].clone();
        }
        let s = self.slash.get_mut(&ch).unwrap();
        s.ac.asked = Some((key.0.clone(), text));
        s.ac.failed = false;
        s.ac.inflight.insert(nonce, key);
        self.net.send(Cmd::Interact(payload));
    }

    pub fn autocomplete_response(&mut self, nonce: &str, d: &Value) {
        let list: Vec<(String, Value)> = d["choices"].as_array().into_iter().flatten().map(|c| (c["name"].as_str().unwrap_or("").to_string(), c["value"].clone())).collect();
        for s in self.slash.values_mut() {
            if let Some(key) = s.ac.inflight.remove(nonce) {
                s.ac.cache.insert(key, list);
                s.ac.failed = false;
                return;
            }
        }
    }

    pub fn autocomplete_failed(&mut self, nonce: &str) -> bool {
        for s in self.slash.values_mut() {
            if s.ac.inflight.remove(nonce).is_some() {
                s.ac.failed = true;
                return true;
            }
        }
        false
    }

    /// Takes the keys a slash command owns: Tab and Shift+Tab between options, the list's arrows and Enter, Backspace in an empty one.
    pub fn slash_keys(&mut self, ui: &mut Ui, chan: &Channel) {
        use super::super::take_key;
        let ch = chan.id;
        let Some(s) = self.slash.get(&ch) else { return };
        let list = if s.optional_open { s.hidden().len() } else { self.picker_options(chan, s).len() };
        let (has_active, sel) = (s.has_active(), s.sel);
        let text_empty = self.drafts.get(&ch).is_none_or(String::is_empty);
        let is_file = s.opts.get(s.active).is_some_and(|o| ty(o) == ATTACHMENT);
        let key = |k: Key, m: Modifiers| ui.input_mut(|i| take_key(i, m, k));
        if list > 0 {
            if key(Key::ArrowDown, Modifiers::NONE) {
                self.slash.get_mut(&ch).unwrap().sel = (sel + 1) % list;
            }
            if key(Key::ArrowUp, Modifiers::NONE) {
                self.slash.get_mut(&ch).unwrap().sel = (sel + list - 1) % list;
            }
            if key(Key::Enter, Modifiers::NONE) || key(Key::Tab, Modifiers::NONE) {
                self.slash_pick(chan);
                return;
            }
            if key(Key::Escape, Modifiers::NONE) {
                let s = self.slash.get_mut(&ch).unwrap();
                s.optional_open = false;
                s.closed = true;
                return;
            }
        }
        if is_file && key(Key::Enter, Modifiers::NONE) {
            self.pick_file(ch);
            return;
        }
        let back = key(Key::Tab, Modifiers::SHIFT);
        let fwd = key(Key::Tab, Modifiers::NONE);
        if back || fwd {
            self.commit(ch);
            let s = self.slash.get_mut(&ch).unwrap();
            if s.step(back) {
                let t = s.slots[s.active].text.clone();
                self.drafts.insert(ch, t);
                self.caret_end = true;
            } else if fwd && !s.hidden().is_empty() {
                s.optional_open = true;
                s.sel = 0;
            }
        }
        if has_active && text_empty && key(Key::Backspace, Modifiers::NONE) {
            let s = self.slash.get_mut(&ch).unwrap();
            let a = s.active;
            if s.slots[a].file.is_some() || s.slots[a].value.is_some() {
                s.slots[a].file = None;
                s.slots[a].value = None;
            } else if s.step(true) {
                let t = s.slots[s.active].text.clone();
                self.drafts.insert(ch, t);
                self.caret_end = true;
            } else {
                self.leave_slash(ch, true);
                return;
            }
        }
        if list == 0 && key(Key::Escape, Modifiers::NONE) {
            self.leave_slash(ch, false);
        }
    }

    fn pick_file(&mut self, ch: Id) {
        if let Some(f) = rfd::FileDialog::new().pick_file() {
            self.set_file(ch, f);
        }
    }

    pub fn set_file(&mut self, ch: Id, f: PathBuf) {
        if let Some(s) = self.slash.get_mut(&ch) {
            let a = s.active;
            s.slots[a].text = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            s.slots[a].file = Some(f);
            s.slots[a].value = None;
            if let Some(n) = s.next_unfilled() {
                s.active = n;
            }
            let t = s.slots[s.active].text.clone();
            let ch_ = ch;
            self.drafts.insert(ch_, t);
            self.caret_end = true;
        }
    }

    /// The list's highlighted row goes into the option, and on to the next that wants filling.
    pub fn slash_pick(&mut self, chan: &Channel) {
        let ch = chan.id;
        let Some(s) = self.slash.get(&ch) else { return };
        if s.optional_open {
            let hidden = s.hidden();
            if let Some(i) = hidden.get(s.sel).copied() {
                let s = self.slash.get_mut(&ch).unwrap();
                s.slots[i].shown = true;
                s.optional_open = false;
                self.activate(ch, i);
            }
            return;
        }
        let items = self.picker_options(chan, s);
        let Some(it) = items.get(s.sel).cloned() else { return };
        let s = self.slash.get_mut(&ch).unwrap();
        let a = s.active;
        let o = s.opts[a].clone();
        s.slots[a].text = it.label.clone();
        let value = if !choices(&o).is_empty() || o["autocomplete"].as_bool() == Some(true) {
            let found = choices(&o).into_iter().find(|(n, _)| *n == it.label).map(|(_, v)| v).or_else(|| s.ac.cache.values().flatten().find(|(n, _)| *n == it.label).map(|(_, v)| v.clone()));
            found.unwrap_or_else(|| json!(it.value))
        } else if ty(&o) == BOOLEAN {
            json!(it.value == "true")
        } else {
            json!(it.value)
        };
        s.slots[a].value = Some(value);
        s.ac.asked = None;
        s.sel = 0;
        match s.next_unfilled() {
            Some(n) => {
                s.active = n;
                s.closed = false;
            }
            None => s.closed = true,
        }
        let t = s.slots[s.active].text.clone();
        self.drafts.insert(ch, t);
        self.caret_end = true;
    }

    fn pill_error(&self, chan: &Channel, s: &Slash, i: usize) -> Option<String> {
        let (o, sl) = (&s.opts[i], &s.slots[i]);
        let empty = sl.text.trim().is_empty() && sl.file.is_none();
        if empty {
            return (s.tried && required(o)).then(|| "This option is required. Specify a value.".to_string());
        }
        if sl.value.is_some() {
            return None;
        }
        if o["autocomplete"].as_bool() == Some(true) && ty(o) == STRING {
            return None;
        }
        if i == s.active && matches!(ty(o), USER | CHANNEL | ROLE | MENTIONABLE) && !s.tried {
            return None;
        }
        self.resolve(chan, o, sl).err().filter(|_| i != s.active || s.tried || !matches!(ty(o), STRING | INTEGER | NUMBER | BOOLEAN) || !sl.text.is_empty())
    }
}

pub struct Info {
    pub label: String,
    pub active: String,
    pub tried: bool,
    pub optional_open: bool,
    pub hidden: Vec<String>,
    pub error: Option<String>,
    pub failed: bool,
    pub list: usize,
    pub filled: Vec<(String, String)>,
}

impl App {
    pub fn slash_info(&self, chan: &Channel) -> Option<Info> {
        let s = self.slash.get(&chan.id)?;
        Some(Info {
            label: s.label(),
            active: s.opts.get(s.active).map(|o| name(o).to_string()).unwrap_or_default(),
            tried: s.tried,
            optional_open: s.optional_open,
            hidden: s.hidden().into_iter().map(|i| name(&s.opts[i]).to_string()).collect(),
            error: s.opts.get(s.active).and_then(|_| self.pill_error(chan, s, s.active)),
            failed: s.ac.failed,
            list: if s.optional_open { s.hidden().len() } else { self.picker_options(chan, s).len() },
            filled: s.opts.iter().zip(&s.slots).filter(|(_, sl)| sl.value.is_some() || sl.file.is_some()).map(|(o, sl)| (name(o).to_string(), sl.value.as_ref().map(json_str).unwrap_or_default())).collect(),
        })
    }

    pub fn slash_hidden_at(&mut self, ch: Id, opt: &str) {
        if let Some(s) = self.slash.get_mut(&ch)
            && let Some(i) = s.hidden().iter().position(|i| name(&s.opts[*i]) == opt)
        {
            s.sel = i;
        }
    }

    pub fn slash_sel(&mut self, ch: Id, at: usize) {
        if let Some(s) = self.slash.get_mut(&ch) {
            s.sel = at;
        }
    }
}

fn json_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        o => o.to_string(),
    }
}

/// The bar over the composer: the option you're in, what it's for, and what's wrong with it.
pub fn bar(app: &mut App, ui: &mut Ui, chan: &Channel) {
    let ch = chan.id;
    let Some(s) = app.slash.get(&ch) else { return };
    let (n, text, err) = if s.opts.is_empty() {
        (String::new(), "This command has no options. Send it!".to_string(), None)
    } else if s.has_active() {
        let o = &s.opts[s.active];
        let mut d = o["description"].as_str().unwrap_or("").to_string();
        if d.is_empty() && matches!(ty(o), INTEGER | NUMBER) {
            d = typed_range(o);
        }
        (name(o).to_string(), d, app.pill_error(chan, s, s.active))
    } else {
        (String::new(), "Nothing required. You can send the command or add an option below".to_string(), None)
    };
    let failed = s.ac.failed && s.opts.get(s.active).is_some_and(|o| o["autocomplete"].as_bool() == Some(true));
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 37.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, pal().bg_secondary);
    let mut x = r.left() + 16.0;
    if !n.is_empty() {
        let nr = p.text(pos2(x, r.center().y), Align2::LEFT_CENTER, &n, semi(16.0), pal().header_primary);
        x = nr.right() + 8.0;
    }
    match (&err, failed) {
        (Some(e), _) => {
            p.text(pos2(x, r.center().y), Align2::LEFT_CENTER, e, medium(14.0), pal().critical);
        }
        (None, true) => {
            p.text(pos2(x, r.center().y), Align2::LEFT_CENTER, "Loading options failed", medium(14.0), pal().critical);
        }
        _ => {
            text::paint(p, pos2(x, r.center().y), Align2::LEFT_CENTER, &text, medium(14.0), pal().header_secondary, r.right() - x - 48.0);
        }
    }
    let xr = Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(16.0, 16.0));
    let resp = ui.interact(xr.expand(4.0), egui::Id::new(("slash_close", ch)), Sense::CLICK);
    icons::draw(ui.painter(), Icon::Close, xr, crate::ui::motion::mix(pal().header_secondary, pal().interactive_hover, crate::ui::motion::hot(ui.ctx(), &resp)));
    app.rects.insert("slash_close".into(), xr);
    if resp.on_hover_text("Close").clicked() {
        app.leave_slash(ch, false);
    }
}

/// What a slash command puts in the composer's row: its icon, its name, a pill for each option, and where the typing goes.
pub fn row(app: &mut App, ui: &mut Ui, chan: &Channel, tid: egui::Id, width: f32, off: bool) {
    let ch = chan.id;
    let (icon_r, _) = ui.allocate_exact_size(vec2(24.0, 56.0), Sense::hover());
    let ir = Rect::from_center_size(icon_r.center(), vec2(24.0, 24.0));
    {
        let s = &app.slash[&ch];
        let app_id = sid(&s.cmd["application_id"]);
        let icon = app.commands.get(&if chan.guild != 0 { chan.guild } else { chan.id }).and_then(|c| c.apps.iter().find(|a| a.0 == app_id)).and_then(|a| a.2.clone());
        match icon {
            Some(h) => avatar(ui, ir, &format!("{}/app-icons/{app_id}/{h}.png?size=64", crate::api::cdn())),
            None if s.cmd["builtin"] == true => {
                ui.painter().circle_filled(ir.center(), 12.0, BLURPLE);
                icons::draw(ui.painter(), Icon::Discord, ir.shrink(5.0), Color32::WHITE);
            }
            None => {
                ui.painter().rect_filled(ir, CornerRadius::same(4), pal().header_secondary);
                ui.painter().text(ir.center(), Align2::CENTER_CENTER, "/", semi(16.0), pal().bg_input);
            }
        }
    }
    ui.add_space(23.0);
    let origin = ui.cursor().min;
    let (area, _) = ui.allocate_exact_size(vec2((width - 47.0).max(60.0), 56.0), Sense::hover());
    let top = origin.y;
    let clip = ui.painter().with_clip_rect(area);
    let label = app.slash[&ch].label();
    let lg = clip.layout_no_wrap(format!("/{label}"), semi(16.0), pal().text_normal);
    let vis = app.slash[&ch].visible();
    let active = app.slash[&ch].active;
    let draft = app.drafts.get(&ch).cloned().unwrap_or_default();
    let hidden = app.slash[&ch].hidden().len();
    let more = clip.layout_no_wrap(format!("+{hidden} optional"), reg(14.0), pal().header_secondary);
    let widths: Vec<f32> = vis
        .iter()
        .map(|i| {
            let s = &app.slash[&ch];
            let kw = clip.layout_no_wrap(name(&s.opts[*i]).to_string(), reg(16.0), pal().text_normal).size().x + KEY_PAD;
            let text = if *i == active { &draft } else { &s.slots[*i].text };
            let vw = clip.layout_no_wrap(text.clone(), reg(16.0), pal().text_normal).size().x;
            kw + 8.0 + vw.max(if *i == active { 24.0 } else { 0.0 }) + 9.0 + 2.0
        })
        .collect();
    // the row scrolls to keep the active pill, and the "+N optional" after the last, in view
    let mut shift = 0.0;
    let mut end = lg.size().x + 8.0;
    for (n, i) in vis.iter().enumerate() {
        if *i == active {
            let tail = if n + 1 == vis.len() && hidden > 0 { more.size().x + 12.0 } else { 0.0 };
            shift = (end + widths[n] + tail - area.width()).max(0.0);
        }
        end += widths[n] + 8.0;
    }
    clip.galley(pos2(area.left() - shift, top + 28.0 - lg.size().y / 2.0), lg.clone(), pal().text_normal);
    let mut x = area.left() - shift + lg.size().x + 8.0;
    let mut click = None;
    for (n, i) in vis.iter().enumerate() {
        let (pw, is_active) = (widths[n], *i == active);
        let pill = Rect::from_min_size(pos2(x, top + 15.0), vec2(pw, 26.0));
        let s = &app.slash[&ch];
        let o = s.opts[*i].clone();
        let err = app.pill_error(chan, s, *i);
        let key = name(&o).to_string();
        let kg = clip.layout_no_wrap(key.clone(), reg(16.0), pal().text_normal);
        let kw = kg.size().x + KEY_PAD;
        clip.rect_filled(pill, CornerRadius::same(4), Color32::from_rgba_unmultiplied(151, 151, 159, 51));
        let kr = Rect::from_min_size(pill.min + vec2(1.0, 1.0), vec2(kw, 24.0));
        clip.rect_filled(kr, CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }, pal().bg_tertiary);
        clip.galley(pos2(kr.left() + KEY_PAD / 2.0, kr.center().y - kg.size().y / 2.0), kg, pal().text_normal);
        let vr = Rect::from_min_max(pos2(kr.right() + 8.0, pill.top() + 2.0), pos2(pill.right() - 9.0, pill.bottom() - 2.0));
        let border = if err.is_some() { RED } else if is_active { Color32::from_rgb(0, 120, 212) } else { Color32::TRANSPARENT };
        clip.rect_stroke(pill, CornerRadius::same(4), Stroke::new(1.0, border), egui::StrokeKind::Inside);
        app.rects.insert(format!("slash_pill:{key}"), pill);
        let resp = ui.interact(pill.intersect(area), egui::Id::new(("slash_pill", ch, *i)), Sense::CLICK);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if is_active {
            let is_file = ty(&o) == ATTACHMENT;
            let draft_ref = app.drafts.entry(ch).or_default();
            if is_file {
                let g = clip.layout_no_wrap(if draft_ref.is_empty() { "Upload a file".to_string() } else { draft_ref.clone() }, reg(16.0), if draft_ref.is_empty() { pal().text_muted } else { pal().text_normal });
                clip.galley(pos2(vr.left(), vr.center().y - g.size().y / 2.0), g, pal().text_normal);
                if resp.clicked() && !off {
                    app.pick_file(ch);
                }
                ui.memory_mut(|m| m.request_focus(tid));
                let mut sink = String::new();
                ui.put(Rect::from_min_size(vr.min, vec2(1.0, 1.0)), egui::TextEdit::singleline(&mut sink).id(tid).event_filter(egui::EventFilter { tab: true, escape: true, horizontal_arrows: true, vertical_arrows: true }).frame(Frame::NONE).margin(Margin::ZERO).desired_width(1.0));
            } else {
                let edit = egui::TextEdit::singleline(draft_ref).id(tid).event_filter(egui::EventFilter { tab: true, escape: true, horizontal_arrows: true, vertical_arrows: true }).frame(Frame::NONE).margin(Margin::ZERO).font(reg(16.0)).text_color(pal().text_normal).interactive(!off).desired_width(vr.width());
                ui.put(Rect::from_min_size(pos2(vr.left(), top + 17.0), vec2(vr.width() + 2.0, 22.0)), edit);
            }
        } else {
            let sl = &app.slash[&ch].slots[*i];
            let (t, c) = if sl.text.is_empty() { (String::new(), pal().text_muted) } else { (sl.text.clone(), pal().text_normal) };
            let g = clip.layout_no_wrap(t, reg(16.0), c);
            clip.galley(pos2(vr.left(), vr.center().y - g.size().y / 2.0), g, c);
            if resp.clicked() && !off {
                click = Some(*i);
            }
        }
        x += pw + 8.0;
    }
    if vis.is_empty() {
        let mut sink = String::new();
        ui.put(Rect::from_min_size(pos2(x, top + 17.0), vec2(1.0, 22.0)), egui::TextEdit::singleline(&mut sink).id(tid).event_filter(egui::EventFilter { tab: true, escape: true, horizontal_arrows: true, vertical_arrows: true }).frame(Frame::NONE).margin(Margin::ZERO).desired_width(1.0));
        ui.memory_mut(|m| m.request_focus(tid));
    }
    if hidden > 0 {
        let r = Rect::from_min_size(pos2(x + 4.0, top + 28.0 - more.size().y / 2.0), more.size());
        clip.galley(r.min, more.clone(), pal().header_secondary);
        let resp = ui.interact(r.expand(4.0).intersect(area), egui::Id::new(("slash_optional", ch)), Sense::CLICK);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        app.rects.insert("slash_optional".into(), r);
        if resp.clicked() {
            let s = app.slash.get_mut(&ch).unwrap();
            s.optional_open = !s.optional_open;
            s.sel = 0;
            app.focus_composer = true;
        }
    }
    if let Some(i) = click {
        app.activate(ch, i);
        ui.memory_mut(|m| m.request_focus(tid));
    }
    let new = app.drafts.get(&ch).cloned().unwrap_or_default();
    if let Some(s) = app.slash.get_mut(&ch)
        && let Some(sl) = s.slots.get_mut(s.active)
        && s.opts.get(s.active).is_some_and(|o| ty(o) != ATTACHMENT)
        && sl.text != new
    {
        sl.text = new;
        sl.value = None;
        s.sel = 0;
        s.closed = false;
        s.ac.edited = Some(Instant::now());
        s.optional_open = false;
    }
    app.ask(chan);
    if let Some(s) = app.slash.get(&ch)
        && let Some(t) = s.ac.edited
        && t.elapsed() < DEBOUNCE
    {
        ui.ctx().request_repaint_after(DEBOUNCE.saturating_sub(t.elapsed()) + Duration::from_millis(20));
    }
}

/// The list over the composer: choices, matches, or the options still to add.
pub fn list(app: &mut App, ctx: &egui::Context, chan: &Channel, composer: Rect) {
    let ch = chan.id;
    let Some(s) = app.slash.get(&ch) else { return };
    let (title, items): (&str, Vec<Opt>) = if s.optional_open {
        ("Options", s.hidden().into_iter().map(|i| Opt { label: name(&s.opts[i]).to_string(), value: i.to_string(), desc: s.opts[i]["description"].as_str().map(str::to_string), lead: Lead::None }).collect())
    } else {
        ("Options", app.picker_options(chan, s))
    };
    let asked = s.opts.get(s.active).is_some_and(|o| {
        let key = (name(o).to_string(), app.context_key(s), s.slots[s.active].text.clone());
        o["autocomplete"].as_bool() == Some(true) && !s.ac.cache.contains_key(&key) && !s.ac.failed && !s.closed
    });
    let waiting = !s.optional_open && asked && items.is_empty();
    if items.is_empty() && !waiting {
        return;
    }
    let title = if s.optional_open { title.to_string() } else { s.opts.get(s.active).map(|o| name(o).to_string()).unwrap_or_default() };
    let sel = s.sel.min(items.len().saturating_sub(1));
    let mut picked = None;
    let mut hover = None;
    egui::Area::new(egui::Id::new("slash_list")).order(egui::Order::Foreground).pivot(Align2::LEFT_BOTTOM).fixed_pos(composer.left_top() - vec2(0.0, 37.0)).show(ctx, |ui| {
        Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
            ui.set_width(composer.width() - 16.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
            ui.painter().text(pos2(hr.left() + 8.0, hr.center().y), Align2::LEFT_CENTER, title, medium(14.0), pal().header_secondary);
            if waiting {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
                ui.painter().text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, "Loading...", reg(15.0), pal().text_muted);
            }
            let h = 40.0;
            let list_h = h * items.len().min(8) as f32;
            egui::ScrollArea::vertical().min_scrolled_height(list_h).max_height(list_h).show(ui, |ui| {
                for (n, it) in items.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::CLICK);
                    if n == sel {
                        ui.painter().rect_filled(r, CornerRadius::same(4), pal().bg_mod_selected);
                    }
                    let mut x = r.left() + 8.0;
                    match &it.lead {
                        Lead::Avatar(u) => {
                            avatar(ui, Rect::from_center_size(pos2(x + 12.0, r.center().y), vec2(24.0, 24.0)), u);
                            x += 32.0;
                        }
                        Lead::Dot(c) => {
                            ui.painter().circle_filled(pos2(x + 8.0, r.center().y), 5.0, *c);
                            x += 20.0;
                        }
                        Lead::Channel(id) => {
                            if let Some(c) = app.store.channels.get(id) {
                                super::super::side::chan_icon(&app.store, ui.painter(), c, Rect::from_center_size(pos2(x + 10.0, r.center().y), vec2(18.0, 18.0)), pal().channel_default);
                            }
                            x += 28.0;
                        }
                        Lead::None => {}
                    }
                    let nr = text::paint(ui.painter(), pos2(x + 4.0, r.center().y), Align2::LEFT_CENTER, &it.label, semi(15.0), pal().header_primary, r.width() * 0.5);
                    if let Some(d) = &it.desc {
                        text::paint(ui.painter(), pos2(nr.right() + 8.0, r.center().y), Align2::LEFT_CENTER, d, reg(14.0), pal().header_secondary, r.right() - nr.right() - 16.0);
                    }
                    if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                        hover = Some(n);
                    }
                    if resp.clicked() {
                        picked = Some(n);
                    }
                }
            });
        });
    });
    if let Some(n) = hover.or(picked) {
        app.slash.get_mut(&ch).unwrap().sel = n;
    }
    if picked.is_some() {
        app.slash_pick(chan);
        app.focus_composer = true;
    }
}

#[cfg(test)]
mod tests;

use std::io::Write;
use std::hash::{Hash, Hasher};
use egui::{Event, Key, Modifiers, MouseWheelUnit, TouchPhase, ViewportCommand, pos2, vec2};
use serde_json::json;

use super::Run;
use crate::ui::App;

#[derive(Clone, Default)]
struct Controls {
    pass: u64,
    items: std::collections::BTreeMap<String, egui::Rect>,
}

impl Controls {
    fn insert(&mut self, pass: u64, name: &str, rect: egui::Rect) {
        if self.pass != pass {
            self.items.clear();
            self.pass = pass;
        }
        self.items.insert(format!("control:{name}"), rect);
    }
}

pub(in crate::ui) fn record(ctx: &egui::Context, name: &str, rect: egui::Rect) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var_os("RUSTCORD_STRESS_SEED").is_some()) {
        return;
    }
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| data.get_temp_mut_or_default::<Controls>(egui::Id::new("explore_controls")).insert(pass, name, rect));
}

fn explore_allowed(name: &str) -> bool {
    let name = name.to_lowercase();
    !["log out", "logout", "purchase", "checkout", "subscribe", "buy ", "upload", "download", "choose file", "select file", "browse files", "browse folder", "open folder", "startup", "computer starts up", "authorize", "external", "shop"].iter().any(|word| name.contains(word))
}

fn explore_pick(names: &[String], visits: &serde_json::Map<String, serde_json::Value>, state: &str, action: &str, salt: usize) -> Option<usize> {
    let count = |name: &str| visits.get(&format!("{state}|{action}|{name}")).and_then(|n| n.as_u64()).unwrap_or(0);
    let least = names.iter().map(|name| count(name)).min()?;
    let choices: Vec<_> = names.iter().enumerate().filter(|(_, name)| count(name) == least).map(|(i, _)| i).collect();
    Some(choices[salt % choices.len()])
}

impl Run {
    pub(super) fn stress_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        if std::env::var("RUSTCORD_TOKEN").as_deref() != Ok("mock-token") || !std::env::var("RUSTCORD_API").is_ok_and(|api| api.starts_with("http://127.0.0.1:")) {
            self.fail("exploratory actions require the local mock".into());
            return;
        }
        let case = self.step - 9000;
        let seed = std::env::var("RUSTCORD_STRESS_SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(1).max(1);
        let limit = std::env::var("RUSTCORD_STRESS_ACTIONS").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(300).clamp(1, 800);
        let screen = ctx.content_rect();
        let mut controls: std::collections::BTreeMap<_, _> = app.rects.iter().map(|(name, rect)| (name.clone(), *rect)).collect();
        let pass = ctx.cumulative_pass_nr();
        if let Some(recorded) = ctx.data(|data| data.get_temp::<Controls>(egui::Id::new("explore_controls"))) {
            if recorded.pass + 1 >= pass {
                controls.extend(recorded.items);
            }
        }
        let mut visible: Vec<_> = controls.iter().filter(|(_, r)| screen.contains(r.center())).map(|(name, _)| name.clone()).collect();
        visible.sort();
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}|{:?}|{:?}", app.ui.guild, app.cur(), app.settings_page).hash(&mut hash);
        for name in &visible {
            name.split(':').filter(|part| !part.chars().all(char::is_numeric)).collect::<Vec<_>>().hash(&mut hash);
        }
        let state = format!("{:016x}", hash.finish());
        let previous = self.rects.get("stress_previous_state").cloned();
        let states = self.rects.entry("stress_states").or_insert(json!({})).as_object_mut().unwrap();
        let fresh = !states.contains_key(&state);
        let count = states.get(&state).and_then(|n| n.as_u64()).unwrap_or(0);
        states.insert(state.clone(), json!(count + 1));
        if case == limit {
            if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("actions.jsonl")) {
                let _ = writeln!(log, "{}", json!({ "case": case, "seed": seed, "terminal": true, "before": state, "previous_state": previous, "visible": visible }));
            }
            self.shot(ctx, "stress_final.png");
            self.mark("stress_completed");
            self.rects.insert("stress_seed".into(), json!(seed));
            self.rects.insert("stress_actions".into(), json!(limit));
            self.step = 99;
            return;
        }
        let mut rng = self.rects.get("stress_rng").and_then(|v| v.as_u64()).unwrap_or(seed);
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng as usize
        };
        let kind = next() % 10;
        let action = match kind {
            0 => {
                let w = [640.0, 800.0, 940.0, 1280.0, 1600.0][next() % 5];
                let h = [500.0, 720.0, 900.0][next() % 3];
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(w, h)));
                json!({ "resize": [w, h] })
            }
            1 => {
                let dy = if next() % 2 == 0 { 1.0 } else { -1.0 };
                self.queue.push_back(vec![Event::PointerMoved(screen.center()), Event::MouseWheel { unit: MouseWheelUnit::Line, delta: vec2(0.0, dy), phase: TouchPhase::Move, modifiers: Modifiers { ctrl: true, ..Default::default() } }]);
                json!({ "zoom_wheel": dy })
            }
            2 => {
                let text = ["é中🦀", "**bold** ||spoiler||", "<t:0:R>", "@al", "```rust\nlet x = 1;\n```", "/", "\n\n", "[x](https://example.test)"][next() % 8];
                self.queue.push_back(vec![Event::Text(text.into())]);
                json!({ "text": text })
            }
            3 => {
                let keys = [Key::Escape, Key::Tab, Key::Backspace, Key::Enter, Key::ArrowDown, Key::ArrowUp, Key::PageUp, Key::PageDown, Key::Home, Key::End, Key::K, Key::F];
                let i = next() % keys.len();
                let key = keys[i];
                let modifiers = Modifiers { ctrl: i >= 10, shift: i == 1 && next() % 2 == 0, ..Default::default() };
                for pressed in [true, false] {
                    self.queue.push_back(vec![Event::Key { key, physical_key: None, pressed, repeat: false, modifiers }]);
                }
                json!({ "key": format!("{key:?}"), "ctrl": modifiers.ctrl, "shift": modifiers.shift })
            }
            4 => {
                let dy = if next() % 2 == 0 { 240.0 } else { -240.0 };
                self.scroll_by(screen.center(), dy);
                json!({ "scroll": dy })
            }
            _ => {
                let mut targets: Vec<_> = controls.iter().filter(|(k, r)| {
                    screen.contains(r.center()) && explore_allowed(k)
                        && (matches!(k.as_str(), "gear" | "friends" | "composer" | "search" | "pins" | "inbox" | "emoji" | "gif")
                        || ["control:", "settings:", "guild:", "chan:", "dm:", "msg:", "profile:", "profed:", "friend:", "composer:", "menu:", "thread:", "forum:"].iter().any(|p| k.starts_with(p)))
                }).map(|(k, r)| (k.clone(), *r)).collect();
                targets.sort_by(|a, b| a.0.cmp(&b.0));
                let method = if kind == 7 { "right" } else if kind == 8 { "hover" } else if kind == 9 { "middle" } else { "click" };
                let names: Vec<_> = targets.iter().map(|(name, _)| name.clone()).collect();
                let visits = self.rects.entry("stress_visits").or_insert(json!({})).as_object_mut().unwrap();
                let picked = explore_pick(&names, visits, &state, method, next());
                if let Some((name, r)) = picked.and_then(|i| targets.get(i)) {
                    let key = format!("{state}|{method}|{name}");
                    let count = visits.get(&key).and_then(|n| n.as_u64()).unwrap_or(0);
                    visits.insert(key, json!(count + 1));
                    if kind == 7 {
                        self.rclick(r.center());
                    } else if kind == 8 {
                        self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                    } else if kind == 9 {
                        self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                        for pressed in [true, false] {
                            self.queue.push_back(vec![Event::PointerButton { pos: r.center(), button: egui::PointerButton::Middle, pressed, modifiers: Modifiers::NONE }]);
                        }
                    } else {
                        self.click(r.center());
                    }
                    json!({ "target": name, "method": method, "at": [r.center().x, r.center().y] })
                } else {
                    let at = pos2(screen.left() + next() as f32 % screen.width(), screen.top() + next() as f32 % screen.height());
                    self.queue.push_back(vec![Event::PointerMoved(at)]);
                    json!({ "hover": [at.x, at.y] })
                }
            }
        };
        let delay_ms = [0, 16, 50, 100, 250, 750, 2000][next() % 7];
        if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("actions.jsonl")) {
            let _ = writeln!(log, "{}", json!({ "case": case, "seed": seed, "action": action, "delay_ms": delay_ms, "before": state, "previous_state": previous, "visible": visible }));
        }
        self.rects.insert("stress_previous_state".into(), json!(state));
        self.rects.insert("stress_rng".into(), json!(rng));
        if fresh || case % 25 == 0 {
            self.shot(ctx, &format!("stress_{case:04}.png"));
        }
        self.next(delay_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exploration_favors_unvisited_actions_in_each_state() {
        let names = vec!["gear".to_owned(), "friends".to_owned()];
        let mut visits = serde_json::Map::new();
        visits.insert("chat|click|gear".into(), json!(3));
        assert_eq!(explore_pick(&names, &visits, "chat", "click", 0), Some(1));
        assert_eq!(explore_pick(&names, &visits, "chat", "right", 0), Some(0));
        assert_eq!(explore_pick(&names, &visits, "settings", "click", 0), Some(0));
        assert_eq!(explore_pick(&[], &visits, "chat", "click", 0), None);
    }

    #[test]
    fn exploration_ties_are_seeded_and_replayable() {
        let names = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let visits = serde_json::Map::new();
        assert_eq!(explore_pick(&names, &visits, "chat", "click", 5), Some(2));
        assert_eq!(explore_pick(&names, &visits, "chat", "click", 5), Some(2));
    }

    #[test]
    fn exploration_leaves_host_settings_and_external_dialogs_out() {
        assert!(explore_allowed("control:button:Save"));
        assert!(explore_allowed("control:profile:Edit"));
        assert!(!explore_allowed("control:toggle:Automatically open Rustcord when your computer starts up"));
        assert!(!explore_allowed("control:button:Upload image"));
        assert!(!explore_allowed("control:button:Checkout"));
    }

    #[test]
    fn exploration_control_registry_keeps_only_current_pass() {
        let mut controls = Controls::default();
        let r = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(20.0, 20.0));
        controls.insert(1, "button:Save", r);
        controls.insert(1, "toggle:Mute", r);
        assert_eq!(controls.items.len(), 2);
        controls.insert(2, "button:Close", r);
        assert_eq!(controls.items.len(), 1);
        assert!(controls.items.contains_key("control:button:Close"));
    }
}

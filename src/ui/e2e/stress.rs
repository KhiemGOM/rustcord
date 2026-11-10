use std::io::Write;
use egui::{Event, Key, ViewportCommand, pos2, vec2};
use serde_json::json;

use super::Run;
use crate::ui::App;

impl Run {
    pub(super) fn stress_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let case = self.step - 9000;
        let seed = std::env::var("RUSTCORD_STRESS_SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(1).max(1);
        let limit = std::env::var("RUSTCORD_STRESS_ACTIONS").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(300).clamp(1, 800);
        if case == limit {
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
        let kind = next() % 8;
        let screen = ctx.content_rect();
        let action = match kind {
            0 => {
                let w = [640.0, 800.0, 940.0, 1280.0, 1600.0][next() % 5];
                let h = [500.0, 720.0, 900.0][next() % 3];
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(w, h)));
                json!({ "resize": [w, h] })
            }
            1 => {
                app.settings.zoom = [0.75, 1.0, 1.25, 1.5, 2.0][next() % 5];
                json!({ "zoom": app.settings.zoom })
            }
            2 => {
                let text = ["é中🦀", "**bold** ||spoiler||", "<t:0:R>", "@al", "```rust\nlet x = 1;\n```", "/", "\n\n", "[x](https://example.test)"][next() % 8];
                self.queue.push_back(vec![Event::Text(text.into())]);
                json!({ "text": text })
            }
            3 => {
                let key = [Key::Escape, Key::Tab, Key::Backspace, Key::Enter, Key::ArrowDown, Key::ArrowUp][next() % 6];
                self.key(key);
                json!({ "key": format!("{key:?}") })
            }
            4 => {
                let dy = if next() % 2 == 0 { 240.0 } else { -240.0 };
                self.scroll_by(screen.center(), dy);
                json!({ "scroll": dy })
            }
            _ => {
                let mut targets: Vec<_> = app.rects.iter().filter(|(k, r)| {
                    screen.contains(r.center()) && (matches!(k.as_str(), "gear" | "friends" | "composer" | "search" | "pins" | "inbox" | "emoji" | "gif")
                        || k.starts_with("settings:") && k.as_str() != "settings:Log Out"
                        || ["guild:", "chan:", "dm:", "msg:"].iter().any(|p| k.starts_with(p)))
                }).map(|(k, r)| (k.clone(), *r)).collect();
                targets.sort_by(|a, b| a.0.cmp(&b.0));
                if let Some((name, r)) = targets.get(next() % targets.len().max(1)) {
                    if kind == 7 {
                        self.rclick(r.center());
                    } else {
                        self.click(r.center());
                    }
                    json!({ "target": name, "right": kind == 7, "at": [r.center().x, r.center().y] })
                } else {
                    let at = pos2(screen.left() + next() as f32 % screen.width(), screen.top() + next() as f32 % screen.height());
                    self.queue.push_back(vec![Event::PointerMoved(at)]);
                    json!({ "hover": [at.x, at.y] })
                }
            }
        };
        if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("actions.jsonl")) {
            let _ = writeln!(log, "{}", json!({ "case": case, "seed": seed, "action": action }));
        }
        self.rects.insert("stress_rng".into(), json!(rng));
        if case % 25 == 0 {
            self.shot(ctx, &format!("stress_{case:04}.png"));
        }
        self.next(100);
    }
}

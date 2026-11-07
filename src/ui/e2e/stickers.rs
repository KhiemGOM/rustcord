//! Lottie stickers (format 3) from the mock's CDN: drawn in chat and in the picker, the pack
//! popout a click on one opens, and that nothing is drawn once they're off the screen. Steps 4320
//! and up, in the chat-extras channel.

use super::*;

const CH: Id = 2085;
const PACK_STICKER: &str = "8103.json";
const TILE: &str = "sticker:8103:2085";

impl Run {
    pub(super) fn sticker_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |key: &str| Self::at(ctx, key);
        match self.step {
            4320 => {
                app.rects.remove(TILE);
                let row = app.store.msgs.get(&CH).and_then(|e| e.list.iter().find(|m| m.content == "a sticker from a default pack")).map(|m| m.id);
                match row {
                    Some(id) => app.jump_to(CH, id),
                    None => app.open(CH),
                }
                self.next(1200);
            }
            4321 => {
                let (tile, frames) = (app.rects.get(TILE).is_some(), crate::gif::shown(PACK_STICKER));
                if !tile || frames < 4 {
                    if self.step_at.elapsed() > Duration::from_secs(12) {
                        self.shot(ctx, "lottie_fail.png");
                        let why = if frames < 4 {
                            format!("the Lottie sticker drew {frames} frames in 12s")
                        } else {
                            format!("the Lottie sticker played ({frames} frames) but its tile was never marked: channel {:?}, scroll {:?}", app.cur(), app.offsets.get(&CH))
                        };
                        self.fail(why);
                    }
                    return;
                }
                self.mark("lottie_chat_plays");
                self.shot(ctx, "lottie_chat.png");
                self.next(400);
            }
            4322 => {
                let Some(r) = app.rects.get(TILE).copied() else { return };
                self.click(r.center());
                self.next(600);
            }
            4323 => {
                let Some(more) = at("source_pop:more") else { return };
                if app.source_pop.as_ref().is_some_and(|p| p.id == 8103) {
                    self.mark("lottie_pack_popout");
                }
                self.shot(ctx, "lottie_pack_popout.png");
                self.click(more.center());
                self.next(700);
            }
            4324 => {
                let Some(r) = at("sticker:8103") else { return };
                if app.stickers.is_none() || crate::gif::shown(PACK_STICKER) < 4 {
                    return;
                }
                self.mark("lottie_picker");
                self.rects.insert("lottie_picker_tile".into(), json!([r.min.x, r.min.y, r.width(), r.height()]));
                self.shot(ctx, "lottie_picker.png");
                self.key(Key::Escape);
                self.next(500);
            }
            4325 => {
                if app.stickers.is_some() {
                    return;
                }
                app.open(2012);
                self.next(600);
                self.rects.insert("lottie_shown_before_idle".into(), json!(crate::gif::shown(PACK_STICKER)));
            }
            4326 => self.next(1500),
            4327 => {
                let before = self.rects.get("lottie_shown_before_idle").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if crate::gif::shown(PACK_STICKER) == before {
                    self.mark("lottie_idle_off_screen");
                }
                self.next(300);
            }
            _ => self.step = 4340,
        }
    }
}

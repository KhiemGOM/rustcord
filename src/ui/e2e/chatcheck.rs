//! Things the P1 round only read about, driven for real in the general channel: the emoji
//! picker's Copy Emoji Image Link, Escape emptying the search box, the spoiler button's tooltip,
//! the /gif and /sticker texts and the thread card with a long last message. Steps 4340 and up.

use super::*;

const COPIED: &str = "not copied yet";

impl Run {
    pub(super) fn tooltip_near(ctx: &egui::Context, r: egui::Rect) -> bool {
        ctx.memory(|m| {
            m.layer_ids().filter(|l| l.order == egui::Order::Tooltip && m.areas().is_visible(l)).filter_map(|l| m.area_rect(l.id)).any(|t| t.expand(8.0).intersects(r) && t.height() < 80.0)
        })
    }

    pub(super) fn chat_check_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        match self.step {
            4340 => {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(COPIED);
                }
                app.open(general);
                self.next(800);
            }
            4341 => {
                self.chord(Key::E, Modifiers::COMMAND);
                self.next(600);
            }
            4342 => {
                if app.picker.is_none() {
                    return;
                }
                self.queue.push_back(vec![Event::Text("ferris".into())]);
                self.next(600);
            }
            4343 => {
                let Some(r) = Self::at(ctx, "emoji0").filter(|_| app.picker.is_some()) else { return };
                self.rclick(r.center());
                self.next(500);
            }
            4344 => {
                let Some(row) = Self::at(ctx, "menu_row:Copy Emoji Image Link") else { return };
                self.shot(ctx, "emoji_image_link_menu.png");
                self.click(row.center());
                self.next(500);
            }
            4345 => {
                let want = format!("{}/emojis/5000.webp?size=32", crate::api::cdn());
                if arboard::Clipboard::new().and_then(|mut c| c.get_text()).is_ok_and(|t| t == want) {
                    self.mark("copy_emoji_image_link");
                }
                app.picker = None;
                self.next(300);
            }
            4346 => {
                let Some(r) = app.rects.get("search").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("coffee".into())]);
                self.key(Key::Enter);
                self.next(0);
            }
            4347 => {
                if !app.search.as_ref().is_some_and(|s| s.results.is_some()) {
                    return;
                }
                self.shot(ctx, "search_before_escape.png");
                self.key(Key::Escape);
                self.next(600);
            }
            4348 => {
                let typed = ctx.data(|d| d.get_temp::<String>(egui::Id::new("search_q"))).unwrap_or_default();
                if app.search.is_none() && typed.is_empty() {
                    self.mark("escape_clears_search");
                }
                self.shot(ctx, "search_after_escape.png");
                self.next(300);
            }
            4349 => {
                let img = image::RgbaImage::from_fn(160, 90, |x, y| image::Rgba([(x * 255 / 160) as u8, (y * 255 / 90) as u8, 200, 255]));
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_image(arboard::ImageData { width: 160, height: 90, bytes: img.into_raw().into() });
                }
                app.paste_files();
                self.next(900);
            }
            4350 => {
                let Some(b) = Self::at(ctx, "card:0:0").filter(|_| !app.files.is_empty()) else { return };
                self.queue.push_back(vec![Event::PointerMoved(b.center() + vec2(1.0, 0.0))]);
                self.queue.push_back(vec![Event::PointerMoved(b.center())]);
                self.next(1100);
            }
            4351 => {
                let Some(b) = Self::at(ctx, "card:0:0") else { return };
                if Self::said(ctx, "card_tip").as_deref() == Some("Spoiler Attachment") && Self::tooltip_near(ctx, b) {
                    self.mark("spoiler_tip_off");
                }
                self.shot(ctx, "spoiler_tip_off.png");
                self.click(b.center());
                self.next(400);
            }
            4352 => {
                let Some(b) = Self::at(ctx, "card:0:0") else { return };
                if !app.files.first().is_some_and(|f| f.spoiler) {
                    return;
                }
                self.queue.push_back(vec![Event::PointerMoved(b.center() + vec2(1.0, 0.0))]);
                self.queue.push_back(vec![Event::PointerMoved(b.center())]);
                self.next(1100);
            }
            4353 => {
                let Some(b) = Self::at(ctx, "card:0:0") else { return };
                if Self::said(ctx, "card_tip").as_deref() == Some("Spoiler Attachment") && Self::tooltip_near(ctx, b) {
                    self.mark("spoiler_tip_on");
                }
                self.shot(ctx, "spoiler_tip_on.png");
                let Some(x) = Self::at(ctx, "card:0:2") else { return };
                self.click(x.center());
                self.next(500);
            }
            4354 => {
                if !app.files.is_empty() {
                    return;
                }
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("/gif".into())]);
                self.next(700);
            }
            4355 => {
                if app.ac_open && Self::said(ctx, "builtin:gif").as_deref() == Some("Search Animated GIFs on the Web") {
                    self.mark("gif_command_text");
                }
                self.shot(ctx, "command_gif.png");
                self.chord(Key::A, Modifiers::COMMAND);
                self.key(Key::Backspace);
                self.queue.push_back(vec![Event::Text("/sticker".into())]);
                self.next(700);
            }
            4356 => {
                if app.ac_open && Self::said(ctx, "builtin:sticker").as_deref() == Some("Search your stickers") {
                    self.mark("sticker_command_text");
                }
                self.shot(ctx, "command_sticker.png");
                self.chord(Key::A, Modifiers::COMMAND);
                self.key(Key::Backspace);
                self.next(500);
            }
            4357 => {
                let Some(id) = app.store.msgs.get(&general).and_then(|e| e.list.iter().find(|m| m.thread.is_some())).map(|m| m.id) else { return };
                app.jump_to(general, id);
                self.next(900);
            }
            4358 => {
                let Some(card) = app.store.msgs.get(&general).and_then(|e| e.list.iter().find_map(|m| m.thread)).and_then(|t| Self::at(ctx, &format!("thread_card:{t}"))) else { return };
                self.rects.insert("thread_card".into(), json!([card.min.x, card.min.y, card.width(), card.height()]));
                self.shot(ctx, "thread_card_short.png");
                self.click(card.center());
                self.next(900);
            }
            _ => self.step = 4360,
        }
    }
}

//! Steps 4600 to 4649: what P3's settings rows really do. The shop's own pointer, the legacy chat
//! input, billing address editing, and the profile's leftovers. `python e2e/from_step.py 4600` runs
//! just this group.

use std::time::Duration;

use egui::{Event, ViewportCommand, pos2, vec2};

use super::Run;
use crate::cursor;
use crate::ui::App;
use crate::ui::shop::Page;

impl Run {
    pub(super) fn p3b_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |k: &str| Self::at(ctx, k);
        let stuck = self.step_at.elapsed() > Duration::from_secs(12);
        match self.step {
            4600 => {
                if app.conn != crate::ui::Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.settings_page = None;
                app.profed = None;
                app.settings.shop_cursor = true;
                app.select_guild(None);
                app.open_shop(Page::Shop);
                self.next(1200);
            }
            4601 => {
                let Some(r) = at("shop:tab:Browse") else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(500);
            }
            4602 => {
                let Some(tab) = at("shop:tab:Browse") else { return };
                if cursor::mode() != 2 {
                    if stuck {
                        self.fail(format!("the pointer over a shop tab was mode {}, not the hand", cursor::mode()));
                    }
                    return;
                }
                self.mark("shop_cursor_hand");
                if cursor::probe() == cursor::handle(cursor::POINTER) {
                    self.mark("shop_cursor_hand_applied");
                }
                self.queue.push_back(vec![Event::PointerMoved(pos2(ctx.content_rect().right() - 6.0, tab.bottom() + 6.0))]);
                self.next(500);
            }
            4603 => {
                if cursor::mode() != 1 {
                    if stuck {
                        self.fail(format!("the pointer over bare shop was mode {}, not the arrow", cursor::mode()));
                    }
                    return;
                }
                self.mark("shop_cursor_arrow");
                if cursor::probe() == cursor::handle(cursor::ARROW) {
                    self.mark("shop_cursor_arrow_applied");
                }
                app.settings.shop_cursor = false;
                self.next(500);
            }
            4604 => {
                if cursor::mode() != 0 {
                    return;
                }
                if cursor::probe() != cursor::handle(cursor::ARROW) && cursor::probe() != cursor::handle(cursor::POINTER) {
                    self.mark("shop_cursor_off");
                }
                app.settings.shop_cursor = true;
                app.open_shop(Page::Nitro);
                self.next(500);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 },
        }
    }
}

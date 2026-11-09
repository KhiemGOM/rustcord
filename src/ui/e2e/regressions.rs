use egui::ViewportCommand;
use serde_json::json;

use super::Run;
use crate::ui::App;

impl Run {
    pub(super) fn fullscreen_edges(&mut self) -> Result<(), String> {
        use std::sync::atomic::Ordering::Relaxed;
        use windows_sys::Win32::Foundation::RECT;
        use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, HTCLIENT, SendMessageW, WM_NCHITTEST};

        let hwnd = crate::winsys::WINDOW.load(Relaxed) as _;
        let mut r: RECT = unsafe { std::mem::zeroed() };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if unsafe { GetWindowRect(hwnd, &mut r) } == 0 || unsafe { GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) } == 0 {
            return Err("could not read fullscreen bounds".into());
        }
        let bounds = |r: RECT| [r.left, r.top, r.right, r.bottom];
        let hits: Vec<isize> = [(r.left + 1, r.top + 1), (r.right - 2, r.top + 1), (r.left + 1, r.bottom - 2), (r.right - 2, r.bottom - 2), ((r.left + r.right) / 2, r.top + 1)].into_iter().map(|(x, y)| {
            let point = (x as u16 as u32 | (y as u16 as u32) << 16) as isize;
            unsafe { SendMessageW(hwnd, WM_NCHITTEST, 0, point) }
        }).collect();
        self.rects.insert("fullscreen_native".into(), json!({ "window": bounds(r), "monitor": bounds(info.rcMonitor), "hits": hits }));
        if bounds(r) != bounds(info.rcMonitor) || hits.iter().any(|h| *h != HTCLIENT as isize) {
            return Err(format!("fullscreen bounds or resize edges differ: {}", self.rects["fullscreen_native"]));
        }
        self.mark("fullscreen_locked");
        Ok(())
    }

    pub(super) fn regression_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        match self.step {
            8800 => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(1280.0, 800.0)));
                app.select_guild(Some(1003));
                self.next(700);
            }
            8801 => {
                let Some(r) = app.rects.get("chan:2302").copied() else { return };
                self.rclick(r.center());
                self.next(300);
            }
            8802 => {
                let Some(r) = Self::at(ctx, "menu_row:Pin Channel to Top") else { return };
                self.shot(ctx, "pin_menu.png");
                self.click(r.center());
                self.next(800);
            }
            8803 => {
                if !app.rects.contains_key("side:pinned") {
                    return;
                }
                self.mark("channel_pinned_by_click");
                self.shot(ctx, "channel_pinned.png");
                app.rest("POST", "/mock/event".into(), Some(json!({ "t": "USER_GUILD_SETTINGS_UPDATE", "d": {
                    "guild_id": "1003", "channel_overrides": [{ "channel_id": "2302", "muted": false }]
                }})), crate::net::Tag::Quiet);
                self.next(800);
            }
            8804 => {
                if app.store.chan_flags.get(&2302).is_none_or(|f| f & crate::ui::PINNED == 0) {
                    self.shot(ctx, "pin_lost_after_update.png");
                    return self.fail("a notification-only settings update unpinned the channel".into());
                }
                self.mark("channel_pin_survives_update");
                let Some(r) = app.rects.get("chan:2302").copied() else { return };
                self.rclick(r.center());
                self.next(300);
            }
            8805 => {
                let Some(r) = Self::at(ctx, "menu_row:Unpin Channel") else { return };
                self.click(r.center());
                self.next(800);
            }
            8806 => {
                if app.rects.contains_key("side:pinned") {
                    return;
                }
                self.mark("channel_unpinned_by_click");
                self.step = 8820;
            }
            8820 => {
                app.select_guild(Some(self.target.0));
                app.open(self.target.1);
                self.next(800);
            }
            8821 | 8827 => {
                let ch = self.target.1;
                app.scroll_fix = Some((ch, 1400.0));
                app.pinned.remove(&ch);
                self.next(600);
            }
            8822 | 8828 => {
                let ch = self.target.1;
                let Some(row) = app.rects.get("msg:last").copied() else { return };
                let Some(entry) = app.store.msgs.get(&ch) else { return };
                let found = entry.list.iter().find(|m| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("row", m.id)))) == Some(row));
                let Some(m) = found else { return };
                self.rects.insert("read_anchor".into(), json!([m.id, row.top()]));
                self.loaded = entry.list.len();
                let Some(first) = entry.list.first() else { return };
                app.net.send(crate::net::Cmd::Messages { channel: ch, page: crate::state::Page::Before(first.id) });
                self.next(1000);
            }
            8823 | 8829 => {
                let ch = self.target.1;
                if app.store.msgs.get(&ch).is_none_or(|e| e.list.len() <= self.loaded) {
                    return;
                }
                let anchor = &self.rects["read_anchor"];
                let id = anchor[0].as_u64().unwrap();
                let y = anchor[1].as_f64().unwrap() as f32;
                let row = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("row", id))));
                if let Some(row) = row {
                    self.rects.insert("history_anchor_shift".into(), json!(row.top() - y));
                }
                self.shot(ctx, "read_anchor_after_history.png");
                if row.is_none_or(|r| (r.top() - y).abs() > 2.0) {
                    return self.fail(format!("history moved the reading anchor from {y} to {row:?}"));
                }
                if self.step == 8823 {
                    self.next(300);
                } else {
                    self.mark("history_kept_reading_anchor");
                    self.step = 8850;
                }
            }
            8824 => {
                app.scroll_fix = Some((self.target.1, 900.0));
                self.next(600);
            }
            8825 => {
                app.net.send(crate::net::Cmd::Messages { channel: self.target.1, page: crate::state::Page::Latest });
                self.next(800);
                self.step = 8827;
            }
            8850 => {
                app.open_shop(crate::ui::shop::Page::Shop);
                self.next(800);
            }
            8851 => {
                let Some(r) = Self::at(ctx, "shop:tab:Orbs Exclusives") else { return };
                self.click(r.center());
                self.next(800);
            }
            8852 => {
                let Some(r) = Self::at(ctx, "shop:card:5203") else {
                    self.scroll_by(ctx.content_rect().center(), -200.0);
                    self.wait = std::time::Instant::now() + std::time::Duration::from_millis(200);
                    return;
                };
                if r.bottom() > ctx.content_rect().bottom() - 8.0 || r.top() < 100.0 {
                    self.scroll_by(ctx.content_rect().center(), if r.top() < 100.0 { 200.0 } else { -200.0 });
                    self.wait = std::time::Instant::now() + std::time::Duration::from_millis(200);
                    return;
                }
                self.shot(ctx, "orb_decoration_card.png");
                self.click(r.center());
                self.next(800);
            }
            8853 => {
                let Some(r) = Self::at(ctx, "shop:slide:1") else { return };
                self.click(r.center());
                self.next(800);
            }
            8854 => {
                if app.shop.as_ref().is_none_or(|s| s.slide != 1) {
                    return;
                }
                self.shot(ctx, "orb_decoration_variant.png");
                self.mark("orb_decoration_variant_selected");
                self.next(300);
            }
            8855 => {
                app.shop = None;
                self.step = 8870;
            }
            8870 => {
                app.open_shop(crate::ui::shop::Page::Shop);
                app.rest("POST", "/mock/fail".into(), Some(json!({ "method": "GET", "path": "/shop/search", "status": 200,
                    "body": { "skus": ["5211"], "pagination": { "total": 1, "offset": 0, "limit": 15, "has_more": false } }
                })), crate::net::Tag::Quiet);
                self.next(800);
            }
            8871 => {
                let Some(r) = Self::at(ctx, "shop:search") else { return };
                self.click(r.center());
                self.queue.push_back(vec![egui::Event::Text("blue panda".into())]);
                self.next(800);
            }
            8872 => {
                let Some(r) = Self::at(ctx, "shop:card:5211") else { return };
                self.click(r.center());
                self.next(500);
            }
            8873 => {
                if app.shop.as_ref().is_none_or(|s| s.product.as_deref() != Some("5211")) || Self::at(ctx, "shop:detail_heart").is_none() {
                    return;
                }
                self.shot(ctx, "decoration_variant_search.png");
                self.next(0);
            }
            8874 => {
                if self.pending_shots > 0 {
                    return;
                }
                self.mark("decoration_variant_search");
                app.shop = None;
                self.step = 8890;
            }
            8890 => {
                self.blur = false;
                app.settings.gif_autoplay = true;
                app.select_guild(Some(self.target.0));
                app.open(2060);
                self.next(800);
            }
            8891 => {
                if app.store.msgs.get(&2060).is_none_or(|e| !e.fresh || e.loading) {
                    return;
                }
                let url = std::env::var("RUSTCORD_E2E_MEDIA_PROXY").unwrap_or_else(|_| format!("{}/media/agabounce.webp?animated=true", crate::api::cdn()));
                app.rest("POST", "/mock/event".into(), Some(json!({ "t": "MESSAGE_CREATE", "d": {
                    "id": "2000000000000000000", "channel_id": "2060", "guild_id": self.target.0.to_string(),
                    "author": { "id": "101", "username": "Mira", "discriminator": "0" },
                    "content": "agabounce regression", "type": 0, "attachments": [], "mentions": [], "mention_roles": [],
                    "embeds": [{ "type": "image", "url": url, "thumbnail": { "url": url, "proxy_url": url, "width": 48, "height": 48 } }]
                }})), crate::net::Tag::Quiet);
                app.scroll_fix = Some((2060, 1e9));
                self.next(800);
            }
            8892 => {
                if !app.rects.contains_key("embed_image:2000000000000000000") || crate::gif::shown("agabounce") < 2 {
                    return;
                }
                self.shot(ctx, "webp_t0.png");
                self.next(0);
            }
            8893 => {
                if self.pending_shots == 0 {
                    let url = std::env::var("RUSTCORD_E2E_MEDIA_PROXY").unwrap_or_else(|_| format!("{}/media/agabounce.webp?animated=true", crate::api::cdn()));
                    for loader in ctx.loaders().bytes.lock().iter() {
                        loader.forget(&url);
                    }
                    let frame = crate::gif::uri(ctx, &url, true);
                    if !frame.contains(crate::gif::SEP) || !matches!(ctx.try_load_texture(&frame, Default::default(), Default::default()), Ok(egui::load::TexturePoll::Ready { .. })) {
                        return self.fail("decoded webp became unavailable after byte eviction".into());
                    }
                    self.mark("webp_survives_byte_eviction");
                    self.next(137);
                }
            }
            8894 => {
                self.shot(ctx, "webp_t1.png");
                self.next(0);
            }
            8895 => {
                if self.pending_shots > 0 {
                    return;
                }
                let Some(r) = app.rects.get("embed_image:2000000000000000000").copied() else { return };
                let changed = self.changed("webp_t0.png", "webp_t1.png", r, ctx.pixels_per_point());
                self.rects.insert("webp_pixels_changed".into(), json!(changed));
                if changed == 0 {
                    return self.fail("animated webp embed did not change pixels".into());
                }
                self.mark("webp_embed_animates");
                app.open(self.target.1);
                self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 };
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 3110 },
        }
    }
}

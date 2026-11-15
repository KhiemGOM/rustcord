use egui::{Id, Key, Rect, pos2};
use std::time::Duration;

use super::Run;
use crate::net::Conn;
use crate::state::Id as Chan;
use crate::ui::App;
use crate::ui::community::Page;

const GUILD: Chan = 1001;
const DM: Chan = 3002;

impl Run {
    /// The close-DM X under a steady pointer, Browse Channels turning a fresh server to its
    /// picked list, the upload card buttons clicked at their top edge, discord's sticker packs,
    /// and the member list's activity settings.
    pub(super) fn fixes_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |key: &str| Self::at(ctx, key);
        let row = |label: &str| Self::at(ctx, &format!("menu_row:{}", label));
        let run_id = Id::new(("fixes", "run"));
        let bad_id = Id::new(("fixes", "bad"));
        let x_id = Id::new(("fixes", "x"));
        let general = self.target.1;
        match self.step {
            3000 => {
                if app.conn != Conn::Online {
                    return;
                }
                app.select_guild(None);
                app.open(3000);
                self.next(900);
            }
            3001 => {
                let Some(r) = self.reach_row(app, &format!("chan:{DM}")) else { return };
                self.queue.push_back(vec![egui::Event::PointerMoved(r.center())]);
                self.next(400);
            }
            3002 => {
                let Some(x) = at(&format!("dm_close:{DM}")) else { return };
                ctx.data_mut(|d| {
                    d.insert_temp(x_id, x);
                    d.insert_temp(run_id, 0u32);
                    d.insert_temp(bad_id, 0u32);
                });
                self.queue.push_back(vec![egui::Event::PointerMoved(x.center())]);
                self.next(300);
            }
            3003 => {
                let seen = at(&format!("dm_close:{DM}")).is_some();
                ctx.data_mut(|d| d.remove::<(Rect, u64)>(Id::new(("e2e", format!("dm_close:{DM}")))));
                ctx.data_mut(|d| *d.get_temp_mut_or_default::<u32>(if seen { run_id } else { bad_id }) += 1);
                if self.step_at.elapsed() > Duration::from_millis(1500) {
                    let (run, bad) = ctx.data(|d| (d.get_temp::<u32>(run_id).unwrap_or(0), d.get_temp::<u32>(bad_id).unwrap_or(0)));
                    if bad == 0 && run >= 20 {
                        self.mark("fixes_dm_x_stable");
                    }
                    self.next(0);
                }
            }
            3004 => {
                if let Some(x) = ctx.data(|d| d.get_temp::<Rect>(x_id)) {
                    self.click(x.center());
                }
                self.next(700);
            }
            3005 => {
                if !app.store.dms.contains(&DM) {
                    self.mark("fixes_dm_x_click");
                }
                app.select_guild(Some(GUILD));
                app.open_home(GUILD, Page::Channels(1));
                self.next(900);
            }
            3006 => {
                if !app.rects.contains_key("browse:chan:2101") {
                    return;
                }
                app.store.opt_in.remove(&GUILD);
                let gone: Vec<Chan> = app.store.opted.iter().copied().filter(|c| app.store.channels.get(c).is_some_and(|c| c.guild == GUILD)).collect();
                for c in gone {
                    app.store.opted.remove(&c);
                }
                if let Some(g) = app.store.guilds.get_mut(&GUILD) {
                    g.my_flags = 0;
                }
                self.next(500);
            }
            3007 => {
                let Some(r) = app.rects.get("browse:chan:2101").copied() else { return };
                self.shot(ctx, "fixes_browse_fresh.png");
                self.click(r.center());
                self.next(700);
            }
            3008 => {
                if app.store.opt_in.contains(&GUILD) && !app.store.opted.contains(&2101) && app.store.opted.contains(&2100) && app.cur() != Some(2101) {
                    self.mark("fixes_browse_fresh");
                }
                let Some(r) = app.rects.get("browse:chan:2101").copied() else { return };
                self.queue.push_back(vec![egui::Event::PointerMoved(r.center() - egui::vec2(120.0, 0.0))]);
                self.next(400);
            }
            3009 => {
                let Some(b) = app.rects.get("browse:open:2101").copied() else { return };
                self.shot(ctx, "fixes_browse_hover.png");
                self.click(b.center());
                self.next(700);
            }
            3010 => {
                if app.cur() == Some(2101) && !app.store.opted.contains(&2101) {
                    self.mark("fixes_browse_open");
                }
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(900);
            }
            3011 => {
                for n in ["a", "b"] {
                    let path = self.dir.join(format!("e2e-fix-{n}.txt"));
                    let _ = std::fs::write(&path, n);
                    app.files.push(crate::ui::composer::Upload::new(path));
                }
                self.next(600);
            }
            3012 => {
                let Some(r) = at("card:0:0") else { return };
                self.shot(ctx, "fixes_cards.png");
                self.click(pos2(r.center().x, r.top() + 3.0));
                self.next(400);
            }
            3013 => {
                if app.files.first().is_some_and(|f| f.spoiler) {
                    self.mark("fixes_card_eye");
                }
                let Some(r) = at("card:0:1") else { return };
                self.click(pos2(r.center().x, r.top() + 3.0));
                self.next(500);
            }
            3014 => {
                if app.modify.is_some() {
                    self.mark("fixes_card_pencil");
                }
                app.modify = None;
                let Some(r) = at("card:0:2") else { return };
                self.click(pos2(r.center().x, r.top() + 3.0));
                self.next(500);
            }
            3015 => {
                if app.files.len() == 1 {
                    self.mark("fixes_card_trash");
                }
                app.files.clear();
                app.sticker_q = Some("thumbs".into());
                self.next(600);
            }
            3016 => {
                let Some(r) = app.rects.get("sticker_button").copied() else { return };
                self.click(r.center());
                self.next(1500);
            }
            3017 => {
                let Some(c) = at("sticker:8102") else { return };
                if app.packs.len() == 2 {
                    self.mark("fixes_sticker_packs");
                }
                self.shot(ctx, "fixes_stickers.png");
                self.click(c.center());
                self.next(800);
            }
            3018 => {
                if app.stickers.is_none() {
                    self.mark("fixes_sticker_closed");
                }
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(600);
            }
            3019 => {
                let Some(g) = app.rects.get("activity:gear").copied() else { return };
                self.click(g.center());
                self.next(500);
            }
            3020 => {
                let (Some(_), Some(hide)) = (row("About Recent Activity"), row("Hide Activity Cards")) else { return };
                self.mark("fixes_feed_menu");
                self.shot(ctx, "fixes_feed_menu.png");
                self.click(hide.center());
                self.next(600);
            }
            3021 => {
                let cards = app.rects.keys().any(|k| k.starts_with("activity:card:"));
                if app.settings.hide_activity_cards && !cards {
                    self.mark("fixes_feed_hidden");
                }
                self.shot(ctx, "fixes_feed_hidden.png");
                let Some(h) = app.rects.iter().find(|(k, _)| k.strip_prefix("activity:").is_some_and(|n| n.parse::<u32>().is_ok())).map(|(_, r)| *r) else { return };
                self.click(h.left_center() + egui::vec2(30.0, 0.0));
                self.next(600);
            }
            3022 => {
                let cards = app.rects.keys().any(|k| k.starts_with("activity:card:"));
                if !app.settings.hide_activity_cards && cards {
                    self.mark("fixes_feed_shown");
                }
                let Some(g) = app.rects.get("activity:gear").copied() else { return };
                self.click(g.center());
                self.next(500);
            }
            3023 => {
                let Some(r) = row("About Recent Activity") else { return };
                self.click(r.center());
                self.next(700);
            }
            3024 => {
                if app.feed_about {
                    self.mark("fixes_feed_about");
                }
                self.shot(ctx, "fixes_feed_about.png");
                self.key(Key::Escape);
                self.next(400);
            }
            3025 => {
                if !app.feed_about {
                    self.mark("fixes_feed_about_closed");
                }
                self.step = 3300;
            }
            _ => self.step = 3300,
        }
    }
}

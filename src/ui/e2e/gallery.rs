//! The mock's gallery channel (messages shaped like discord's own): the attachment mosaics, forwards,
//! stickers and provider embeds drawn, then the account's favourites read in and written back.
//! Steps 4000 and up; entered from the end of the clips steps, ending at 99.

use base64::Engine;
use egui::Rect;

use super::*;
use crate::proto::frecency::{self, Gif, Item};
use crate::state::ReactEmoji;

const GALLERY: Id = 2080;
const NOTES: Id = 2081;
const OWNED: Id = 1001;
const GIF: &str = "https://tenor.com/view/e2e-favourite";

// message text, and how many tiles its mosaic has
const MOSAICS: [(&str, usize); 8] = [
    ("two photos", 2),
    ("three photos", 3),
    ("four photos", 4),
    ("five photos", 5),
    ("seven photos", 7),
    ("eleven photos, one more than a message holds", 10),
    ("two photos and a clip", 3),
    ("a clip and a photo", 2),
];

/// Tiles that sit inside the block, never overlap, and are four apart wherever they neighbour.
fn tidy(whole: Rect, tiles: &[Rect]) -> bool {
    let inside = tiles.iter().all(|t| whole.expand(0.6).contains_rect(*t));
    let apart = tiles.iter().enumerate().all(|(i, a)| {
        tiles.iter().skip(i + 1).all(|b| {
            let (x, y) = (a.intersect(*b).width(), a.intersect(*b).height());
            let overlap = a.expand(-0.5).intersects(b.expand(-0.5));
            let beside = |gap: f32| (gap - 4.0).abs() < 0.8 || gap.abs() > 6.0;
            !overlap && (x < 1.0 || beside((a.top() - b.bottom()).max(b.top() - a.bottom()))) && (y < 1.0 || beside((a.left() - b.right()).max(b.left() - a.right())))
        })
    });
    inside && apart && whole.width() <= 550.6
}

fn patch(cdn: &str) -> Vec<u8> {
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let hot = |total: u32| Item { total, recent: vec![now - 1000, now - 60_000], frecency: -1, score: 0 };
    let gifs = [Gif { url: GIF.into(), src: format!("{cdn}/media/gif1.png"), w: 220, h: 160, order: 1 }];
    let mut out = frecency::gifs_field(&gifs);
    out.extend(frecency::stickers_field(&[749054660769218631]));
    out.extend(frecency::emojis_field(&["5000".into(), "smile".into()]));
    let mut emoji_use = std::collections::BTreeMap::new();
    emoji_use.insert("5000".to_string(), hot(40));
    emoji_use.insert("fire".to_string(), hot(3));
    out.extend(frecency::emoji_use_field(&emoji_use));
    out
}


/// Every picture the channel's embeds carry is decoded, so a screenshot shows them and not their placeholders.
fn settled(run: &Run, app: &App, ctx: &egui::Context) -> bool {
    let urls = app.store.msgs.get(&GALLERY).into_iter().flat_map(|e| e.list.iter()).flat_map(|m| m.embeds.iter()).flat_map(|e| [e.thumb.as_ref(), e.image.as_ref()]).flatten();
    let all = urls.map(|m| ctx.try_load_texture(&m.url, egui::TextureOptions::default(), egui::load::SizeHint::default())).all(|t| matches!(t, Ok(egui::load::TexturePoll::Ready { .. })));
    run.step_at.elapsed() >= Duration::from_millis(1800) && (all || run.step_at.elapsed() > Duration::from_secs(10))
}

impl Run {
    pub(super) fn gallery_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let find = |app: &App, text: &str| app.store.msgs.get(&GALLERY).and_then(|e| e.list.iter().find(|m| m.content == text)).map(|m| m.id);
        match self.step {
            4000 => {
                if app.conn != Conn::Online {
                    return;
                }
                app.select_guild(Some(self.target.0));
                app.open(GALLERY);
                self.sub = 0;
                self.next(800);
            }
            4001 => {
                let Some(e) = app.store.msgs.get(&GALLERY).filter(|e| e.fresh && !e.loading && e.list.len() > 20) else { return };
                let own = e.list.iter().any(|m| m.kind == 21) || app.store.msgs.contains_key(&2081);
                self.rects.insert("gallery_messages".into(), json!(e.list.len()));
                self.rects.insert("gallery_thread_known".into(), json!(own));
                self.mark("gallery_loaded");
                self.next(300);
            }
            // each mosaic scrolled into view and measured
            4002 => {
                let Some((text, _)) = MOSAICS.get(self.sub as usize) else {
                    self.step = 4010;
                    return;
                };
                let Some(id) = find(app, text) else { return self.fail(format!("no gallery message {text:?}")) };
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4003 => {
                let (text, n) = MOSAICS[self.sub as usize];
                let Some(id) = find(app, text) else { return };
                let Some(whole) = app.rects.get(&format!("mosaic:{id}:{GALLERY}")).copied() else { return };
                let tiles: Vec<Rect> = (0..n).filter_map(|i| app.rects.get(&format!("mosaic:{id}:{i}:{GALLERY}")).copied()).collect();
                let extra = app.rects.contains_key(&format!("mosaic:{id}:{n}:{GALLERY}"));
                if tiles.len() == n && !extra && tidy(whole, &tiles) {
                    self.mark(&format!("mosaic_{}", self.sub));
                }
                self.rects.insert(format!("mosaic_{}_tiles", self.sub), json!(tiles.iter().map(|t| [t.min.x, t.min.y, t.width(), t.height()]).collect::<Vec<_>>()));
                self.rects.insert(format!("mosaic_{}_block", self.sub), json!([whole.min.x, whole.min.y, whole.width(), whole.height()]));
                self.shot(ctx, &format!("mosaic_{}.png", self.sub));
                self.sub += 1;
                self.step = 4002;
                self.wait = Instant::now() + Duration::from_millis(500);
            }
            // a forward shows the message it carries, a sticker its picture
            4010 => {
                let forwarded = app.store.msgs.get(&GALLERY).map(|e| e.list.iter().filter(|m| m.forward.is_some()).collect::<Vec<_>>()).unwrap_or_default();
                if forwarded.len() == 3 && forwarded[0].content.starts_with("cold start") && forwarded[1].attachments.len() == 1 && forwarded[2].attachments.len() == 2 {
                    self.mark("forward_parsed");
                }
                let id = forwarded.first().map(|m| m.id).unwrap_or(0);
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4011 => {
                if !settled(self, app, ctx) {
                    return;
                }
                self.shot(ctx, "forward.png");
                let stickers = app.store.msgs.get(&GALLERY).map(|e| e.list.iter().filter(|m| !m.stickers.is_empty()).count()).unwrap_or(0);
                if stickers == 4 {
                    self.mark("stickers_parsed");
                }
                let Some(id) = find(app, "a gif sticker with words") else { return };
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4012 => {
                if !settled(self, app, ctx) {
                    return;
                }
                self.shot(ctx, "stickers.png");
                let Some(id) = find(app, "Pick a deploy target") else { return };
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4013 => {
                if !settled(self, app, ctx) {
                    return;
                }
                self.shot(ctx, "components.png");
                let Some(id) = app.store.msgs.get(&GALLERY).and_then(|e| e.list.iter().find(|m| m.flags & (1 << 15) != 0)).map(|m| m.id) else { return };
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4014 => {
                if !settled(self, app, ctx) {
                    return;
                }
                self.shot(ctx, "components_v2.png");
                let Some(id) = find(app, "https://www.youtube.com/watch?v=dQw4w9WgXcQ") else { return };
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4015 => {
                if !settled(self, app, ctx) {
                    return;
                }
                self.shot(ctx, "embeds.png");
                let Some(id) = find(app, "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT") else { return };
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4016 => {
                if !settled(self, app, ctx) {
                    return;
                }
                self.shot(ctx, "embeds_2.png");
                let Some(id) = find(app, "starting a thread on this one") else { return };
                app.jump_to(GALLERY, id);
                self.next(900);
            }
            4017 => {
                if !settled(self, app, ctx) {
                    return;
                }
                let urls: Vec<String> = app.store.msgs.get(&GALLERY).into_iter().flat_map(|e| e.list.iter()).flat_map(|m| m.embeds.iter()).flat_map(|e| [e.thumb.as_ref(), e.image.as_ref()]).flatten().map(|m| m.url.clone()).collect();
                let state: Vec<String> = urls
                    .iter()
                    .map(|u| match ctx.try_load_texture(u, egui::TextureOptions::default(), egui::load::SizeHint::default()) {
                        Ok(egui::load::TexturePoll::Ready { .. }) => format!("ready {u}"),
                        Ok(_) => format!("pending {u}"),
                        Err(e) => format!("{e} {u}"),
                    })
                    .collect();
                self.rects.insert("embed_images".into(), json!(state));
                self.shot(ctx, "thread_and_polls.png");
                app.open(NOTES);
                self.next(1500);
            }
            4018 => {
                self.shot(ctx, "thread_starter.png");
                let Some(m) = app.store.msgs.get(&NOTES).and_then(|e| e.list.first()) else { return };
                if m.kind == 21 && m.reply.is_some() {
                    self.mark("thread_starter_parsed");
                }
                self.next(800);
            }
            4019 => {
                app.open(GALLERY);
                app.jump_to_present(GALLERY);
                self.step = 4020;
                self.wait = Instant::now() + Duration::from_millis(500);
            }
            // another client of the account stars things: they arrive without a restart
            4020 => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(patch(crate::api::cdn()));
                app.rest("POST", "/mock/frecency".into(), Some(json!({ "settings": b64 })), crate::net::Tag::Quiet);
                self.next(0);
            }
            4021 => {
                let got = app.settings.fav_gifs.iter().any(|g| g.0 == GIF) && app.ui.fav_stickers.contains(&749054660769218631);
                let emoji = app.settings.fav_emoji.iter().any(|e| e.id == 5000);
                if got && emoji && app.ui.frequent.first().is_some_and(|e| e.id == 5000) {
                    self.mark("favs_from_account");
                    self.next(0);
                }
            }
            // and what is starred here goes back
            4022 => {
                app.settings.fav_gifs.retain(|g| g.0 != GIF);
                app.settings.fav_emoji.push(ReactEmoji { id: 0, name: "🔥".into(), animated: false });
                app.used(ReactEmoji { id: 0, name: "🔥".into(), animated: false });
                self.next(0);
            }
            4023 => {
                if app.favs_settled() && self.step_at.elapsed() > Duration::from_secs(6) {
                    if !app.settings.fav_gifs.iter().any(|g| g.0 == GIF) && app.settings.fav_emoji.iter().any(|e| e.name == "🔥") {
                        self.mark("favs_written");
                    }
                    self.next(0);
                }
            }
            // a server the mock user owns: a member list of its own, and the owner's menu
            4024 => {
                app.select_guild(Some(OWNED));
                let Some(c) = app.store.guilds.get(&OWNED).and_then(|g| g.channels.first().copied()) else { return };
                app.open(c);
                self.next(1000);
            }
            4025 => {
                let people = app.store.guilds.get(&OWNED).map_or(0, |g| g.list.iter().filter(|i| matches!(i, crate::state::ListItem::Member(_))).count());
                if people < 3 {
                    return;
                }
                self.rects.insert("owned_members".into(), json!(people));
                self.mark("members_filled");
                self.shot(ctx, "owned_members.png");
                let Some(r) = app.rects.get("guildmenu").copied() else { return };
                self.click(r.center());
                self.next(700);
            }
            4026 => {
                self.shot(ctx, "owned_menu.png");
                self.key(Key::Escape);
                self.next(400);
            }
            _ => self.step = 99,
        }
    }
}

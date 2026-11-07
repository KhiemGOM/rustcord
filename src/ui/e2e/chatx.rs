//! Invite cards, folded blocked messages, poll voters, the emoji and sticker popouts, Copy Image,
//! the @silent mark, Custom reminders, the top emoji and large uploads, in steps 3700 and up. They
//! run in the mock's chat-extras channel and end the main chain.

use super::*;
use crate::ui::Pick;

const CH: Id = 2085;

impl Run {
    pub(super) fn chatx_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |key: &str| Self::at(ctx, key);
        let id_of = |app: &App, text: &str| app.store.msgs.get(&CH).and_then(|e| e.list.iter().find(|m| m.content.contains(text))).map(|m| m.id);
        let poll_id = |app: &App| app.store.msgs.get(&CH).and_then(|e| e.list.iter().find(|m| m.poll.is_some())).map(|m| m.id);
        let ready = |app: &App| app.cur() == Some(CH) && app.store.msgs.get(&CH).is_some_and(|e| e.list.len() >= 11 && e.fresh && !e.loading);
        let count = |app: &App, text: &str| app.store.msgs.get(&CH).map_or(0, |e| e.list.iter().filter(|m| !m.pending && m.content == text).count());
        let pending = |app: &App| app.store.msgs.get(&CH).and_then(|e| e.list.iter().find(|m| m.pending)).and_then(|m| m.nonce.clone());
        let files = self.dir.join("chatx_files");
        match self.step {
            3700 => {
                if app.conn != Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.select_guild(Some(self.target.0));
                app.open(CH);
                self.next(900);
            }
            3701 => {
                if !ready(app) {
                    return;
                }
                app.store.ignored.insert(109);
                app.scroll_fix = Some((CH, 0.0));
                self.next(1200);
            }
            3702 => {
                if at("invite:e2eChat1:join").is_none() || at("invite:e2eGone1:gone").is_none() || at("invite:e2eHere1:join").is_none() {
                    return;
                }
                self.mark("chatx_invite_cards");
                self.shot(ctx, "chatx_invites.png");
                self.next(300);
            }
            3703 => {
                let Some(r) = at("invite:e2eChat1:join") else { return };
                self.click(r.center());
                self.next(300);
            }
            3704 => {
                if !app.store.guilds.contains_key(&3000) {
                    return;
                }
                self.mark("chatx_invite_join");
                self.next(500);
            }
            3705 => {
                let Some(r) = at("invite:e2eChat1:join") else { return };
                self.click(r.center());
                self.next(300);
            }
            3706 => {
                if app.ui.guild != Some(3000) {
                    return;
                }
                self.mark("chatx_invite_joined_opens");
                app.select_guild(Some(self.target.0));
                app.open(CH);
                self.next(900);
            }
            3707 => {
                if !ready(app) {
                    return;
                }
                let Some(id) = id_of(app, "ordinary message") else { return };
                app.scroll_to = Some((CH, id));
                self.next(500);
            }
            3708 => {
                if app.scroll_to.is_some() || at("blocked:2:blocked:show").is_none() || at("blocked:1:ignored:show").is_none() {
                    return;
                }
                self.mark("chatx_blocked_collapsed");
                self.shot(ctx, "chatx_blocked.png");
                let Some(r) = at("blocked:2:blocked:show") else { return };
                self.click(r.center());
                self.next(400);
            }
            3709 => {
                let Some(r) = at("blocked:2:blocked:hide") else { return };
                self.mark("chatx_blocked_shown");
                self.shot(ctx, "chatx_blocked_shown.png");
                self.click(r.center());
                self.next(400);
            }
            3710 => {
                if at("blocked:2:blocked:show").is_none() || app.unblocked.contains(&108) {
                    return;
                }
                self.mark("chatx_blocked_folded_again");
                let Some(id) = poll_id(app) else { return };
                app.scroll_to = Some((CH, id));
                self.next(500);
            }
            3711 => {
                let Some(r) = app.rects.get("poll_votes_btn").copied() else { return };
                if app.scroll_to.is_some() {
                    return;
                }
                self.click(r.center());
                self.next(500);
            }
            3712 => {
                if at("poll_votes:user:101").is_none() {
                    return;
                }
                self.mark("chatx_poll_voters");
                self.shot(ctx, "chatx_poll_votes.png");
                let Some(r) = at("poll_votes:tab:2") else { return };
                self.click(r.center());
                self.next(500);
            }
            3713 => {
                self.mark("chatx_poll_voters_tab");
                self.key(Key::Escape);
                self.next(400);
            }
            3714 => {
                if app.votes.is_some() {
                    return;
                }
                let Some(id) = id_of(app, "image to copy") else { return };
                app.scroll_to = Some((CH, id));
                self.next(500);
            }
            3715 => {
                let Some(r) = app.rects.get("image").copied() else { return };
                if app.scroll_to.is_some() {
                    return;
                }
                self.rclick(r.center());
                self.next(400);
            }
            3716 => {
                let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", "Copy Image")))) else { return };
                self.shot(ctx, "chatx_image_menu.png");
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.clear();
                }
                self.click(r.center());
                self.next(1500);
            }
            3717 => {
                let got = arboard::Clipboard::new().and_then(|mut c| c.get_image()).map(|i| (i.width, i.height)).unwrap_or_default();
                if got == (800, 450) {
                    self.mark("chatx_copy_image");
                }
                let Some(id) = id_of(app, "sticker from elsewhere") else { return };
                app.scroll_to = Some((CH, id));
                self.next(500);
            }
            3718 => {
                let Some(r) = app.rects.get("sticker:2085").copied() else { return };
                if app.scroll_to.is_some() {
                    return;
                }
                self.click(r.center());
                self.next(600);
            }
            3719 => {
                if at("source_pop:join").is_none() || at("source_pop:more").is_none() {
                    return;
                }
                self.mark("chatx_sticker_popout");
                self.shot(ctx, "chatx_sticker_popout.png");
                let Some(r) = at("source_pop:join") else { return };
                self.click(r.center());
                self.next(500);
            }
            3720 => {
                if app.source_pop.is_some() {
                    return;
                }
                let Some(id) = id_of(app, "a faraway") else { return };
                app.scroll_to = Some((CH, id));
                self.next(500);
            }
            3721 => {
                let Some(r) = app.rects.get("custom_emoji:9999").copied() else { return };
                if app.scroll_to.is_some() {
                    return;
                }
                self.rects.insert("emoji_at".into(), json!([r.center().x, r.center().y, r.width()]));
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(700);
            }
            3722 => {
                let Some(r) = app.rects.get("custom_emoji:9999").copied() else { return };
                if app.rects.contains_key("emoji_tip") {
                    self.mark("chatx_emoji_tooltip");
                }
                self.shot(ctx, "chatx_emoji_hover.png");
                self.click(r.center());
                self.next(600);
            }
            3723 => {
                if app.source_pop.is_none() {
                    return self.fail("a real click on the custom emoji didn't open its popout".into());
                }
                self.mark("chatx_emoji_click");
                self.next(300);
                self.step = 3725;
            }
            3725 => {
                let loaded = app.sources.get(&(false, 9999)).is_some_and(|v| !v.is_null());
                if !app.source_pop.as_ref().is_some_and(|p| !p.sticker && p.id == 9999) || !loaded {
                    return;
                }
                self.mark("chatx_emoji_popout");
                self.shot(ctx, "chatx_emoji_popout.png");
                self.next(500);
                self.step = 3724;
            }
            3724 => {
                self.key(Key::Escape);
                self.next(400);
                self.step = 3726;
            }
            3726 => {
                if app.source_pop.is_some() {
                    return;
                }
                let Some(id) = id_of(app, "quiet please") else { return };
                app.scroll_to = Some((CH, id));
                self.next(500);
            }
            3727 => {
                let Some(id) = id_of(app, "quiet please") else { return };
                let Some(r) = app.rects.get(&format!("silent:{id}")).copied() else { return };
                if app.scroll_to.is_some() {
                    return;
                }
                self.mark("chatx_silent");
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(900);
            }
            3728 => {
                self.shot(ctx, "chatx_silent.png");
                let Some(id) = id_of(app, "quiet please") else { return };
                app.menu = Some((CH, id, pos2(420.0, 160.0)));
                self.next(500);
            }
            3729 => {
                let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", "Create Reminder")))) else { return };
                self.click(r.center());
                self.next(500);
            }
            3730 => {
                let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", "Custom...")))) else { return };
                self.shot(ctx, "chatx_reminder_menu.png");
                self.click(r.center());
                self.next(500);
            }
            3731 => {
                let Some(r) = at("remind_custom:input") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("in 2 hours".into())]);
                self.next(500);
            }
            3732 => {
                let Some(r) = at("remind_custom:set") else { return };
                self.shot(ctx, "chatx_remind_custom.png");
                self.click(r.center());
                self.next(500);
            }
            3733 => {
                if app.remind_custom.is_some() {
                    return;
                }
                self.mark("chatx_remind_custom");
                app.picker = Some((Pick::Composer, pos2(700.0, 440.0)));
                self.next(800);
            }
            3734 => {
                if app.top_emojis.get(&self.target.0).is_none_or(|t| t.len() != 2) || at("picker:top").is_none() {
                    return;
                }
                self.mark("chatx_top_emojis");
                self.shot(ctx, "chatx_top_emojis.png");
                self.next(600);
            }
            3735 => {
                app.picker = None;
                self.next(300);
                self.step = 3737;
            }
            3737 => {
                app.open(2030);
                self.next(900);
            }
            3738 => {
                if !app.store.forum_posts.get(&2030).is_some_and(|p| !p.is_empty()) || !app.store.channels.contains_key(&2031) {
                    return;
                }
                app.open_thread(2031, true, false);
                self.next(900);
            }
            3739 => {
                if app.cur() != Some(2031) || app.rects.get("op_chip").is_none() {
                    return;
                }
                self.mark("chatx_op_chip");
                self.shot(ctx, "chatx_op_chip.png");
                self.next(500);
            }
            3740 => {
                app.open(CH);
                let _ = std::fs::create_dir_all(&files);
                for (name, mb) in [("slow-4mb.bin", 4usize), ("big-3mb.bin", 3), ("huge-12mb.bin", 12)] {
                    let _ = std::fs::write(files.join(name), vec![0u8; mb << 20]);
                }
                app.files.clear();
                app.attach_files(vec![files.join("huge-12mb.bin")]);
                self.next(500);
            }
            3741 => {
                if app.confirm.as_ref().is_none_or(|c| c.title != "Your files are too powerful") {
                    return;
                }
                if app.files.is_empty() {
                    self.mark("chatx_too_powerful");
                }
                self.shot(ctx, "chatx_too_powerful.png");
                self.key(Key::Enter);
                self.next(500);
            }
            3742 => {
                if app.confirm.is_some() {
                    return;
                }
                app.attach_files(vec![files.join("slow-4mb.bin")]);
                app.drafts.insert(CH, "slow upload".into());
                app.post(CH, false);
                self.next(500);
            }
            3743 => {
                let Some(job) = pending(app).and_then(|n| crate::upload::get(&n)) else { return };
                let (sent, total) = (job.sent.load(std::sync::atomic::Ordering::Relaxed), job.total());
                let (Some(x), true) = (app.rects.get("upload_cancel:2085").copied(), sent > total / 8) else { return };
                if sent < total {
                    self.mark("chatx_upload_progress");
                }
                self.shot(ctx, "chatx_upload.png");
                self.click(x.center());
                self.next(300);
            }
            3744 => {
                let gone = app.store.msgs.get(&CH).is_some_and(|e| !e.list.iter().any(|m| m.content == "slow upload"));
                if !gone {
                    return;
                }
                self.mark("chatx_upload_cancelled");
                self.loaded = count(app, "big upload");
                app.attach_files(vec![files.join("big-3mb.bin")]);
                app.drafts.insert(CH, "big upload".into());
                app.post(CH, false);
                self.next(500);
            }
            3745 => {
                let done = app.store.msgs.get(&CH).is_some_and(|e| e.list.iter().any(|m| !m.pending && m.content == "big upload" && m.attachments.iter().any(|a| a.filename == "big-3mb.bin")));
                if !done || count(app, "big upload") <= self.loaded {
                    return;
                }
                self.mark("chatx_upload_done");
                self.shot(ctx, "chatx_upload_done.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                app.scroll_fix = Some((CH, 0.0));
                self.next(1200);
            }
            3746 => {
                self.shot(ctx, "chatx_invites_small.png");
                let Some(id) = id_of(app, "ordinary message") else { return };
                app.scroll_to = Some((CH, id));
                self.next(900);
            }
            3747 => {
                if app.scroll_to.is_some() {
                    return;
                }
                self.shot(ctx, "chatx_blocked_small.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 800.0)));
                self.next(600);
            }
            _ => self.step = 3800,
        }
    }
}

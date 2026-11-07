//! The forum's post form against the mock's REQUIRE_TAG ideas forum: the tag picker, the refusal
//! it would otherwise hit, "See more tags", and the body that went out. Steps 4300 and up.

use super::*;
use crate::net::Tag;

const FORUM: Id = 2030;
const MORE: [&str; 12] = ["bug", "docs", "help", "meta", "question", "showcase", "tips", "wip", "design", "release", "ops", "misc"];

impl Run {
    pub(super) fn forum_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let rect = |app: &App, key: &str| app.rects.get(key).copied();
        let tagged = |app: &App| app.store.forum_posts.get(&FORUM).and_then(|p| p.iter().filter_map(|id| app.store.channels.get(id)).find(|c| c.name == "Tagged post")).map(|c| c.applied.clone());
        match self.step {
            4300 => {
                if app.conn != Conn::Online {
                    return;
                }
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                app.select_guild(Some(self.target.0));
                app.open(FORUM);
                self.next(900);
            }
            4301 => {
                if app.cur() != Some(FORUM) || !app.store.forum_posts.get(&FORUM).is_some_and(|p| !p.is_empty()) {
                    return;
                }
                app.rest("PATCH", format!("/channels/{FORUM}"), Some(json!({ "flags": 16 })), Tag::Quiet);
                self.next(600);
            }
            4302 => {
                let (Some(b), true) = (rect(app, "forum:new_post"), app.store.channels.get(&FORUM).is_some_and(|c| c.require_tag)) else { return };
                self.click(b.center());
                self.next(500);
            }
            4303 => {
                let Some(r) = rect(app, "forum:post_title") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("Tagged post".into())]);
                self.next(300);
            }
            4304 => {
                let Some(r) = rect(app, "forum:post_body") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("Needs a tag first".into())]);
                self.next(400);
            }
            4305 => {
                let (Some(err), Some(send)) = (rect(app, "forum:post_tag_required"), rect(app, "forum:post_send")) else { return };
                self.mark("forum_required_error");
                self.shot(ctx, "forum_required_error.png");
                self.rects.insert("forum_error_box".into(), json!([err.min.x, err.min.y, err.width(), err.height()]));
                self.click(send.center());
                self.next(600);
            }
            4306 => {
                let Some(tag) = rect(app, "forum:post_tag:2") else { return };
                if app.post.is_some() {
                    self.mark("forum_required_blocks");
                }
                self.click(tag.center());
                self.next(400);
            }
            4307 => {
                let Some(send) = rect(app, "forum:post_send") else { return };
                if app.post.as_ref().is_some_and(|p| p.tags == [2]) && rect(app, "forum:post_tag_required").is_none() {
                    self.mark("forum_tag_picked");
                }
                self.shot(ctx, "forum_tag_picked.png");
                self.click(send.center());
                self.next(900);
            }
            4308 => {
                if app.post.is_some() || tagged(app).is_none() {
                    return;
                }
                if tagged(app) == Some(vec![2]) {
                    self.mark("forum_post_created_tagged");
                }
                self.shot(ctx, "forum_post_created.png");
                let tags: Vec<_> = [("1", "feature"), ("2", "ux")].into_iter().chain(MORE.iter().enumerate().map(|(i, n)| (["3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14"][i], *n))).map(|(id, name)| json!({ "id": id, "name": name })).collect();
                app.rest("PATCH", format!("/channels/{FORUM}"), Some(json!({ "available_tags": tags })), Tag::Quiet);
                self.next(700);
            }
            4309 => {
                let (Some(b), true) = (rect(app, "forum:new_post"), app.store.channels.get(&FORUM).is_some_and(|c| c.tags.len() == 14)) else { return };
                self.click(b.center());
                self.next(500);
            }
            4310 => {
                let Some(more) = rect(app, "forum:post_more_tags") else { return };
                self.mark("forum_see_more_tags");
                self.click(more.center());
                self.next(500);
            }
            4311 => {
                let Some(row) = rect(app, "forum:more_tag:14") else { return };
                self.shot(ctx, "forum_more_tags.png");
                self.click(row.center());
                self.next(500);
            }
            4312 => {
                if app.post.as_ref().is_some_and(|p| p.tags == [14]) {
                    self.mark("forum_more_tag_picked");
                }
                app.post = None;
                app.rest("PATCH", format!("/channels/{FORUM}"), Some(json!({ "flags": 0, "available_tags": [{ "id": "1", "name": "feature", "emoji_name": "✨" }, { "id": "2", "name": "ux" }] })), Tag::Quiet);
                self.next(500);
            }
            4313 => {
                if app.post.is_some() {
                    return;
                }
                app.open_thread(2031, true, false);
                self.next(1200);
            }
            4314 => {
                let Some(follow) = Self::at(ctx, "forum_bar:follow") else { return };
                if app.cur() != Some(2031) || Self::at(ctx, "forum_bar:react").is_none() {
                    return;
                }
                self.mark("forum_bar_drawn");
                self.shot(ctx, "forum_bar.png");
                self.click(follow.center());
                self.next(700);
            }
            4315 => {
                if app.store.channels.get(&2031).is_some_and(|c| c.joined) {
                    self.mark("forum_bar_following");
                }
                self.shot(ctx, "forum_bar_following.png");
                let Some(link) = Self::at(ctx, "forum_bar:link") else { return };
                self.click(link.center());
                self.next(500);
            }
            4316 => {
                if Self::at(ctx, "forum_bar:copied").is_some() {
                    self.mark("forum_bar_copied");
                }
                let Some(react) = Self::at(ctx, "forum_bar:react") else { return };
                self.click(react.center());
                self.next(600);
            }
            4317 => {
                if app.picker.is_some() {
                    self.mark("forum_bar_react_picker");
                }
                self.key(Key::Escape);
                self.next(400);
            }
            4318 => {
                app.open(2011);
                self.next(900);
            }
            4319 => {
                let Some(id) = app.store.msgs.get(&2011).and_then(|e| e.list.iter().find(|m| m.thread.is_some())).map(|m| m.id) else { return };
                if self.rects.insert("thread_card_jumped".into(), json!(true)).is_none() {
                    app.attach(2050);
                    app.jump_to(2011, id);
                    self.wait = Instant::now() + Duration::from_millis(1500);
                    return;
                }
                self.shot(ctx, "thread_card.png");
                self.next(300);
            }
            _ => self.step = 4320,
        }
    }
}

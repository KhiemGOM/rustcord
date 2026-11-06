use egui::{Event, Key, Rect, vec2};
use std::time::{Duration, Instant};

use super::Run;
use crate::state::{Id, Stage};
use crate::ui::App;

const OPTIONAL: [(&str, &str); 10] = [
    ("choice", ""),
    ("count", "11"),
    ("ratio", "0.5"),
    ("flag", ""),
    ("person", "ali"),
    ("rank", ""),
    ("place", "gen"),
    ("either", "bob"),
    ("file", ""),
    ("city", "ber"),
];

fn bot_msg(app: &App, ch: Id, f: impl Fn(&crate::state::Message) -> bool) -> Option<crate::state::Message> {
    app.store.msgs.get(&ch)?.list.iter().rev().find(|m| m.author == 104 && f(m)).cloned()
}

fn stage(app: &App, ch: Id) -> Option<&'static str> {
    app.store.msgs.get(&ch)?.list.iter().rev().find_map(|m| {
        m.busy.as_ref().filter(|_| m.interaction.is_some() && m.nonce.as_deref() == Some(m.id.to_string().as_str())).map(|b| match b.stage {
            Stage::Sending => "sending",
            Stage::Created => "created",
            Stage::Failed(..) => "failed",
        })
    })
}

impl Run {
    fn type_text(&mut self, text: &str) {
        self.queue.push_back(vec![Event::Text(text.into())]);
    }

    fn pause(&mut self, ms: u64) {
        self.wait = Instant::now() + Duration::from_millis(ms);
    }

    fn command(&mut self, app: &mut App, ctx: &egui::Context, text: &str) -> bool {
        let Some(r) = app.rects.get("composer").copied() else { return false };
        self.click(r.center() + vec2(-r.width() / 4.0, 0.0));
        self.type_text(text);
        let _ = ctx;
        true
    }

    /// Bot commands: option pills and their pickers, autocomplete, modals, the states between sending a command and its answer, select menus and buttons.
    pub(super) fn bot_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let chan = app.store.channels.get(&general).cloned();
        let Some(chan) = chan else { return };
        let menus = bot_msg(app, general, |m| !m.components.is_empty()).map_or(0, |m| m.id);
        let late = self.step_at.elapsed() > Duration::from_secs(12);
        let rect = |key: &str| ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("e2e", key))));
        let info = app.slash_info(&chan);
        let at = |app: &App, key: &str| app.rects.get(key).copied();
        macro_rules! when {
            ($ok:expr, $body:block) => {
                if $ok {
                    $body
                } else {
                    if late {
                        self.next(0);
                    }
                    return;
                }
            };
        }
        match self.step {
            3500 => {
                app.select_guild(Some(self.target.0));
                app.open(general);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                self.next(1200);
            }
            3501 => {
                if self.command(app, ctx, "/showc") {
                    self.next(900);
                }
            }
            3502 => when!(app.ac_open, {
                self.mark("bot_picker");
                self.shot(ctx, "bot_slash_list.png");
                self.key(Key::Tab);
                self.next(400);
            }),
            3503 => when!(info.as_ref().is_some_and(|i| i.label == "showcase"), {
                let i = info.as_ref().unwrap();
                if at(app, "slash_pill:text").is_some() && at(app, "slash_pill:choice").is_none() && i.hidden.len() == 10 && i.active == "text" {
                    self.mark("bot_pills");
                }
                self.shot(ctx, "bot_pills.png");
                self.type_text("ab");
                self.key(Key::Enter);
                self.next(500);
            }),
            3504 => when!(info.as_ref().is_some_and(|i| i.tried), {
                let i = info.as_ref().unwrap();
                if i.error.as_deref() == Some("Enter between 3 and 20 characters.") {
                    self.mark("bot_pill_validation");
                }
                self.shot(ctx, "bot_pill_error.png");
                self.type_text("c");
                self.next(300);
            }),
            3505 => {
                self.key(Key::Tab);
                self.next(400);
            }
            3506 => when!(info.as_ref().is_some_and(|i| i.optional_open && i.list == 10), {
                self.mark("bot_optional_list");
                self.shot(ctx, "bot_optional_list.png");
                self.key(Key::Escape);
                self.loaded = 0;
                self.sub = 0;
                self.next(300);
            }),
            3507 => {
                let Some(i) = info.as_ref() else { return };
                if self.loaded >= OPTIONAL.len() {
                    self.shot(ctx, "bot_all_options.png");
                    self.mark("bot_all_options");
                    self.next(200);
                    return;
                }
                let (name, text) = OPTIONAL[self.loaded];
                match self.sub {
                    0 => {
                        if !i.optional_open {
                            let Some(r) = at(app, "slash_optional") else { return };
                            self.click(r.center());
                        }
                        self.sub = 1;
                        self.pause(300);
                    }
                    1 => {
                        if !i.optional_open {
                            self.sub = 0;
                            return;
                        }
                        app.slash_hidden_at(general, name);
                        self.key(Key::Enter);
                        self.sub = 2;
                        self.pause(300);
                    }
                    2 => {
                        if i.active != name {
                            return;
                        }
                        if name == "file" {
                            let path = self.dir.join("bot_upload.txt");
                            let _ = std::fs::write(&path, "an attachment for the bot");
                            app.set_file(general, path);
                            self.sub = 5;
                            self.pause(200);
                            return;
                        }
                        if !text.is_empty() {
                            self.type_text(text);
                        }
                        self.sub = 3;
                        self.pause(if name == "city" { 1400 } else { 400 });
                    }
                    3 => {
                        match name {
                            "count" => {
                                if i.error.as_deref() == Some("Enter a number between 1 and 10.") {
                                    self.mark("bot_integer_range");
                                    self.shot(ctx, "bot_integer_error.png");
                                }
                                self.key(Key::Backspace);
                                self.key(Key::Backspace);
                                self.type_text("4");
                                self.sub = 5;
                            }
                            "ratio" => self.sub = 5,
                            _ => {
                                if i.list == 0 {
                                    if self.step_at.elapsed() > Duration::from_secs(30) {
                                        self.sub = 5;
                                    }
                                    return;
                                }
                                if name == "choice" {
                                    app.slash_sel(general, 1);
                                    self.shot(ctx, "bot_choices.png");
                                }
                                if name == "person" {
                                    self.shot(ctx, "bot_user_picker.png");
                                }
                                if name == "city" {
                                    self.mark("bot_autocomplete");
                                    self.shot(ctx, "bot_autocomplete.png");
                                }
                                self.key(Key::Enter);
                                self.sub = 4;
                            }
                        }
                        self.pause(400);
                    }
                    4 => {
                        if i.filled.iter().any(|(n, _)| n == name) {
                            self.sub = 5;
                        }
                    }
                    _ => {
                        self.loaded += 1;
                        self.sub = 0;
                    }
                }
            }
            3508 => {
                let all = ["choice", "count", "ratio", "flag", "person", "rank", "place", "either", "file", "city"];
                if info.as_ref().is_some_and(|i| all.iter().all(|n| i.filled.iter().any(|(f, _)| f == n) || *n == "count" || *n == "ratio")) {
                    self.mark("bot_pickers");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(700);
            }
            3509 => {
                self.shot(ctx, "bot_pills_940.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                self.next(500);
            }
            3510 => {
                self.key(Key::Enter);
                self.next(1500);
            }
            3511 => when!(bot_msg(app, general, |m| m.content.starts_with("`/showcase`")).is_some(), {
                self.mark("bot_showcase_reply");
                self.shot(ctx, "bot_showcase_reply.png");
                self.next(300);
            }),
            3512 => {
                if self.command(app, ctx, "/lookup") {
                    self.next(500);
                }
            }
            3513 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3514 => when!(info.as_ref().is_some_and(|i| i.label == "lookup"), {
                self.type_text("fail");
                self.next(2000);
            }),
            3515 => when!(info.as_ref().is_some_and(|i| i.failed), {
                self.mark("bot_autocomplete_failed");
                self.shot(ctx, "bot_autocomplete_failed.png");
                let Some(r) = at(app, "slash_close") else { return };
                self.click(r.center());
                self.next(400);
            }),
            3516 => when!(app.slash_info(&chan).is_none(), {
                self.mark("bot_slash_closed");
                if self.command(app, ctx, "/feedback") {
                    self.next(600);
                }
            }),
            3517 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3518 => when!(info.as_ref().is_some_and(|i| i.label == "feedback"), {
                self.key(Key::Enter);
                self.next(600);
            }),
            3519 => when!(app.interactions.modal.is_some() && rect("bot_modal:submit").is_some(), {
                self.mark("bot_modal_open");
                self.shot(ctx, "bot_modal.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(700);
            }),
            3520 => {
                self.shot(ctx, "bot_modal_940.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                self.next(600);
            }
            3521 => when!(rect("bot_modal:submit").is_some(), {
                let r = rect("bot_modal:submit").unwrap();
                self.click(r.center());
                self.next(500);
            }),
            3522 => when!(app.interactions.modal.as_ref().is_some_and(|m| m.invalid() == 2), {
                self.mark("bot_modal_validation");
                self.shot(ctx, "bot_modal_errors.png");
                let r = rect("bot_input:subject").unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.type_text("Great app");
                self.next(300);
            }),
            3523 => {
                let r = rect("bot_input:details").unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.type_text("it opens forms now");
                self.next(300);
            }
            3524 => when!(rect("bot_select:mood").is_some(), {
                let r = rect("bot_select:mood").unwrap();
                self.click(r.center());
                self.next(400);
            }),
            3525 => when!(rect("select_opt:great").is_some(), {
                self.shot(ctx, "bot_modal_select.png");
                let r = rect("select_opt:great").unwrap();
                self.click(r.center());
                self.next(400);
            }),
            3526 => {
                self.shot(ctx, "bot_modal_filled.png");
                let r = rect("bot_modal:submit").unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.next(800);
            }
            3527 => when!(app.interactions.modal.is_none() && bot_msg(app, general, |m| m.content.starts_with("Thanks for the feedback")).is_some(), {
                self.mark("bot_modal_submit");
                if self.command(app, ctx, "/slow") {
                    self.next(500);
                }
            }),
            3528 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3529 => when!(info.as_ref().is_some_and(|i| i.label == "slow"), {
                self.key(Key::Enter);
                self.next(300);
            }),
            3530 => when!(stage(app, general) == Some("sending"), {
                self.mark("bot_sending");
                self.shot(ctx, "bot_sending.png");
                self.next(200);
            }),
            3531 => when!(stage(app, general) == Some("created"), {
                self.mark("bot_thinking");
                self.shot(ctx, "bot_thinking.png");
                self.next(200);
            }),
            3532 => when!(bot_msg(app, general, |m| m.flags & 128 != 0).is_some(), {
                self.mark("bot_deferred");
                self.shot(ctx, "bot_deferred.png");
                self.next(200);
            }),
            3533 => when!(bot_msg(app, general, |m| m.content.starts_with("Done thinking")).is_some(), {
                self.mark("bot_deferred_edit");
                if self.command(app, ctx, "/broken") {
                    self.next(500);
                }
            }),
            3534 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3535 => when!(info.as_ref().is_some_and(|i| i.label == "broken"), {
                self.key(Key::Enter);
                self.next(300);
            }),
            3536 => when!(stage(app, general) == Some("failed"), {
                self.mark("bot_failed");
                self.shot(ctx, "bot_failed.png");
                self.next(3300);
            }),
            3537 => {
                self.shot(ctx, "bot_timed_out.png");
                if stage(app, general) == Some("failed") {
                    self.mark("bot_timed_out");
                }
                if self.command(app, ctx, "/cooldown") {
                    self.next(500);
                }
            }
            3538 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3539 => when!(info.as_ref().is_some_and(|i| i.label == "cooldown"), {
                self.key(Key::Enter);
                self.next(1500);
            }),
            3540 => {
                if self.command(app, ctx, "/cooldown") {
                    self.next(500);
                }
            }
            3541 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3542 => when!(info.as_ref().is_some_and(|i| i.label == "cooldown"), {
                self.key(Key::Enter);
                self.next(700);
            }),
            3543 => when!(stage(app, general) == Some("failed"), {
                self.mark("bot_cooldown");
                self.shot(ctx, "bot_cooldown.png");
                self.next(200);
            }),
            3544 => {
                if self.command(app, ctx, "/menus") {
                    self.next(500);
                }
            }
            3545 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3546 => when!(info.as_ref().is_some_and(|i| i.label == "menus"), {
                self.key(Key::Enter);
                self.next(1500);
            }),
            3547 => when!(bot_msg(app, general, |m| !m.components.is_empty()).is_some(), {
                app.jump_to_present(general);
                self.next(1000);
            }),
            3548 => when!(rect("select:pick_user").is_some(), {
                self.mark("bot_menus");
                self.shot(ctx, "bot_menus.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(900);
            }),
            3549 => {
                self.shot(ctx, "bot_menus_940.png");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                self.next(900);
            }
            3550 => when!(rect("select:pick_color").is_some(), {
                let r = rect("select:pick_color").unwrap();
                self.click(r.center());
                self.next(500);
            }),
            3551 => when!(rect("select_opt:r").is_some(), {
                self.shot(ctx, "bot_select_open.png");
                let r = rect("select_opt:r").unwrap();
                self.click(r.center());
                self.next(500);
            }),
            3552 => {
                self.mark("bot_select_min");
                self.shot(ctx, "bot_select_min.png");
                let r = rect("select_opt:g").unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.next(1200);
            }
            3553 => when!(bot_msg(app, general, |m| m.content.starts_with("pick_color:")).is_some(), {
                self.mark("bot_select_multi");
                self.key(Key::Escape);
                self.next(300);
            }),
            3554 => {
                app.jump_to(general, menus);
                self.next(600);
            }
            3555 => when!(rect("select:pick_user").is_some(), {
                let r = rect("select:pick_user").unwrap();
                self.click(r.center());
                self.next(500);
            }),
            3556 => when!(rect("select_opt:101").is_some(), {
                self.shot(ctx, "bot_user_select.png");
                let r = rect("select_opt:101").unwrap();
                self.click(r.center());
                self.next(1200);
            }),
            3557 => when!(bot_msg(app, general, |m| m.content.starts_with("pick_user: 101")).is_some(), {
                self.mark("bot_select_user");
                app.jump_to(general, menus);
                self.next(700);
            }),
            3558 => when!(rect("select:pick_role").is_some(), {
                let r = rect("select:pick_role").unwrap();
                self.click(r.center());
                self.next(500);
            }),
            3559 => {
                self.shot(ctx, "bot_role_select.png");
                let r = rect("select_opt:1100").unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.next(1200);
            }
            3560 => when!(bot_msg(app, general, |m| m.content.starts_with("pick_role: 1100")).is_some(), {
                self.mark("bot_select_role");
                app.jump_to(general, menus);
                self.next(700);
            }),
            3561 => when!(rect("select:pick_channel").is_some(), {
                let r = rect("select:pick_channel").unwrap();
                self.click(r.center());
                self.next(500);
            }),
            3562 => {
                self.shot(ctx, "bot_channel_select.png");
                let r = rect(&format!("select_opt:{general}")).unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.next(1200);
            }
            3563 => when!(bot_msg(app, general, |m| m.content.starts_with("pick_channel:")).is_some(), {
                self.mark("bot_select_channel");
                app.jump_to(general, menus);
                self.next(700);
            }),
            3564 => when!(rect("select:pick_either").is_some(), {
                let r = rect("select:pick_either").unwrap();
                self.click(r.center());
                self.next(500);
            }),
            3565 => {
                self.shot(ctx, "bot_mentionable_select.png");
                let r = rect("select_opt:101").unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.next(300);
            }
            3566 => {
                let r = rect("select_opt:1100").unwrap_or(Rect::ZERO);
                self.click(r.center());
                self.next(1200);
            }
            3567 => when!(bot_msg(app, general, |m| m.content.starts_with("pick_either:")).is_some(), {
                self.mark("bot_select_mentionable");
                app.jump_to(general, menus);
                self.next(700);
            }),
            3568 => when!(rect("select:pick_off").is_some(), {
                let r = rect("select:pick_off").unwrap();
                self.click(r.center());
                self.next(600);
            }),
            3569 => {
                if !bot_msg(app, general, |m| m.content.starts_with("pick_off")).is_some() {
                    self.mark("bot_disabled");
                }
                let Some(r) = at(app, "button:slow_btn") else { return };
                self.click(r.center());
                self.next(600);
            }
            3570 => when!(app.store.msgs.get(&general).is_some_and(|e| e.list.iter().any(|m| m.busy.as_ref().is_some_and(|b| b.component == "slow_btn"))), {
                self.mark("bot_button_loading");
                self.shot(ctx, "bot_button_loading.png");
                self.next(2600);
            }),
            3571 => {
                app.jump_to(general, menus);
                let Some(r) = at(app, "button:fail_btn") else { return };
                self.click(r.center());
                self.next(900);
            }
            3572 => when!(app.store.msgs.get(&general).is_some_and(|e| e.list.iter().any(|m| m.busy.as_ref().is_some_and(|b| matches!(b.stage, Stage::Failed(..)) && b.component == "fail_btn"))), {
                self.mark("bot_button_failed");
                self.shot(ctx, "bot_button_failed.png");
                self.next(300);
            }),
            3573 => {
                let Some(r) = at(app, "button:https://example.com/docs") else { return };
                self.click(r.center());
                self.next(600);
            }
            3574 => when!(app.leaving.as_ref().is_some_and(|(u, _)| u == "https://example.com/docs"), {
                self.mark("bot_link_prompt");
                self.shot(ctx, "bot_link_prompt.png");
                app.leaving = None;
                if at(app, "button:sku:7001").is_some() {
                    self.mark("bot_premium_button");
                }
                self.next(300);
            }),
            3575 => {
                if self.command(app, ctx, "/report") {
                    self.next(500);
                }
            }
            3576 => when!(app.ac_open, {
                self.key(Key::Tab);
                self.next(300);
            }),
            3577 => when!(info.as_ref().is_some_and(|i| i.label == "report"), {
                self.key(Key::Enter);
                self.next(1500);
            }),
            3578 => when!(bot_msg(app, general, |m| m.content.is_empty() && !m.embeds.is_empty()).is_some(), {
                let id = bot_msg(app, general, |m| m.content.is_empty() && !m.embeds.is_empty()).map(|m| m.id).unwrap_or(0);
                app.reply = Some(id);
                app.focus_composer = true;
                self.next(400);
            }),
            3579 => {
                self.type_text("thanks for the report");
                self.key(Key::Enter);
                self.next(1500);
            }
            3580 => when!(app.store.msgs.get(&general).is_some_and(|e| e.list.iter().any(|m| m.content == "thanks for the report" && m.reply.is_some())), {
                self.mark("bot_reply_header");
                app.jump_to_present(general);
                self.next(900);
            }),
            3581 => {
                self.shot(ctx, "bot_reply_header.png");
                self.step = 2900;
            }
            _ => self.step = 99,
        }
    }
}

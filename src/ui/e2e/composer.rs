use egui::Key;
use std::time::Duration;

use super::Run;
use crate::state::{BLOCKED, FRIEND, Id};
use crate::ui::App;
use crate::ui::gate::{self, Gate};

const SLOW: Id = 2015;
const READONLY: Id = 2016;
const NEWS: Id = 2002;
const LOCKED: Id = 2051;
const OLD: Id = 2052;
const DM: Id = 3000;

impl Run {
    /// The composer's states, its plus menu, polls, the app launcher, upload cards, and the
    /// group DM picker. A step that can't be met in six seconds is skipped, not failed.
    pub(super) fn composer_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let late = self.step_at.elapsed() > Duration::from_secs(6);
        let menu_row = |label: &str| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", label))));
        let gate_of = |app: &App, ch: Id| app.store.channels.get(&ch).map(|c| gate::check(app, c));
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
            560 => {
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(800);
            }
            561 => when!(at(app, "plus_button").is_some(), {
                let r = at(app, "plus_button").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(300);
            }),
            562 => when!(["Upload a File", "Create Thread", "Create Poll", "Use Apps"].iter().all(|l| menu_row(l).is_some()), {
                self.mark("composer_plus_menu");
                self.shot(ctx, "composer_plus_menu.png");
                let r = menu_row("Create Poll").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(400);
            }),
            563 => when!(app.poll.is_some(), {
                if let Some(p) = app.poll.as_mut() {
                    p.question = "which crate?".into();
                    p.answers[0].0 = "serde".into();
                    p.answers[1].0 = "tokio".into();
                }
                self.next(400);
            }),
            564 => when!(Self::at(ctx, "poll:post").is_some(), {
                self.shot(ctx, "composer_poll.png");
                let r = Self::at(ctx, "poll:post").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(800);
            }),
            565 => when!(app.store.msgs.get(&general).is_some_and(|e| e.list.iter().any(|m| m.poll.as_ref().is_some_and(|p| p.question == "which crate?" && p.answers.len() == 2))), {
                self.mark("composer_poll");
                app.open(SLOW);
                self.next(800);
            }),
            566 => {
                if let Some(r) = at(app, "composer") {
                    self.click(r.center());
                }
                self.queue.push_back(vec![egui::Event::Text("first".into())]);
                self.key(Key::Enter);
                self.next(800);
            }
            567 => when!(app.store.slow_left(&app.store.channels[&SLOW]) > 0 && at(app, "cooldown").is_some(), {
                self.mark("composer_slowmode");
                self.shot(ctx, "composer_slowmode.png");
                self.queue.push_back(vec![egui::Event::Text("second".into())]);
                self.key(Key::Enter);
                self.next(600);
            }),
            568 => {
                let kept = app.drafts.get(&SLOW).is_some_and(|d| d == "second");
                let sent = app.store.msgs.get(&SLOW).map_or(0, |e| e.list.iter().filter(|m| m.author == app.store.me).count());
                if kept && sent == 1 {
                    self.mark("composer_slow_blocked");
                }
                app.drafts.remove(&SLOW);
                app.open(READONLY);
                self.next(600);
            }
            569 => when!(matches!(gate_of(app, READONLY), Some(Gate::Off(_))), {
                self.mark("composer_no_perm");
                self.shot(ctx, "composer_no_perm.png");
                app.open(NEWS);
                self.next(600);
            }),
            570 => when!(matches!(gate_of(app, NEWS), Some(Gate::Bar(gate::Bar::Follow))) && at(app, "gate:button").is_some(), {
                self.mark("composer_follow_bar");
                self.shot(ctx, "composer_follow_bar.png");
                let r = at(app, "gate:button").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(400);
            }),
            571 => when!(app.follow.is_some(), {
                self.mark("composer_follow_modal");
                if let Some(f) = app.follow.as_mut() {
                    f.guild = Some(1001);
                    f.target = Some(2100);
                }
                self.next(300);
            }),
            572 => when!(Self::at(ctx, "follow:Follow").is_some(), {
                self.shot(ctx, "composer_follow_modal.png");
                let r = Self::at(ctx, "follow:Follow").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(600);
            }),
            573 => {
                app.follow = None;
                app.open(LOCKED);
                self.next(600);
            }
            574 => when!(matches!(gate_of(app, LOCKED), Some(Gate::Off(_))), {
                self.mark("composer_locked_thread");
                self.shot(ctx, "composer_locked_thread.png");
                app.open(OLD);
                self.next(600);
            }),
            575 => when!(app.store.channels.get(&OLD).is_some_and(|c| c.archived && gate::placeholder(c, &gate::check(app, c)).is_some()) && !matches!(gate_of(app, OLD), Some(Gate::Off(_))), {
                self.mark("composer_archived_thread");
                self.shot(ctx, "composer_archived_thread.png");
                if let Some(g) = app.store.guilds.get_mut(&1000) {
                    g.my_timeout = chrono::Utc::now().timestamp() + 3600;
                }
                app.open(general);
                self.next(500);
            }),
            576 => {
                if let Some(g) = app.store.guilds.get_mut(&1000) {
                    g.my_timeout = chrono::Utc::now().timestamp() + 3600;
                }
                app.open(general);
                self.next(600);
            }
            577 => when!(matches!(gate_of(app, general), Some(Gate::Off(_))) && at(app, "cooldown").is_some(), {
                self.mark("composer_timeout");
                self.shot(ctx, "composer_timeout.png");
                self.next(0);
            }),
            578 => {
                if let Some(g) = app.store.guilds.get_mut(&1000) {
                    g.my_timeout = 0;
                }
                app.store.relationships.insert(101, BLOCKED);
                app.ui.guild = None;
                app.open(DM);
                self.next(600);
            }
            579 => when!(matches!(gate_of(app, DM), Some(Gate::Bar(gate::Bar::Blocked(_)))) && at(app, "gate:button").is_some(), {
                self.mark("composer_blocked_dm");
                self.shot(ctx, "composer_blocked_dm.png");
                let r = at(app, "gate:button").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(400);
            }),
            580 => {
                app.store.relationships.insert(101, FRIEND);
                app.select_guild(Some(self.target.0));
                app.open(general);
                app.notice = Some((general, "You cannot message this user".into()));
                self.next(500);
            }
            581 => when!(at(app, "notice:dismiss").is_some(), {
                self.mark("composer_notice");
                self.shot(ctx, "composer_notice.png");
                let r = at(app, "notice:dismiss").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(300);
            }),
            582 => {
                if app.notice.is_none() {
                    self.mark("composer_notice_dismissed");
                }
                let path = self.dir.join("e2e-card.txt");
                let _ = std::fs::write(&path, "card");
                app.files.push(crate::ui::composer::Upload::new(path));
                self.next(500);
            }
            583 => when!(Self::at(ctx, "card:0").is_some(), {
                self.shot(ctx, "composer_cards.png");
                let r = Self::at(ctx, "card:0").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(300);
            }),
            584 => {
                self.key(Key::E);
                self.next(400);
            }
            585 => when!(app.modify.is_some() && Self::at(ctx, "modify:save").is_some(), {
                self.mark("composer_modify");
                self.shot(ctx, "composer_modify.png");
                if let Some(m) = app.modify.as_mut() {
                    m.alt = "a note".into();
                }
                let r = Self::at(ctx, "modify:save").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(400);
            }),
            586 => {
                app.modify = None;
                if app.files.first().is_some_and(|f| f.alt == "a note") {
                    self.mark("composer_modify_save");
                }
                if let Some(r) = Self::at(ctx, "card:0") {
                    self.click(r.center());
                }
                self.next(300);
            }
            587 => {
                self.key(Key::D);
                self.next(400);
            }
            588 => {
                if app.files.is_empty() {
                    self.mark("composer_card_d");
                }
                app.files.clear();
                if let Some(r) = at(app, "apps_button") {
                    self.click(r.center());
                }
                self.next(600);
            }
            589 => when!(app.launcher.is_some(), {
                self.mark("composer_launcher");
                self.shot(ctx, "composer_launcher.png");
                self.key(Key::Escape);
                self.next(300);
            }),
            590 => {
                if app.launcher.is_none() {
                    self.mark("composer_launcher_esc");
                }
                app.launcher = None;
                if let Some(r) = at(app, "header:topic") {
                    self.click(r.center());
                }
                self.next(400);
            }
            591 => when!(app.topic.is_some(), {
                self.mark("composer_topic");
                self.shot(ctx, "composer_topic.png");
                self.key(Key::Escape);
                self.next(300);
            }),
            592 => {
                app.topic = None;
                app.ui.guild = None;
                app.open(DM);
                self.next(700);
            }
            593 => when!(at(app, "dm:video").is_some() && at(app, "dm:add").is_some(), {
                self.mark("composer_dm_header");
                self.shot(ctx, "composer_dm_header.png");
                let r = at(app, "dm:add").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(500);
            }),
            594 => when!(app.group_pick.is_some(), {
                self.mark("composer_group_pick");
                if let Some(p) = app.group_pick.as_mut() {
                    p.picked = vec![102, 103];
                }
                self.next(400);
            }),
            595 => when!(Self::at(ctx, "pick:go").is_some(), {
                self.shot(ctx, "composer_group_pick.png");
                let r = Self::at(ctx, "pick:go").unwrap_or(egui::Rect::ZERO);
                self.click(r.center());
                self.next(900);
            }),
            596 => when!(app.cur().and_then(|c| app.store.channels.get(&c)).is_some_and(|c| c.kind == 3 && c.recipients.len() == 3), {
                self.mark("composer_group_dm");
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(300);
            }),
            _ => self.step = 800,
        }
    }
}

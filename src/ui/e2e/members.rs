//! Steps 3110 to 3189: the server settings Members table used for real against the mock: sort,
//! search, the signals filter, adding and removing a role, prune, a timeout, a kick and a ban from
//! a row's menu, and bulk ban and kick on selected rows, each answered by the mock's own state.
//! The mock's member state outlives one client, so only the cold run (RUSTCORD_E2E_MEMBERS) drives
//! it. `python e2e/from_step.py 3110` runs just this group.

use std::time::Duration;

use egui::{Event, Key, Modifiers, Rect, pos2};

use super::Run;
use crate::state::Id;
use crate::ui::App;
use crate::ui::server::Page;

const ROLE_ADDED: &str = "Contributor";
// the server of the mock that the account owns, so every moderation action is on offer
const OWNED: Id = 1001;

fn ids(app: &App) -> Vec<Id> {
    let rows = app.server.as_ref().and_then(|s| s.pages.members.list.as_ref());
    rows.into_iter().flatten().filter_map(|m| m["member"]["user"]["id"].as_str()?.parse().ok()).collect()
}

fn row_of(app: &App, uid: Id) -> Option<serde_json::Value> {
    let rows = app.server.as_ref().and_then(|s| s.pages.members.list.as_ref())?;
    rows.iter().find(|m| m["member"]["user"]["id"].as_str() == Some(&uid.to_string())).cloned()
}

fn menu(ctx: &egui::Context, label: &str) -> Option<Rect> {
    Run::at(ctx, &format!("menu_row:{}", label))
}

impl Run {
    /// Clicks a row of the user menu: scrolled down first, since the rows past its end are the moderation ones, and
    /// once it has stopped moving, since its note loads in and shifts the rest.
    fn settled_menu_click(&mut self, ctx: &egui::Context, label: &str, then: u64) {
        let Some(r) = menu(ctx, label) else { return };
        if self.loaded == 0 {
            let at = menu(ctx, "Profile").map_or(r.center(), |p| p.center());
            self.queue.push_back(vec![Event::PointerMoved(at)]);
            self.queue.push_back(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, -1000.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
            self.loaded = 1;
            self.wait = std::time::Instant::now() + Duration::from_millis(700);
            return;
        }
        if self.steady != Some(r) {
            self.steady = Some(r);
            return;
        }
        self.steady = None;
        self.loaded = 0;
        self.rects.insert(format!("menu:{label}"), serde_json::json!([r.min.x, r.min.y, r.width(), r.height()]));
        self.shot(ctx, &format!("members_menu_{}.png", label.split(' ').next().unwrap_or_default().to_lowercase()));
        self.click(r.center());
        self.next(then);
    }

    pub(super) fn members_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let gid = OWNED;
        let at = |app: &App, k: &str| app.rects.get(k).copied();
        let stuck = self.step_at.elapsed() > Duration::from_secs(12);
        let ok = |run: &mut Self, what: &str| {
            run.mark(what);
            run.next(500);
        };
        match self.step {
            3110 => {
                if std::env::var_os("RUSTCORD_E2E_MEMBERS").is_none() {
                    self.step = 99;
                    return;
                }
                if app.server.is_none() {
                    app.open_server_settings(gid);
                }
                if let Some(s) = &mut app.server {
                    s.page = Page::Members;
                }
                if ids(app).len() >= 7 && app.rects.contains_key("members:sort") {
                    self.shot(ctx, "members_table.png");
                    ok(self, "mm_rows");
                }
            }
            3111 => {
                let Some(r) = at(app, "members:sort") else { return };
                self.click(r.center());
                self.next(400);
            }
            3112 => {
                let Some(r) = menu(ctx, "Member Since (Oldest first)") else { return };
                self.click(r.center());
                self.next(1200);
            }
            3113 => {
                if ids(app).first() == Some(&107) && ids(app).last() == Some(&101) {
                    ok(self, "mm_sort");
                } else if stuck {
                    self.fail(format!("oldest first left the table as {:?}", ids(app)));
                }
            }
            3114 => {
                let Some(r) = at(app, "members:search") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("carol".into())]);
                self.next(1500);
            }
            3115 => {
                if ids(app) == [103] {
                    self.shot(ctx, "members_search.png");
                    ok(self, "mm_search");
                } else if stuck {
                    self.fail(format!("searching carol left {:?}", ids(app)));
                }
            }
            3116 => {
                let k = |pressed, key| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: if key == Key::A { Modifiers::COMMAND } else { Modifiers::NONE } };
                for key in [Key::A, Key::Backspace] {
                    self.queue.push_back(vec![k(true, key)]);
                    self.queue.push_back(vec![k(false, key)]);
                }
                self.next(1500);
            }
            3117 => {
                if ids(app).len() >= 7 {
                    ok(self, "mm_search_cleared");
                } else if stuck {
                    self.fail(format!("clearing the search left {:?}", ids(app)));
                }
            }
            3118 => {
                let Some(r) = at(app, "members:signal") else { return };
                self.click(r.center());
                self.next(400);
            }
            3119 => {
                let Some(r) = menu(ctx, "Timed out") else { return };
                self.click(r.center());
                self.next(600);
            }
            3120 => {
                let shown: Vec<Id> = app.rects.keys().filter_map(|k| k.strip_prefix("members:row:")?.parse().ok()).collect();
                if shown == [103] {
                    self.shot(ctx, "members_timed_out.png");
                    ok(self, "mm_signal_timed");
                } else if stuck {
                    self.fail(format!("the timed out filter showed {shown:?}"));
                }
            }
            3121 => {
                let Some(r) = at(app, "members:signal") else { return };
                self.click(r.center());
                self.next(400);
            }
            3122 => {
                let Some(r) = menu(ctx, "Unusual DM activity") else { return };
                self.click(r.center());
                self.next(600);
            }
            3123 => {
                let shown: Vec<Id> = app.rects.keys().filter_map(|k| k.strip_prefix("members:row:")?.parse().ok()).collect();
                if shown == [105] {
                    ok(self, "mm_signal_unusual");
                } else if stuck {
                    self.fail(format!("the unusual DM filter showed {shown:?}"));
                }
            }
            3124 => {
                let Some(r) = at(app, "members:signal") else { return };
                self.click(r.center());
                self.next(400);
            }
            3125 => {
                let Some(r) = menu(ctx, "All members") else { return };
                self.click(r.center());
                self.next(600);
            }
            3126 => {
                let Some(r) = at(app, "members:row:106") else { return };
                self.queue.push_back(vec![Event::PointerMoved(pos2(r.center().x, r.center().y))]);
                self.next(300);
            }
            3127 => {
                let Some(r) = at(app, "members:add_role:106") else { return };
                self.click(r.center());
                self.next(400);
            }
            3128 => {
                let Some(r) = menu(ctx, ROLE_ADDED) else { return };
                self.click(r.center());
                self.next(1200);
            }
            3129 => {
                let roles = row_of(app, 106).map(|m| m["member"]["roles"].clone());
                if roles.as_ref().is_some_and(|r| r.as_array().is_some_and(|r| r.iter().any(|x| x == "1101"))) {
                    ok(self, "mm_role_added");
                } else if stuck {
                    self.fail(format!("adding {ROLE_ADDED} to frank left roles {roles:?}"));
                }
            }
            3130 => {
                let Some(r) = at(app, "members:role_dot:106") else { return };
                self.click(r.center());
                self.next(1200);
            }
            3131 => {
                let roles = row_of(app, 106).map(|m| m["member"]["roles"].clone());
                if roles.as_ref().is_some_and(|r| r.as_array().is_some_and(|r| r.is_empty())) {
                    ok(self, "mm_role_removed");
                } else if stuck {
                    self.fail(format!("removing the role from frank left roles {roles:?}"));
                }
            }
            3132 => {
                let Some(r) = at(app, "members:prune") else { return };
                self.click(r.center());
                self.next(800);
            }
            3133 => {
                let Some(r) = Self::at(ctx, "prune:go") else { return };
                if app.prune.as_ref().is_some_and(|d| d.count == Some(2)) {
                    self.shot(ctx, "members_prune.png");
                    self.click(r.center());
                    self.next(1500);
                } else if stuck {
                    self.fail("the prune dialog never counted two members".into());
                }
            }
            3134 => {
                if ids(app).len() == 5 && !ids(app).contains(&106) && !ids(app).contains(&107) {
                    ok(self, "mm_prune");
                } else if stuck {
                    self.fail(format!("prune left {:?}", ids(app)));
                }
            }
            3135 => {
                let Some(r) = at(app, "members:btn:102:1") else { return };
                self.click(r.center());
                self.next(3000);
            }
            3136 => {
                let name = app.store.name_in(gid, 102);
                self.settled_menu_click(ctx, &format!("Timeout {name}"), 500);
            }
            3137 => {
                let Some(r) = Self::at(ctx, "timeout:go") else {
                    if self.step_at.elapsed() > Duration::from_secs(3) && !self.marks.iter().any(|(n, _)| n == "mm_timeout_shot") {
                        self.mark("mm_timeout_shot");
                        self.shot(ctx, "members_after_timeout_click.png");
                    }
                    return;
                };
                self.click(r.center());
                self.next(1500);
            }
            3138 => {
                let until = row_of(app, 102).and_then(|m| m["member"]["communication_disabled_until"].as_str().map(String::from));
                if until.is_some() {
                    self.shot(ctx, "members_timeout.png");
                    ok(self, "mm_timeout");
                } else if stuck {
                    self.fail("timing out bob left no communication_disabled_until".into());
                }
            }
            3139 => {
                let Some(r) = at(app, "members:btn:105:1") else { return };
                self.click(r.center());
                self.next(3000);
            }
            3140 => {
                let name = app.store.name_in(gid, 105);
                self.settled_menu_click(ctx, &format!("Kick {name}"), 600);
            }
            3141 => {
                if app.kickban.is_some() {
                    self.key(Key::Enter);
                    self.next(1500);
                }
            }
            3142 => {
                if !ids(app).contains(&105) && ids(app).len() == 4 {
                    ok(self, "mm_kick");
                } else if stuck {
                    self.fail(format!("kicking eve left {:?}", ids(app)));
                }
            }
            3143 => {
                let Some(r) = at(app, "members:btn:104:1") else { return };
                self.click(r.center());
                self.next(3000);
            }
            3144 => {
                let name = app.store.name_in(gid, 104);
                self.settled_menu_click(ctx, &format!("Ban {name}"), 600);
            }
            3145 => {
                if let Some(d) = app.kickban.as_mut() {
                    d.pick = Some(2);
                    self.key(Key::Enter);
                    self.next(1500);
                }
            }
            3146 => {
                if !ids(app).contains(&104) && ids(app).len() == 3 {
                    ok(self, "mm_ban");
                } else if stuck {
                    self.fail(format!("banning dyno left {:?}", ids(app)));
                }
            }
            3147 => {
                let Some(r) = at(app, "members:pick:101") else { return };
                self.click(r.center());
                self.next(300);
            }
            3148 => {
                let Some(r) = at(app, "members:bulk:Ban") else { return };
                self.shot(ctx, "members_selected.png");
                self.click(r.center());
                self.next(600);
            }
            3149 => {
                if app.confirm.is_some() {
                    self.key(Key::Enter);
                    self.next(1500);
                }
            }
            3150 => {
                if ids(app).contains(&101) && app.toast.is_some() {
                    ok(self, "mm_bulk_ban_owner_refused");
                } else if stuck {
                    self.fail(format!("banning the owner left {:?} and toast {:?}", ids(app), app.toast.as_ref().map(|t| &t.0)));
                }
            }
            3151 => {
                if let Some(s) = &mut app.server {
                    s.pages.members.picked.clear();
                }
                self.next(300);
            }
            3152 => {
                let (Some(a), Some(b)) = (at(app, "members:pick:102"), at(app, "members:pick:103")) else { return };
                self.click(a.center());
                self.click(b.center());
                self.next(400);
            }
            3153 => {
                let Some(r) = at(app, "members:bulk:Kick") else { return };
                self.click(r.center());
                self.next(600);
            }
            3154 => {
                if app.confirm.is_some() {
                    self.key(Key::Enter);
                    self.next(2000);
                }
            }
            3155 => {
                if ids(app) == [101] {
                    self.shot(ctx, "members_after.png");
                    ok(self, "mm_bulk_kick");
                } else if stuck {
                    self.fail(format!("bulk kick of bob and carol left {:?}", ids(app)));
                }
            }
            _ => self.step = 99,
        }
    }
}

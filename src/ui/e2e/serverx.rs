//! Steps 4800 to 4899: the leftovers of the server pages. Member applications on the account's own
//! server: the tabs, a real click on a row and on Approve, Reject with a reason, an application
//! arriving over the gateway, and the sort. `python e2e/from_step.py 4800` runs just this group.

use egui::{Event, Rect, pos2};

use super::Run;
use crate::state::{Id, Role};
use crate::ui::{App, applications, community};

const OWNED: Id = 1001;
const RULES: Id = 1001;
const BOTS: Id = 4800;
const DUO: Id = 4800;
const FEATURE: &str = "MEMBER_VERIFICATION_MANUAL_APPROVAL";

fn rect(app: &App, key: &str) -> Option<Rect> {
    app.rects.get(key).copied()
}

fn rows(app: &App, i: usize) -> usize {
    applications::counts(app, OWNED).map_or(0, |c| c.0[i])
}

fn menu(ctx: &egui::Context, label: &str) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("menu_row", label))))
}

fn first_row(app: &App) -> Option<Rect> {
    let id = app.rects.keys().filter_map(|k| k.strip_prefix("joins:row:")).next()?;
    rect(app, &format!("joins:row:{id}"))
}

impl Run {
    pub(super) fn serverx_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > std::time::Duration::from_secs(12);
        match self.step {
            4800 => {
                if let Some(g) = app.store.guilds.get_mut(&OWNED)
                    && !g.features.iter().any(|f| f == FEATURE)
                {
                    g.features.push(FEATURE.into());
                }
                app.server = None;
                app.select_guild(Some(OWNED));
                app.open_home(OWNED, community::Page::Members);
                self.next(600);
            }
            4801 => {
                let Some(r) = rect(app, "joins:tab:1") else { return };
                self.click(r.center());
                self.next(800);
            }
            4802 => {
                if rows(app, 0) < 2 {
                    if stuck {
                        self.fail(format!("the pending tab listed {} applications", rows(app, 0)));
                    }
                    return;
                }
                self.shot(ctx, "joins_pending.png");
                self.mark("joins_pending");
                if applications::counts(app, OWNED).is_some_and(|c| c.1 != 2) {
                    return self.fail("the pending count was not 2".into());
                }
                app.rects.retain(|k, _| !k.starts_with("joins:row:"));
                self.next(300);
            }
            4803 => {
                let Some(r) = first_row(app) else { return };
                self.click(r.center());
                self.next(700);
            }
            4804 => {
                let Some(r) = rect(app, "joins:approve") else {
                    if stuck {
                        self.fail("a click on an application did not open it".into());
                    }
                    return;
                };
                self.shot(ctx, "joins_detail.png");
                self.mark("joins_detail");
                self.click(r.center());
                self.next(900);
            }
            4805 => {
                if applications::counts(app, OWNED).is_some_and(|c| c.1 == 1 && c.0[0] == 1) {
                    self.mark("joins_approved");
                    self.next(300);
                } else if stuck {
                    self.fail("approving left the pending list as it was".into());
                }
            }
            4806 => {
                let Some(r) = rect(app, "joins:tab:2") else { return };
                self.click(r.center());
                self.next(800);
            }
            4807 => {
                if rows(app, 1) < 2 {
                    if stuck {
                        self.fail(format!("the rejected tab listed {}", rows(app, 1)));
                    }
                    return;
                }
                self.shot(ctx, "joins_rejected.png");
                self.mark("joins_rejected");
                let Some(r) = rect(app, "joins:tab:1") else { return };
                self.click(r.center());
                app.rects.retain(|k, _| !k.starts_with("joins:row:"));
                self.next(500);
            }
            4808 => {
                let Some(r) = first_row(app) else { return };
                self.click(r.center());
                self.next(600);
            }
            4809 => {
                let Some(r) = rect(app, "joins:reject") else { return };
                self.click(r.center());
                self.next(600);
            }
            4810 => {
                let Some(r) = rect(app, "joins:reason") else {
                    if stuck {
                        self.fail("Reject did not ask for a reason".into());
                    }
                    return;
                };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("please read the rules first".into())]);
                self.next(400);
            }
            4811 => {
                self.shot(ctx, "joins_reject_modal.png");
                let Some(r) = rect(app, "joins:reject_ok") else { return };
                self.click(r.center());
                self.next(900);
            }
            4812 => {
                if applications::counts(app, OWNED).is_some_and(|c| c.1 == 0) {
                    self.mark("joins_rejected_with_reason");
                    app.rest("POST", "/mock/join-request".into(), None, crate::net::Tag::Quiet);
                    self.next(900);
                } else if stuck {
                    self.fail("rejecting left the pending count as it was".into());
                }
            }
            4813 => {
                if applications::counts(app, OWNED).is_some_and(|c| c.1 == 1 && c.0[0] == 1) {
                    self.shot(ctx, "joins_live.png");
                    self.mark("joins_gateway_create");
                    self.next(300);
                } else if stuck {
                    self.fail("an application sent over the gateway did not reach the list".into());
                }
            }
            4814 => {
                let Some(r) = rect(app, "joins:sort") else { return };
                self.click(r.center());
                self.next(900);
            }
            4815 => {
                if applications::counts(app, OWNED).is_some_and(|c| c.2 && c.0[0] == 1) {
                    self.mark("joins_sort");
                    self.next(300);
                } else if stuck {
                    self.fail("the sort arrow did not flip the order".into());
                }
            }
            4816 => {
                if let Some(g) = app.store.guilds.get_mut(&OWNED) {
                    g.features.retain(|f| f != FEATURE);
                }
                app.home = None;
                self.next(300);
            }
            4817 => {
                if let Some(g) = app.store.guilds.get_mut(&RULES) {
                    g.roles.entry(BOTS).or_insert_with(|| Role { id: BOTS, name: "e2e role".into(), position: 1, perms: 0, ..Default::default() });
                }
                app.open_server_settings(RULES);
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Roles;
                }
                self.next(900);
            }
            4818 => {
                let Some(r) = rect(app, &format!("role:more:{BOTS}")) else { return };
                self.click(r.center());
                self.next(500);
            }
            4819 => {
                let Some(r) = menu(ctx, "View Server as Role") else {
                    if stuck {
                        self.fail(format!("the role's More menu had no View Server as Role (can={}, perms={}, menu={})", crate::ui::viewas::can(app, RULES, BOTS), app.store.my_perms(RULES), menu(ctx, "Copy Role ID").is_some()));
                    }
                    return;
                };
                self.shot(ctx, "viewas_menu.png");
                self.click(r.center());
                self.next(900);
            }
            4820 => {
                if app.store.viewing != Some((RULES, vec![BOTS])) || rect(app, "viewas:back").is_none() {
                    if stuck {
                        self.fail("View Server as Role did not start".into());
                    }
                    return;
                }
                if app.store.my_perms(RULES) == u64::MAX || app.server.is_some() {
                    return self.fail("viewing as a role left the owner's permissions on or Server Settings open".into());
                }
                self.shot(ctx, "viewas_bar.png");
                self.mark("viewas_on");
                let Some(r) = rect(app, "viewas:disable") else { return };
                self.click(r.center());
                self.next(500);
            }
            4821 => {
                if app.store.viewing.is_none() && app.store.my_perms(RULES) != 0 {
                    self.mark("viewas_off");
                    self.next(300);
                } else if stuck {
                    self.fail("Disable left the server viewed as a role".into());
                }
            }
            4822 => {
                app.open_server_settings(RULES);
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Roles;
                }
                self.next(900);
            }
            4823 => {
                let Some(r) = rect(app, &format!("role:edit:{DUO}")) else { return };
                self.click(r.center());
                self.next(700);
            }
            4824 => {
                let Some(r) = Self::at(ctx, "role:tab:2") else { return };
                self.click(r.center());
                self.next(900);
            }
            4825 => {
                let Some(r) = Self::at(ctx, "role:add_requirement") else { return };
                self.click(r.center());
                self.next(500);
            }
            4826 => {
                let Some(r) = menu(ctx, "Steam") else {
                    if stuck {
                        self.fail("Add requirement listed no Steam".into());
                    }
                    return;
                };
                self.click(r.center());
                self.next(900);
            }
            4827 => {
                let Some(r) = Self::at(ctx, "linked:steam:game_count") else {
                    if stuck {
                        self.fail("a Steam requirement showed no Games field".into());
                    }
                    return;
                };
                self.shot(ctx, "linked_fields.png");
                self.click(pos2(r.right() - 24.0, r.center().y));
                self.next(900);
            }
            4828 => {
                self.shot(ctx, "linked_games.png");
                self.mark("linked_fields");
                self.next(500);
            }
            4829 => {
                app.server = None;
                self.next(300);
            }
            4830 => {
                if let Some(g) = app.store.guilds.get_mut(&OWNED)
                    && !g.features.iter().any(|f| f == "DISCOVERABLE")
                {
                    g.features.push("DISCOVERABLE".into());
                }
                app.open_server_settings(OWNED);
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Discovery;
                }
                self.next(900);
            }
            4831 => {
                let Some(r) = Self::at(ctx, "field:discovery_tag") else {
                    if stuck {
                        self.fail("the discovery page had no tag field".into());
                    }
                    return;
                };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("rust".into())]);
                self.next(400);
            }
            4832 => {
                self.key(egui::Key::Enter);
                self.next(500);
            }
            4833 => {
                let Some(r) = Self::at(ctx, "discovery:save") else { return };
                self.shot(ctx, "discovery_listing.png");
                self.click(r.center());
                self.next(900);
            }
            4834 => {
                let saved = app.server.as_ref().and_then(|s| s.data.get("discovery_meta")).is_some_and(|m| m["keywords"] == serde_json::json!(["rust"]));
                if saved {
                    self.mark("discovery_listing");
                    if let Some(g) = app.store.guilds.get_mut(&OWNED) {
                        g.features.retain(|f| f != "DISCOVERABLE");
                    }
                    app.server = None;
                    self.next(300);
                } else if stuck {
                    self.fail("saving the discovery tags did not come back".into());
                }
            }
            _ => self.step = 3110,
        }
    }
}

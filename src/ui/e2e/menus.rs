//! Menus, dialogs and pages checked as a user meets them: the log out question, the shortcut
//! sheet, a tall menu near the bottom edge, server notification settings, Add Friend, the server
//! guide, Discovery, Add App, private threads, rail tooltips, then the same dialogs in a short
//! window.

use egui::{Event, Key, Modifiers, Rect, pos2, vec2};

use super::*;
use crate::ui::{addapp, community, keys, newserver, popups};

const LOBBY: Id = 2100;
const GAMERS: Id = 1001;

impl Run {
    fn inside(ctx: &egui::Context, r: Rect) -> bool {
        ctx.content_rect().expand(0.5).contains_rect(r)
    }

    fn settle(&mut self, ctx: &egui::Context, name: &str) -> bool {
        if self.sub == 0 {
            self.shot(ctx, name);
            self.sub = 1;
            self.step_at = Instant::now();
            return false;
        }
        if self.step_at.elapsed() < Duration::from_millis(600) {
            return false;
        }
        self.sub = 0;
        true
    }

    fn reach(&mut self, ctx: &egui::Context, key: &str) -> Option<Rect> {
        let r = Self::at(ctx, key)?;
        let m = Self::at(ctx, "modal:guild_notifs")?;
        if r.top() >= m.top() + 80.0 && r.bottom() <= m.bottom() - 110.0 {
            return Some(r);
        }
        let dir = if r.top() < m.top() + 80.0 { 150.0 } else { -150.0 };
        self.queue.push_back(vec![Event::PointerMoved(m.center()), Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0.0, dir), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
        self.wait = Instant::now() + Duration::from_millis(150);
        None
    }

    fn typed(&mut self, s: &str) {
        self.queue.push_back(vec![Event::Text(s.into())]);
    }

    pub(super) fn menu_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let guild = self.target.0;
        let general = self.target.1;
        let row = |label: &str| ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("menu_row", label))));
        match self.step {
            3800 => {
                if app.conn != Conn::Online || !app.store.channels.contains_key(&general) {
                    return;
                }
                app.select_guild(Some(guild));
                app.open(general);
                app.settings_page = Some(crate::ui::settings::Page::Account);
                self.next(800);
            }
            3801 => {
                if !self.open_page(app, "Log Out") {
                    return;
                }
                self.next(500);
            }
            3802 => {
                if app.confirm.as_ref().is_some_and(|c| c.title == "Log Out" && c.body == "Are you sure you want to logout?") && app.settings_page.is_some() && app.token.is_some() {
                    self.mark("logout_asks_first");
                    if !self.settle(ctx, "menus_logout.png") {
                    return;
                }
                }
                self.key(Key::Escape);
                self.next(400);
            }
            3803 => {
                if app.confirm.is_none() && app.token.is_some() {
                    self.mark("logout_cancel_stays_in");
                }
                app.settings_page = None;
                app.combos = Some(keys::Combos::default());
                self.next(600);
            }
            3804 => {
                let Some(panel) = Self::at(ctx, "combos:panel") else { return };
                let screen = ctx.content_rect();
                if Self::inside(ctx, panel) && panel.width() >= 800.0f32.min(screen.width() - 1.0) && panel.height() > 450.0f32.min(screen.height() * 0.5) {
                    self.mark("combos_fits");
                }
                if !self.settle(ctx, "menus_combos.png") {
                    return;
                }
                app.combos = None;
                let s = ctx.content_rect();
                app.user_menu = Some((101, guild, None, pos2(s.center().x, s.bottom() - 90.0)));
                self.next(700);
            }
            3805 => {
                let Some(block) = row("Block") else { return };
                if Self::inside(ctx, block) && row("Copy User ID").is_none_or(|r| Self::inside(ctx, r)) {
                    self.mark("tall_menu_shows_block");
                }
                if !self.settle(ctx, "menus_user_menu.png") {
                    return;
                }
                self.key(Key::Escape);
                app.user_menu = None;
                app.guild_notifs = Some(guild);
                self.next(600);
            }
            3806 => {
                let Some(r) = self.reach(ctx, "notifs:everyone") else { return };
                if !self.settle(ctx, "menus_notifs.png") {
                    return;
                }
                for key in ["notifs:mute", "notifs:roles", "notifs:highlights", "notifs:events", "notifs:mobile"] {
                    if Self::at(ctx, key).is_none() {
                        return self.fail(format!("notification settings has no {key}"));
                    }
                }
                self.click(r.center());
                self.next(350);
            }
            3807..=3811 => {
                let keys = ["notifs:roles", "notifs:highlights", "notifs:events", "notifs:mobile", "notifs:mute"];
                let Some(r) = self.reach(ctx, keys[(self.step - 3807) as usize]) else { return };
                self.click(r.center());
                self.next(350);
            }
            3812 => {
                let Some(r) = self.reach(ctx, "notifs:add") else { return };
                if !self.settle(ctx, "menus_notifs_overrides.png") {
                    return;
                }
                self.click(r.center());
                self.next(400);
            }
            3813 => {
                self.key(Key::Escape);
                let Some(r) = Self::at(ctx, "plate:Done") else { return };
                self.click(r.center());
                self.next(500);
            }
            3814 => {
                if app.guild_notifs.is_none() {
                    self.mark("notifs_done_closes");
                }
                app.select_guild(None);
                app.friends = true;
                app.friends_tab = 4;
                self.next(700);
            }
            3815 => {
                self.typed("nobody_here");
                self.next(300);
            }
            3816 => {
                if app.add_friend == "nobody_here" {
                    self.mark("add_friend_autofocus");
                }
                self.key(Key::Enter);
                self.next(900);
            }
            3817 => {
                let Some((ok, text)) = app.add_friend_note.clone() else { return };
                if !ok && text == "No one with that name could be found." {
                    self.mark("add_friend_error_line");
                }
                if !self.settle(ctx, "menus_add_friend.png") {
                    return;
                }
                self.typed("alice");
                self.next(200);
            }
            3818 => {
                self.key(Key::Enter);
                self.next(900);
            }
            3819 => {
                let Some((ok, text)) = app.add_friend_note.clone() else { return };
                if !ok && self.step_at.elapsed() < Duration::from_secs(3) {
                    return;
                }
                if ok && text.contains("alice") {
                    self.mark("add_friend_success_line");
                }
                app.friends = false;
                app.friends_tab = 1;
                app.select_guild(Some(GAMERS));
                app.open(LOBBY);
                self.next(900);
            }
            3820 => {
                app.open_home(GAMERS, community::Page::Guide);
                self.next(900);
            }
            3821 => {
                let Some(r) = Self::at(ctx, "guide:action:0") else { return };
                self.mark("server_guide_page");
                if !self.settle(ctx, "menus_guide.png") {
                    return;
                }
                self.click(r.center());
                self.next(700);
            }
            3822 => {
                if app.cur() == Some(LOBBY) && app.home.is_none() {
                    self.mark("server_guide_opens_channel");
                }
                app.open_server_settings(GAMERS);
                if let Some(s) = &mut app.server {
                    s.page = crate::ui::server::Page::Discovery;
                }
                self.next(1000);
            }
            3823 => {
                if Self::at(ctx, "button:Enable Discovery").is_none() {
                    return;
                }
                self.mark("discovery_page");
                if !self.settle(ctx, "menus_discovery.png") {
                    return;
                }
                app.server = None;
                app.open_directory();
                self.next(800);
            }
            3824 => {
                let card = json!({ "id": "9101", "name": "Rusty Racers", "install_params": { "scopes": ["bot", "applications.commands"], "permissions": "68608" } });
                app.directory = None;
                addapp::open(app, &card);
                self.next(900);
            }
            3825 => {
                let Some(r) = Self::at(ctx, "addapp:server") else { return };
                if !self.settle(ctx, "menus_add_app.png") {
                    return;
                }
                self.click(r.center());
                self.next(500);
            }
            3826 => {
                let Some(r) = row("Rust Lounge") else { return };
                self.click(r.center());
                self.next(500);
            }
            3827 => {
                let Some(r) = Self::at(ctx, "addapp:authorize") else { return };
                self.click(r.center());
                self.next(900);
            }
            3828 => {
                if app.add_app.is_none() {
                    self.mark("add_app_in_app");
                }
                app.new_thread = Some(popups::NewThread::blank(LOBBY));
                self.next(700);
            }
            3829 => {
                let Some(r) = Self::at(ctx, "new_thread:private") else { return };
                if !self.settle(ctx, "menus_private_thread.png") {
                    return;
                }
                self.click(r.center());
                self.next(400);
            }
            3830 => {
                let Some(r) = Self::at(ctx, "new_thread:create") else { return };
                self.click(r.center());
                self.next(900);
            }
            3831 => {
                app.new_thread = None;
                app.select_guild(Some(guild));
                app.open(general);
                self.next(600);
            }
            3832 => {
                let Some(r) = app.rects.get("guild:home").copied() else { return };
                self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                self.next(900);
            }
            3833 => {
                let Some((icon, tip)) = ctx.data(|d| d.get_temp::<(Rect, Rect)>(egui::Id::new(("e2e", "rail_tip")))) else { return };
                if tip.left() >= icon.right() && (tip.center().y - icon.center().y).abs() < 3.0 {
                    self.mark("rail_tip_beside_icon");
                }
                if !self.settle(ctx, "menus_rail_tip.png") {
                    return;
                }
                app.select_guild(None);
                app.open_directory();
                self.next(900);
            }
            3834 => {
                let title = ctx.data(|d| d.get_temp::<String>(egui::Id::new(("e2e", "title")))).unwrap_or_default();
                if title == "Discover" {
                    self.mark("title_says_discover");
                }
                if !self.settle(ctx, "menus_discover.png") {
                    return;
                }
                app.directory = None;
                app.open_quests();
                self.next(900);
            }
            3835 => {
                let title = ctx.data(|d| d.get_temp::<String>(egui::Id::new(("e2e", "title")))).unwrap_or_default();
                if title == "Quests" {
                    self.mark("title_says_quests");
                }
                app.quests = None;
                app.settings.streamer = true;
                app.settings.hide_personal = true;
                app.save_settings(ctx);
                app.select_guild(Some(guild));
                app.open(general);
                app.open_profile(101, guild, pos2(300.0, 160.0));
                self.next(900);
            }
            3836 => {
                if app.rects.contains_key("popout_note") {
                    return self.fail("the profile popout showed a note in streamer mode".into());
                }
                self.mark("streamer_hides_note");
                app.settings.streamer = false;
                app.save_settings(ctx);
                app.profile = None;
                self.next(400);
            }
            3837 => {
                app.start_search(general, "pinned:true before:2026-01-01 has:image".into());
                app.set_status_for("idle", Some(900));
                self.next(600);
            }
            3838 => {
                app.set_status_for("online", None);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(1200);
            }
            3839 => {
                app.combos = Some(keys::Combos::default());
                self.next(600);
            }
            3840 => {
                let Some(panel) = Self::at(ctx, "combos:panel") else { return };
                if Self::inside(ctx, panel) {
                    self.mark("combos_fits_short");
                }
                if !self.settle(ctx, "menus_combos_short.png") {
                    return;
                }
                app.combos = None;
                app.wizard = Some(newserver::Wizard::new(app));
                self.next(700);
            }
            3841 => {
                let Some(m) = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("e2e", "modal:new_server")))) else { return };
                if Self::inside(ctx, m) {
                    self.mark("new_server_fits_short");
                }
                if !self.settle(ctx, "menus_new_server_short.png") {
                    return;
                }
                app.wizard = None;
                app.switcher = Some("@".into());
                self.next(700);
            }
            3842 => {
                if !self.settle(ctx, "menus_switcher_short.png") {
                    return;
                }
                app.switcher = None;
                app.guild_notifs = Some(guild);
                self.next(700);
            }
            3843 => {
                let Some(m) = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("e2e", "modal:guild_notifs")))) else { return };
                if Self::inside(ctx, m) {
                    self.mark("notifs_fits_short");
                }
                if !self.settle(ctx, "menus_notifs_short.png") {
                    return;
                }
                app.guild_notifs = None;
                app.open_directory();
                self.next(900);
            }
            3844 => {
                if !self.settle(ctx, "menus_discover_short.png") {
                    return;
                }
                app.directory = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 800.0)));
                self.step = 99;
            }
            _ => self.step = 99,
        }
    }
}

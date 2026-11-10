//! The profile editor against the mock, in steps 3900 and up: every tile opens its picker, each
//! picker's choice lands in the draft, the card edits in place, widgets are added, and Save
//! writes bodies discord would accept. Then the same profile on a full profile, with its effect
//! playing, at the sizes the app runs at.

use egui::{Event, Key, Modifiers, vec2};
use serde_json::json;
use std::time::{Duration, Instant};

use super::Run;
use crate::ui::App;
use crate::ui::profed::{Dlg, Menu};
use crate::ui::settings::Page;

impl Run {
    pub(super) fn profed_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let at = |key: &str| Self::at(ctx, key);
        let area = |key: &str| Self::at(ctx, key).unwrap_or(ctx.content_rect());
        // clicks a mark, scrolling its scroller first when it sits outside what shows
        let reach = |this: &mut Self, key: &str, scroller: &str, pause: u64| {
            let Some(r) = at(key) else { return };
            let view = area(scroller);
            if !view.contains_rect(r) {
                this.queue.push_back(vec![Event::PointerMoved(view.center())]);
                this.queue.push_back(vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, if r.top() > view.center().y { -160.0 } else { 160.0 }),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                }]);
                this.wait = Instant::now() + Duration::from_millis(200);
                return;
            }
            this.click(r.center());
            this.next(pause);
        };
        let panel = |this: &mut Self, key: &str| reach(this, key, "profed_panel", 350);
        let region = |this: &mut Self, key: &str| reach(this, key, "profed_region", 350);
        let click = |this: &mut Self, key: &str| {
            let Some(r) = at(key) else { return };
            this.click(r.center());
            this.next(350);
        };
        let dlg = |app: &App| app.profed.as_ref().map(|e| match &e.dlg {
            Dlg::None => 0,
            Dlg::Avatar => 1,
            Dlg::Pick(..) => 2,
            Dlg::Style(_) => 3,
            Dlg::Colour(..) => 4,
            Dlg::Widget(_) => 5,
            Dlg::Browse => 6,
        });
        let draft = |app: &App| app.profed.as_ref().map(|e| e.draft.clone());
        let tag_guild = app.store.guild_order.iter().copied().find(|g| app.store.guilds.get(g).is_some_and(|g| g.features.iter().any(|f| f == "GUILD_TAGS")));
        match self.step {
            3900 => {
                app.store.me_raw["premium_type"] = json!(2);
                app.settings_page = Some(Page::Profiles);
                self.next(600);
            }
            3901 => {
                if !app.profed.as_ref().is_some_and(|e| e.ready) {
                    return;
                }
                let keys = ["profile_select", "hide_styles", "tile:nameplate", "tile:avatar", "tile:decoration", "tile:dns", "swatch:primary", "swatch:accent", "tile:banner", "tile:effect", "tile:frame", "add_status", "profile:pronouns", "profile:bio", "browse_widgets", "widget:favorite_games", "widget:played_games", "profed_tab:Board", "profed_close"];
                if keys.iter().any(|k| at(k).is_none()) || at("widget_app:9001").is_none() || at("widget_app:9004").is_none() {
                    return;
                }
                self.mark("profed_layout");
                self.shot(ctx, "profed.png");
                self.next(500);
            }
            3902 => panel(self, "tile:decoration"),
            3903 => {
                if dlg(app) == Some(2) && at("collectible:7001").is_some() && at("collectible:7101").is_some() && at("collectible:7201").is_some() {
                    self.mark("profed_pick_sections");
                    self.shot(ctx, "profed_decoration.png");
                    click(self, "collectible:7001");
                }
            }
            3904 => click(self, "pick:apply"),
            3905 => {
                if draft(app).is_some_and(|d| d.decoration.as_ref().is_some_and(|i| i.sku == "7001")) && dlg(app) == Some(0) {
                    self.mark("profed_decoration_applied");
                }
                self.step = 3910;
            }
            3910 => panel(self, "tile:effect"),
            3911 => {
                if dlg(app) == Some(2) && at("collectible:7003").is_some() {
                    self.shot(ctx, "profed_effect.png");
                    click(self, "collectible:7003");
                }
            }
            3912 => {
                if crate::gif::shown("effect_7003") >= 1 {
                    self.mark("profed_effect_preview_plays");
                }
                click(self, "pick:apply");
            }
            3913 => {
                if draft(app).is_some_and(|d| d.effect.as_ref().is_some_and(|i| i.sku == "7003")) {
                    self.mark("profed_effect_applied");
                }
                self.next(1200);
            }
            3914 => {
                self.shot(ctx, "profed_effect_on_card.png");
                if crate::gif::shown("effect_7003_a") >= 2 || crate::gif::shown("effect_7003_b") >= 2 {
                    self.mark("profed_effect_animates");
                }
                self.next(0);
            }
            3915 => panel(self, "tile:frame"),
            3916 => {
                if dlg(app) == Some(2) && at("collectible:7005").is_some() {
                    click(self, "collectible:7005");
                }
            }
            3917 => click(self, "pick:apply"),
            3918 => {
                if draft(app).is_some_and(|d| d.frame.as_ref().is_some_and(|i| i.sku == "7005")) {
                    self.mark("profed_frame_applied");
                }
                self.step = 3920;
            }
            3920 => panel(self, "tile:nameplate"),
            3921 => {
                if dlg(app) == Some(2) && at("collectible:7002").is_some() {
                    self.shot(ctx, "profed_nameplate.png");
                    click(self, "collectible:7002");
                }
            }
            3922 => click(self, "pick:apply"),
            3923 => {
                if draft(app).is_some_and(|d| d.nameplate.as_ref().is_some_and(|i| i.sku == "7002")) {
                    self.mark("profed_nameplate_applied");
                }
                self.step = 3930;
            }
            3930 => panel(self, "tile:avatar"),
            3931 => {
                if dlg(app) == Some(1) && at("avatar:recent:1").is_some() {
                    self.mark("profed_recent_avatars");
                    self.shot(ctx, "profed_avatar.png");
                    click(self, "avatar:recent:1");
                }
            }
            3932 => {
                if draft(app).is_some_and(|d| matches!(d.avatar, crate::ui::profed::Pic::Recent(ref id, _) if id == "2")) && dlg(app) == Some(0) {
                    self.mark("profed_recent_avatar_picked");
                }
                self.step = 3935;
            }
            3935 => panel(self, "tile:avatar"),
            3936 => {
                if dlg(app) == Some(1) {
                    if let Some(r) = at("avatar:recent:0") {
                        self.queue.push_back(vec![Event::PointerMoved(r.center())]);
                        self.next(300);
                    }
                }
            }
            3937 => click(self, "avatar:recent_x:0"),
            3938 => {
                if let Some(c) = app.confirm.take() {
                    self.mark("profed_recent_delete_asks");
                    (c.run)(app);
                    self.next(500);
                }
            }
            3939 => {
                if app.profed.as_mut().is_some_and(|e| matches!(e.dlg, Dlg::Avatar)) {
                    app.profed.as_mut().map(|e| e.dlg = Dlg::None);
                }
                self.next(200);
            }
            3940 => panel(self, "tile:dns"),
            3941 => {
                if dlg(app) == Some(3) && at("style:font:7").is_some() {
                    self.shot(ctx, "profed_style.png");
                    click(self, "style:font:7");
                }
            }
            3942 => click(self, "style:effect:2"),
            3943 => click(self, "style:preset:0:3"),
            3944 => click(self, "style:preset:1:5"),
            3945 => {
                self.shot(ctx, "profed_style_picked.png");
                click(self, "style:surprise");
            }
            3946 => {
                if dlg(app) == Some(3) {
                    self.mark("profed_surprise_me");
                    click(self, "style:font:7");
                }
            }
            3947 => click(self, "style:effect:2"),
            3948 => click(self, "style:preset:0:3"),
            3949 => click(self, "style:preset:1:5"),
            3950 => click(self, "style:apply"),
            3951 => {
                let ok = draft(app).and_then(|d| d.style).is_some_and(|s| s.font == 7 && s.effect == 2 && s.colors == vec![crate::ui::profed::PRESETS[3], crate::ui::profed::PRESETS[5]]);
                if ok {
                    self.mark("profed_style_applied");
                }
                self.step = 3955;
            }
            3955 => panel(self, "swatch:primary"),
            3956 => {
                if dlg(app) == Some(4) {
                    if let Some(r) = at("colour:hex") {
                        self.click(r.center());
                        self.queue.push_back(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
                        self.queue.push_back(vec![Event::Text("#00b89a".into())]);
                        self.next(400);
                    }
                }
            }
            3957 => {
                self.shot(ctx, "profed_colour.png");
                click(self, "colour:done");
            }
            3958 => {
                if draft(app).is_some_and(|d| d.theme[0] == Some(0x00b89a)) {
                    self.mark("profed_theme_colour");
                }
                self.next(200);
            }
            3959 => click(self, "edit_banner"),
            3960 => {
                if app.profed.as_ref().is_some_and(|e| e.menu == Some(Menu::Banner)) && at("banner_menu:2").is_some() {
                    self.mark("profed_banner_menu");
                    self.shot(ctx, "profed_banner_menu.png");
                    click(self, "banner_menu:1");
                }
            }
            3961 => {
                if dlg(app) == Some(2) {
                    self.mark("profed_banner_menu_opens_effects");
                    click(self, "pick:cancel");
                }
            }
            3962 => {
                if dlg(app) == Some(0) {
                    self.next(100);
                }
            }
            3963 => click(self, "tag_select"),
            3964 => {
                if app.profed.as_ref().is_some_and(|e| e.menu == Some(Menu::Tag)) && at("tag:search").is_some() && at("tag:none").is_some() {
                    self.mark("profed_tag_menu");
                    self.shot(ctx, "profed_tag_menu.png");
                    let Some(g) = tag_guild else { return self.fail("no server with tags in the mock".into()) };
                    if at(&format!("tag:{g}")).is_some() {
                        click(self, &format!("tag:{g}"));
                    }
                }
            }
            3965 => {
                if draft(app).is_some_and(|d| d.tag.is_some()) {
                    self.mark("profed_tag_picked");
                }
                self.next(100);
            }
            3966 => click(self, "profile:pronouns"),
            3967 => {
                self.queue.push_back(vec![Event::Text("they/them".into())]);
                self.next(300);
            }
            3968 => click(self, "profile:bio"),
            3969 => {
                self.queue.push_back(vec![Event::Text("hello from the editor".into())]);
                self.next(300);
            }
            3970 => {
                if draft(app).is_some_and(|d| d.pronouns == "they/them" && d.bio.contains("hello from the editor")) {
                    self.mark("profed_card_edits");
                }
                click(self, "add_status");
            }
            3971 => {
                if app.status_text.is_some() {
                    self.mark("profed_status_opens");
                    app.status_text = None;
                }
                self.next(200);
            }
            3972 => region(self, "widget:favorite_games"),
            3973 => {
                if dlg(app) == Some(5) && at("widget_game:9001").is_some() {
                    self.shot(ctx, "profed_widget.png");
                    click(self, "widget_game:9001");
                }
            }
            3974 => click(self, "widget:save"),
            3975 => {
                if draft(app).is_some_and(|d| d.widgets.iter().any(|w| w.slot == crate::ui::profed::Slot::Favorite && w.games.len() == 1)) {
                    self.mark("profed_widget_added");
                }
                self.next(200);
            }
            3976 => region(self, "widget:played_games"),
            3977 => {
                if dlg(app) == Some(5) && at("widget_game:9002").is_some() {
                    click(self, "widget_game:9002");
                }
            }
            3978 => click(self, "widget_game:9005"),
            3979 => click(self, "widget:save"),
            3980 => {
                if draft(app).is_some_and(|d| d.widgets.iter().any(|w| w.slot == crate::ui::profed::Slot::Played && w.games.len() == 2)) {
                    self.mark("profed_widget_games_added");
                }
                self.next(200);
            }
            3981 => region(self, "browse_widgets"),
            3982 => {
                if dlg(app) == Some(6) {
                    self.mark("profed_browse_widgets");
                    self.shot(ctx, "profed_browse.png");
                    self.key(Key::Escape);
                    self.next(300);
                }
            }
            3983 => region(self, "profed_tab:Wishlist"),
            3984 => {
                self.shot(ctx, "profed_wishlist.png");
                self.mark("profed_tabs");
                self.next(0);
            }
            3985 => region(self, "profed_tab:Board"),
            3986 => {
                self.shot(ctx, "profed_dirty.png");
                if at("save_changes").is_some() {
                    self.mark("profed_save_bar");
                }
                click(self, "save_changes");
            }
            3987 => {
                if app.profed.as_ref().is_some_and(|e| e.ready && !e.saving && e.draft == e.base) {
                    self.mark("profed_saved");
                    self.next(300);
                }
            }
            3988 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(940.0, 500.0)));
                self.next(900);
            }
            3989 => {
                self.shot(ctx, "profed_940x500.png");
                self.next(500);
            }
            3990 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(1280.0, 720.0)));
                self.next(900);
            }
            3991 => {
                self.shot(ctx, "profed_1280x720.png");
                self.next(500);
            }
            3992 => {
                click(self, "hide_styles");
            }
            3993 => {
                if app.profed.as_ref().is_some_and(|e| e.hide) {
                    self.mark("profed_hide_styles");
                    self.shot(ctx, "profed_hidden.png");
                }
                if let Some(e) = &mut app.profed {
                    e.hide = false;
                }
                self.next(200);
            }
            3994 => {
                app.profed = None;
                app.settings_page = None;
                app.open_profile(101, 0, egui::pos2(300.0, 160.0));
                self.next(1500);
            }
            3995 => {
                if app.profiles.get(&101).is_none_or(|p| p.is_null()) || !app.cosmo.items.contains_key("7003") {
                    return;
                }
                self.mark("profile_effect_resolved");
                self.shot(ctx, "profed_popout.png");
                self.next(700);
            }
            3996 => {
                self.shot(ctx, "profed_popout_later.png");
                if app.rects.contains_key("view_full_bio") && app.rects.contains_key("game_collection") {
                    self.mark("profile_popout_extras");
                }
                if crate::gif::shown("effect_7003_b") >= 2 || crate::gif::shown("effect_7003_a") >= 2 {
                    self.mark("profile_effect_plays_on_profile");
                }
                let Some(h) = app.rects.get("profile_head").copied() else { return };
                self.click(h.left_top() + vec2(56.0, 101.0));
                self.next(400);
            }
            3997 => {
                if app.viewer.is_some() {
                    self.mark("profile_view_picture");
                    app.viewer = None;
                }
                let Some(r) = app.rects.get("game_collection").copied() else { return };
                self.click(r.center());
                self.next(500);
            }
            3998 => {
                if app.full_profile.as_ref().is_some_and(|f| f.tab == crate::ui::profile::Tab::Board) {
                    self.mark("profile_board_tab");
                    if app.store.rel_since.contains_key(&101) {
                        self.mark("profile_friends_since");
                    }
                    self.shot(ctx, "profed_full_board.png");
                    let Some(r) = Self::at(ctx, &format!("fp_tab:{}", crate::ui::profile::Tab::Wishlist as u8)) else { return };
                    self.click(r.center());
                    self.next(700);
                }
            }
            3999 => {
                if app.full_profile.as_ref().is_some_and(|f| f.tab == crate::ui::profile::Tab::Wishlist) {
                    self.mark("profile_wishlist_tab");
                    self.shot(ctx, "profed_full_wishlist.png");
                    app.full_profile = None;
                    self.next(200);
                }
            }
            4000 => {
                app.sform.profile_guild = Some(1001);
                app.settings_page = Some(Page::Profiles);
                self.next(800);
            }
            4001 => {
                if !app.profed.as_ref().is_some_and(|e| e.ready && e.guild == Some(1001)) {
                    return;
                }
                self.mark("profed_server_profile");
                self.shot(ctx, "profed_server.png");
                click(self, "profile:pronouns");
            }
            4002 => {
                self.queue.push_back(vec![Event::Text("ze/zir".into())]);
                self.next(300);
            }
            4003 => click(self, "save_changes"),
            4004 => {
                if app.profed.as_ref().is_some_and(|e| e.ready && !e.saving && e.draft == e.base) {
                    self.mark("profed_server_saved");
                    app.profed = None;
                    app.settings_page = None;
                    self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 4100 };
                }
            }
            _ => self.step = 4100,
        }
    }
}

use egui::load::TexturePoll;
use super::Run;
use crate::ui::{App, settings::Page};

impl Run {
    pub(super) fn profile_check(&mut self, app: &mut App, ctx: &egui::Context) {
        if std::env::var_os("RUSTCORD_READONLY").is_none() {
            return self.fail("profile check requires RUSTCORD_READONLY".into());
        }
        match self.step {
            0 => {
                if app.conn != crate::net::Conn::Online {
                    return;
                }
                app.settings_page = Some(Page::Profiles);
                self.next(600);
            }
            1 => {
                let Some(ed) = app.profed.as_ref().filter(|e| e.ready) else { return };
                let mut urls = vec![("avatar", ed.draft.avatar_url.as_str())];
                if let Some(url) = ed.draft.banner_url.as_deref() {
                    urls.push(("banner", url));
                }
                for (name, url) in urls {
                    match ctx.try_load_texture(url, Default::default(), Default::default()) {
                        Ok(TexturePoll::Ready { .. }) => {}
                        Ok(TexturePoll::Pending { .. }) => return,
                        Err(e) => return self.fail(format!("editor {name}: {e}")),
                    }
                }
                if ed.draft.banner_url.is_some() && Self::at(ctx, "profed_backdrop").is_none() {
                    return;
                }
                for id in ["1346069614634864772", "1323482066758930452"] {
                    if Self::at(ctx, &format!("widget_app:{id}")).is_none() {
                        return;
                    }
                }
                self.mark("profile_assets_loaded");
                self.shot(ctx, "profile_editor.png");
                self.next(300);
            }
            _ => {
                if self.pending_shots == 0 {
                    self.step = 99;
                }
            }
        }
    }
}

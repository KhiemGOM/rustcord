use super::*;

impl Run {
    pub(super) fn refusal_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        use crate::ui::server::Page;
        let page = |app: &mut App, p: Page| {
            if let Some(s) = &mut app.server {
                s.page = p;
            }
        };
        let errors = |app: &App| app.server.as_ref().map(|s| s.errors.clone()).unwrap_or_default();
        let paused = |app: &App| app.store.guilds.get(&1001).is_some_and(|g| g.invites_paused());
        match self.step {
            3400 => {
                app.open_server_settings(1001);
                self.next(800);
            }
            3401 => {
                if !app.server.as_ref().is_some_and(|s| s.data.get("profile").is_some_and(|v| !v.is_null())) {
                    return;
                }
                page(app, Page::Tag);
                self.next(600);
            }
            3402 => {
                let Some(r) = Self::at(ctx, "field:tag") else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
                self.queue.push_back(vec![Event::Text("A".into())]);
                self.next(500);
            }
            3403 => {
                let Some(r) = Self::at(ctx, "bar:Save Changes") else { return };
                self.click(r.center());
                self.next(300);
            }
            3404 => {
                let Some(e) = errors(app).into_iter().find(|e| e.0 == "tag") else { return };
                if !app.toast.as_ref().is_some_and(|(t, _)| t.starts_with(&e.1)) {
                    return self.fail(format!("the toast didn't say {:?}", e.1));
                }
                self.shot(ctx, "refused_tag.png");
                self.mark("refused_tag");
                self.next(400);
            }
            3405 => {
                let Some(r) = Self::at(ctx, "bar:Reset") else { return };
                self.click(r.center());
                self.next(300);
            }
            3406 => {
                if !errors(app).is_empty() || app.server.as_ref().is_some_and(|s| s.dirty) {
                    return;
                }
                self.mark("refused_reset");
                page(app, Page::Invites);
                self.next(600);
            }
            3407 => {
                let Some(r) = Self::at(ctx, "button:Pause Invites") else { return };
                self.click(r.center());
                self.next(300);
            }
            3408 => {
                if !paused(app) {
                    return;
                }
                self.mark("invites_paused");
                self.next(400);
            }
            3409 => {
                let Some(r) = Self::at(ctx, "button:Resume Invites") else { return };
                self.shot(ctx, "invites_paused.png");
                self.click(r.center());
                self.next(300);
            }
            3410 => {
                if paused(app) {
                    return;
                }
                self.mark("invites_resumed");
                app.server = None;
                self.step = 4000;
            }
            _ => self.step = 4000,
        }
    }
}

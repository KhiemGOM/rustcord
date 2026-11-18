//! Steps 5320 to 5329: discord's survey notice. `RUSTCORD_E2E_TO=1 python e2e/from_step.py 5320`
//! runs just this group.

use std::time::Duration;

use super::Run;
use crate::ui::App;

const PROMPT: &str = "How is blocking users on Discord working for you?";

impl Run {
    pub(super) fn survey_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let stuck = self.step_at.elapsed() > Duration::from_secs(10);
        match self.step {
            5320 => {
                app.select_guild(None);
                app.ui.survey_fetched = 0;
                app.ui.survey_seen = 0;
                app.ui.surveys_hidden.clear();
                app.rest("POST", "/mock/survey".into(), None, crate::net::Tag::Quiet);
                self.next(500);
            }
            5321 => {
                crate::ui::settings::refetch(app, "survey");
                self.next(0);
            }
            5322 => {
                let (Some(bar), Some(go), Some(close)) = (Self::at(ctx, "notice:bar"), Self::at(ctx, "notice:button"), Self::at(ctx, "notice:close")) else {
                    if stuck {
                        self.shot(ctx, "survey_missing.png");
                        self.step = 5329;
                    }
                    return;
                };
                let near = |a: f32, b: f32| (a - b).abs() < 0.6;
                let side = app.rects.get("home:Friends").copied();
                if Self::said(ctx, "notice:text").as_deref() == Some(PROMPT)
                    && near(bar.left(), 72.0)
                    && near(bar.height(), 36.0)
                    && near(bar.right(), ctx.content_rect().right())
                    && near(close.right(), bar.right())
                    && near(go.height(), 24.0)
                    && side.is_some_and(|s| s.top() > bar.bottom())
                {
                    self.mark("survey_notice");
                }
                self.shot(ctx, "survey.png");
                self.click(close.center());
                self.next(500);
            }
            5323 => {
                if Self::at(ctx, "notice:bar").is_none() && app.ui.surveys_hidden.iter().any(|k| k == "2024-11_blocking") {
                    self.mark("survey_closed");
                } else if !stuck {
                    return;
                }
                self.next(0);
            }
            _ => self.step = if std::env::var_os("RUSTCORD_E2E_TO").is_some() { 9999 } else { 5400 },
        }
    }
}

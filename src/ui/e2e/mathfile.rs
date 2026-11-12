use egui::{Context, Key};
use serde_json::json;

use super::Run;
use crate::ui::App;

impl Run {
    pub(super) fn mathfile_steps(&mut self, app: &mut App, ctx: &Context) {
        let at = |key: &str| Self::at(ctx, key);
        match self.step {
            1890 => {
                let Ok(url) = std::env::var("RUSTCORD_E2E_MATH_URL") else { return self.fail("math fixture URL missing".into()) };
                app.select_guild(Some(self.target.0));
                app.open(2060);
                app.rest("POST", "/mock/event".into(), Some(json!({ "t": "MESSAGE_CREATE", "d": {
                    "id": "2000000000000000999", "channel_id": "2060", "guild_id": self.target.0.to_string(),
                    "author": { "id": "101", "username": "Mira", "discriminator": "0" },
                    "content": "Markdown math", "type": 0, "embeds": [], "mentions": [], "mention_roles": [],
                    "attachments": [{ "id": "42", "filename": "formula.md", "url": url, "proxy_url": url, "size": include_str!("../../../e2e/fixtures/math.md").len(), "content_type": "text/markdown" }]
                }})), crate::net::Tag::Quiet);
                app.scroll_fix = Some((2060, 1e9));
                self.next(700);
            }
            1891 => {
                let Some(formula) = at("math:false:\\int_0^\\infty e^{-x^2}\\,dx") else { return };
                if at("math:true:x^2 + y^2 = z^2").is_none() {
                    return;
                }
                self.rects.insert("display_math_size".into(), json!([formula.width(), formula.height()]));
                if formula.height() < 40.0 || formula.width() < 80.0 {
                    return self.fail("display math was shrunk to an inline line height".into());
                }
                if at("textfile:rendered").is_none_or(|body| !body.contains_rect(formula)) {
                    return self.fail("the integral was clipped in the attachment preview".into());
                }
                if at("math:true:literal").is_some() || at("math:true:also_literal").is_some() {
                    return self.fail("math was rendered inside a code span or fence".into());
                }
                self.mark("markdown_math");
                self.shot(ctx, "markdown_math.png");
                let Some(r) = at("textfile:source") else { return };
                self.click(r.center());
                self.next(300);
            }
            1892 => {
                if at("textfile:rendered").is_some() {
                    return self.fail("Markdown source toggle did not switch to source".into());
                }
                self.mark("markdown_source");
                self.shot(ctx, "markdown_source.png");
                let Some(r) = at("textfile:source") else { return };
                self.click(r.center());
                self.next(300);
            }
            1893 => {
                if at("textfile:rendered").is_none() { return }
                let Some(r) = at("textfile:whole") else { return };
                self.click(r.center());
                self.next(400);
            }
            1894 => {
                if at("textfile:whole_rendered").is_none() { return }
                self.mark("markdown_whole");
                self.shot(ctx, "markdown_whole.png");
                self.key(Key::Escape);
                self.next(200);
            }
            _ => {
                if self.pending_shots == 0 { self.step = 9999; }
            }
        }
    }
}

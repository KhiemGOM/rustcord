//! The composer's spellcheck (against the word list in e2e/spell_words.txt) and the native
//! Discover > Servers page, in steps 1700 and up.

use super::*;

impl Run {
    pub(super) fn spell_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let row = |label: &str| ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("menu_row", label))));
        let menu = egui::Popup::is_any_open(ctx);
        let flagged = app.spell.flagged(&app.settings);
        match self.step {
            1700 => {
                app.select_guild(Some(self.target.0));
                app.open(general);
                self.next(800);
            }
            1701 => {
                let Some(r) = app.rects.get("composer").copied() else { return };
                self.click(r.center());
                self.queue.push_back(vec![Event::Text("helo wrld ".into())]);
                self.next(700);
            }
            1702 => {
                let Some(w) = app.spell.first.filter(|_| flagged == 2) else { return };
                self.mark("spell_squiggles");
                self.shot(ctx, "spell_squiggles.png");
                self.rclick(w.center());
                self.next(500);
            }
            1703 => {
                let (Some(pick), Some(_), Some(_)) = (row("hello"), row("Add to Dictionary"), row("Spellcheck")) else { return };
                if !menu {
                    return;
                }
                self.mark("spell_menu");
                self.shot(ctx, "spell_menu.png");
                self.click(pick.center());
                self.next(500);
            }
            1704 => {
                let Some(w) = app.spell.first.filter(|_| flagged == 1 && app.drafts.get(&general).is_some_and(|d| d == "hello wrld ")) else { return };
                self.mark("spell_replace");
                self.rclick(w.center());
                self.next(500);
            }
            1705 => {
                let Some(add) = row("Add to Dictionary").filter(|_| menu) else { return };
                self.click(add.center());
                self.next(500);
            }
            1706 => {
                if !app.settings.learned_words.iter().any(|w| w == "wrld") || flagged != 0 {
                    return;
                }
                self.mark("spell_learn");
                self.key(Key::End);
                self.queue.push_back(vec![Event::Text("qwzx ".into())]);
                self.next(700);
            }
            1707 => {
                let Some(w) = app.spell.first.filter(|_| flagged == 1) else { return };
                self.rclick(w.center());
                self.next(500);
            }
            1708 => {
                let Some(sw) = row("Spellcheck").filter(|_| menu) else { return };
                self.click(sw.center());
                self.next(500);
            }
            1709 => {
                let Some(t) = app.rects.get("composer_text").copied().filter(|_| !app.settings.spellcheck && flagged == 0) else { return };
                self.mark("spell_off");
                self.rclick(t.min + vec2(8.0, 8.0));
                self.next(500);
            }
            1710 => {
                let Some(sw) = row("Spellcheck").filter(|_| menu) else { return };
                self.click(sw.center());
                self.next(500);
            }
            1711 => {
                if !app.settings.spellcheck {
                    return;
                }
                self.mark("spell_on");
                app.drafts.remove(&general);
                app.settings.learned_words.clear();
                app.save_settings(ctx);
                self.step = 1720;
            }
            _ => self.step = 99,
        }
    }

    pub(super) fn discover_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let feed = |app: &App, k: &str| app.rects.get(&format!("discover:feed:{k}")).copied();
        match self.step {
            1720 => {
                app.open_servers();
                self.next(800);
            }
            1721 => {
                let (Some(_), Some(tab)) = (feed(app, "home"), app.rects.get("discover:tab:1").copied()) else { return };
                self.mark("discover_home");
                self.shot(ctx, "discover_home.png");
                self.click(tab.center());
                self.next(800);
            }
            1722 => {
                let (Some(_), Some(search)) = (feed(app, "c:1"), app.rects.get("discover:search").copied()) else { return };
                self.mark("discover_category");
                self.shot(ctx, "discover_gaming.png");
                self.click(search.center());
                self.queue.push_back(vec![Event::Text("rust".into())]);
                self.key(Key::Enter);
                self.next(800);
            }
            1723 => {
                let Some(card) = feed(app, "q:rust") else { return };
                self.mark("discover_search");
                self.shot(ctx, "discover_search.png");
                self.click(card.center());
                self.next(800);
            }
            1724 => {
                if app.directory.is_some() {
                    return;
                }
                self.mark("discover_preview");
                self.step = 99;
            }
            _ => self.step = 99,
        }
    }
}

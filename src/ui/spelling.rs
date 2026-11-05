//! The composer's spellcheck as discord's desktop app draws it: red squiggles under misspelled
//! words, and a right-click menu with the suggestions, Add to Dictionary, the Spellcheck switch
//! and the usual cut, copy and paste.

use egui::text::CCursor;
use egui::text_selection::CCursorRange;
use egui::text_edit::TextEditOutput;
use egui::{Pos2, Shape, Stroke, Ui, pos2};

use super::{Trail, menu_body, menu_closing, menu_item, menu_row, menu_sep};
use crate::cache::Settings;
use crate::spell::{self, Speller};
use crate::theme::RED;

type Word = (usize, usize, String);

#[derive(Default)]
pub struct State {
    speller: Option<Speller>,
    text: String,
    words: Vec<Word>,
    hit: Option<Word>,
    learned_hit: Option<String>,
    sel: Option<(usize, usize)>,
    at: usize,
}

enum Act {
    Replace(String),
    Learn(String),
    Forget(String),
    Toggle,
    Cut,
    Copy,
    Paste,
}

const HELP: &str = "https://support.discord.com/hc/en-us/articles/360000877191";

impl State {
    pub fn flagged(&self, s: &Settings) -> usize {
        self.words.iter().filter(|w| self.is_bad(w, s)).count()
    }

    fn is_bad(&self, w: &Word, s: &Settings) -> bool {
        s.spellcheck && self.speller.as_ref().is_some_and(|sp| sp.bad(&w.2)) && !s.learned_words.contains(&w.2.to_lowercase())
    }
}

/// Runs after the composer's TextEdit: checks new text, paints the squiggles and serves the
/// right-click menu. True when a setting changed and wants saving.
pub fn composer(st: &mut State, s: &mut Settings, ui: &mut Ui, out: &TextEditOutput, draft: &mut String, tid: egui::Id) -> bool {
    let ctx = ui.ctx().clone();
    if s.spellcheck && st.speller.as_ref().is_none_or(|sp| sp.tag != s.locale) {
        st.speller = Some(Speller::new(&s.locale, &ctx));
        st.text.clear();
    }
    if let Some(sp) = st.speller.as_mut().filter(|_| s.spellcheck) {
        sp.poll();
        if st.text != *draft {
            st.text.clone_from(draft);
            st.words = spell::words(draft);
            sp.ask(st.words.iter().map(|w| w.2.as_str()));
        }
    }
    let focused = out.response.has_focus();
    let cursor = out.cursor_range.map(|r| r.primary.index.0);
    let pos = |i: usize| out.galley.pos_from_cursor(CCursor::new(i));
    for w in &st.words {
        if !st.is_bad(w, s) || (focused && cursor.is_some_and(|c| (w.0..=w.1).contains(&c))) {
            continue;
        }
        let (a, b) = (pos(w.0), pos(w.1));
        if (a.top() - b.top()).abs() > 1.0 {
            continue;
        }
        let (x0, x1, y) = (out.galley_pos.x + a.left(), out.galley_pos.x + b.left(), out.galley_pos.y + a.bottom() - 1.0);
        let mut pts: Vec<Pos2> = Vec::new();
        let mut x = x0;
        while x < x1 {
            pts.push(pos2(x, y - if pts.len() % 2 == 0 { 1.5 } else { 0.0 }));
            x += 2.0;
        }
        pts.push(pos2(x1, y - if pts.len() % 2 == 0 { 1.5 } else { 0.0 }));
        ui.painter().add(Shape::line(pts, Stroke::new(1.0, RED)));
    }

    if out.response.secondary_clicked() {
        let at = ui.input(|i| i.pointer.interact_pos()).map_or(0, |p| out.galley.cursor_from_pos(p - out.galley_pos).index.0);
        st.at = at;
        st.sel = out.cursor_range.map(|r| r.sorted_cursors()).filter(|[a, b]| a != b).map(|[a, b]| (a.index.0, b.index.0));
        let under = st.words.iter().find(|w| (w.0..=w.1).contains(&at)).cloned();
        st.hit = under.clone().filter(|w| st.is_bad(w, s));
        st.learned_hit = under.map(|w| w.2.to_lowercase()).filter(|w| s.learned_words.contains(w));
        if let (Some(h), Some(sp)) = (&st.hit, st.speller.as_mut()) {
            sp.want_suggestions(&h.2);
        }
    }
    let mut act = None;
    egui::Popup::context_menu(&out.response).close_behavior(menu_closing(&ctx)).show(|ui| {
        menu_body(ui, |ui| {
            if let Some(h) = &st.hit {
                let sugg = st.speller.as_ref().map(|sp| sp.suggestions(&h.2)).unwrap_or_default();
                for w in sugg {
                    if menu_item(ui, None, w, false) {
                        act = Some(Act::Replace(w.clone()));
                    }
                }
                if menu_item(ui, None, "Add to Dictionary", false) {
                    act = Some(Act::Learn(h.2.to_lowercase()));
                }
            } else if let Some(w) = &st.learned_hit
                && menu_item(ui, None, "Remove from Dictionary", false)
            {
                act = Some(Act::Forget(w.clone()));
            }
            if menu_row(ui, None, "Spellcheck", None, false, Trail::Check(s.spellcheck)).clicked() {
                act = Some(Act::Toggle);
            }
            if s.spellcheck && menu_item(ui, None, "Languages", false) {
                ctx.open_url(egui::OpenUrl::new_tab(HELP));
            }
            menu_sep(ui);
            if st.sel.is_some() {
                if menu_item(ui, None, "Cut", false) {
                    act = Some(Act::Cut);
                }
                if menu_item(ui, None, "Copy", false) {
                    act = Some(Act::Copy);
                }
            }
            if menu_item(ui, None, "Paste", false) {
                act = Some(Act::Paste);
            }
        })
    });

    let Some(act) = act else { return false };
    let chars = |d: &str, a: usize, b: usize| d.chars().skip(a).take(b - a).collect::<String>();
    let (from, to) = st.sel.unwrap_or((st.at, st.at));
    let put = |draft: &mut String, a: usize, b: usize, text: &str| {
        let bytes = |n: usize| draft.char_indices().nth(n).map_or(draft.len(), |c| c.0);
        draft.replace_range(bytes(a)..bytes(b), text);
        a + text.chars().count()
    };
    let mut end = None;
    let mut changed = false;
    match act {
        Act::Replace(w) => {
            if let Some(h) = &st.hit {
                end = Some(put(draft, h.0, h.1, &w));
            }
        }
        Act::Learn(w) => {
            s.learned_words.push(w);
            changed = true;
        }
        Act::Forget(w) => {
            s.learned_words.retain(|x| *x != w);
            changed = true;
        }
        Act::Toggle => {
            s.spellcheck = !s.spellcheck;
            changed = true;
        }
        Act::Copy => ctx.copy_text(chars(draft, from, to)),
        Act::Cut => {
            ctx.copy_text(chars(draft, from, to));
            end = Some(put(draft, from, to, ""));
        }
        Act::Paste => {
            if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                end = Some(put(draft, from, to, &text));
            }
        }
    }
    if let Some(c) = end
        && let Some(mut te) = egui::TextEdit::load_state(&ctx, tid)
    {
        te.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(c))));
        te.store(&ctx, tid);
    }
    ctx.memory_mut(|m| m.request_focus(tid));
    changed
}

use egui::{RichText, Ui};
use serde_json::Value;

use super::{account, modals, card, open_web, primary, profiles, remote};
use crate::net::Tag;
use crate::state::Id;
use crate::theme::*;
use crate::ui::App;

#[derive(Default)]
pub struct Forms {
    pub search: String,
    pub modal: Option<modals::Modal>,
    pub verify: Option<modals::Verify>,
    pub web: Option<modals::Web>,
    pub sudo: Option<modals::Req>,
    pub sub: account::Sub,
    pub profile: Option<profiles::Profile>,
    pub profile_base: Option<profiles::Profile>,
    pub profile_guild: Option<Id>,
    pub picker: Option<profiles::Kind>,
    pub saving: bool,
    pub recording: Option<usize>,
    pub rec_keys: Vec<u32>,
    pub reveal_email: bool,
    pub reveal_phone: bool,
    pub age_note_off: bool,
    pub media_tab: u8,
    pub dm_guild: Option<Id>,
    pub people_open: [bool; 2],
    pub act_search: String,
    pub act_sort: u8,
    pub app_search: String,
    pub app_open: Vec<String>,
    pub session: u32,
    pub visit: u32,
    pub page: Option<super::Page>,
    pub gift_code: String,
}

impl Forms {
    pub fn recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn modal_open(&self) -> bool {
        self.modal.is_some()
    }

    pub fn modal_closed(&self) -> bool {
        self.modal.is_none()
    }
}

/// The classic JSON settings object. Its fields have names, so writing one can
/// never land on the wrong setting; if discord ever retires it, writes just fail.
pub(super) fn legacy(app: &mut App, ui: &mut Ui) -> Option<Value> {
    let v = remote(app, "legacy", "/users/@me/settings").cloned();
    match v {
        Some(Ok(v)) => Some(v),
        None => {
            super::lead(ui);
            ui.add(egui::Spinner::new().color(pal().text_muted));
            None
        }
        Some(Err(_)) => {
            card(ui, |ui| {
                ui.label(RichText::new("Discord didn't hand these settings to Rustcord. You can still change them in the browser.").font(reg(14.0)).color(pal().header_secondary));
                ui.add_space(8.0);
                if primary(ui, "Open on discord.com").clicked() {
                    open_web("/settings");
                }
            });
            None
        }
    }
}

pub(super) fn set_legacy(app: &mut App, patch: Value) {
    if let Some(Some(Ok(cur))) = app.remote.get_mut("legacy")
        && let (Some(c), Some(p)) = (cur.as_object_mut(), patch.as_object())
    {
        for (k, v) in p {
            c.insert(k.clone(), v.clone());
        }
    }
    app.rest("PATCH", "/users/@me/settings".into(), Some(patch), Tag::Remote("action"));
}

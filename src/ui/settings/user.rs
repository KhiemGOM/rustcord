use super::{account, modals};
use crate::state::Id;

#[derive(Default)]
pub struct Forms {
    pub search: String,
    pub test_mode: Option<super::testmode::Form>,
    pub modal: Option<modals::Modal>,
    pub verify: Option<modals::Verify>,
    pub web: Option<modals::Web>,
    pub sudo: Option<modals::Req>,
    pub sub: account::Sub,
    pub profile_guild: Option<Id>,
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

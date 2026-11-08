use serde_json::Value;

use super::{Store, sid};

impl Store {
    pub(super) fn live(&mut self, t: &str, d: &Value) {
        match t {
            "CHANNEL_RECIPIENT_ADD" | "CHANNEL_RECIPIENT_REMOVE" => {
                let uid = self.upsert_user(&d["user"]);
                let ch = sid(&d["channel_id"]);
                let Some(c) = self.channels.get_mut(&ch).filter(|c| c.guild == 0) else { return };
                if t.ends_with("ADD") {
                    if !c.recipients.contains(&uid) {
                        c.recipients.push(uid);
                    }
                } else {
                    c.recipients.retain(|u| *u != uid);
                }
            }
            "GUILD_MEMBER_ADD" => {
                let gid = sid(&d["guild_id"]);
                let uid = self.upsert_user(&d["user"]);
                self.upsert_member(gid, uid, d);
                if let Some(g) = self.guilds.get_mut(&gid) {
                    g.member_count += 1;
                }
            }
            "GUILD_MEMBER_REMOVE" => {
                let gid = sid(&d["guild_id"]);
                let uid = sid(&d["user"]["id"]);
                if let Some(m) = self.members.get_mut(&gid) {
                    m.remove(&uid);
                }
                if let Some(g) = self.guilds.get_mut(&gid) {
                    g.member_count = g.member_count.saturating_sub(1);
                }
                for list in self.thread_members.values_mut() {
                    list.retain(|u| *u != uid);
                }
            }
            _ => {}
        }
    }
}

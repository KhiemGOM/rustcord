//! Writing the account's settings proto back: a PreloadedUserSettings holding only the one
//! field that changed, which Discord merges into what it has.

use base64::Engine;
use serde_json::json;

use crate::net::Tag;
use crate::ui::App;

pub enum Leaf {
    Raw(u64),
    Bool(bool),
    Num(u64),
    Text(String),
}

fn varint(mut v: u64, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn field(n: u64, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    varint(n << 3 | 2, &mut out);
    varint(body.len() as u64, &mut out);
    out.extend_from_slice(body);
    out
}

pub fn patch(path: &[u64], leaf: &Leaf) -> Vec<u8> {
    let Some((last, parents)) = path.split_last() else { return Vec::new() };
    let mut msg = match leaf {
        Leaf::Raw(v) => {
            let mut out = Vec::new();
            varint(last << 3, &mut out);
            varint(*v, &mut out);
            out
        }
        Leaf::Bool(v) => field(*last, if *v { &[0x08, 1] } else { &[] }),
        Leaf::Num(v) => {
            let mut inner = vec![0x08];
            varint(*v, &mut inner);
            field(*last, &inner)
        }
        Leaf::Text(s) => {
            let mut inner = vec![0x0a];
            varint(s.len() as u64, &mut inner);
            inner.extend_from_slice(s.as_bytes());
            field(*last, &inner)
        }
    };
    for p in parents.iter().rev() {
        msg = field(*p, &msg);
    }
    msg
}

pub fn put(app: &App, path: &[u64], leaf: Leaf) {
    let b64 = base64::engine::general_purpose::STANDARD.encode(patch(path, &leaf));
    app.rest("PATCH", "/users/@me/settings-proto/1".into(), Some(json!({ "settings": b64 })), Tag::Remote("action"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_read_back_through_the_account_parser() {
        let a = crate::proto::account(&patch(&[13, 2], &Leaf::Raw(1)), false);
        assert_eq!(a.dev, Some(true));
        let a = crate::proto::account(&patch(&[12, 1], &Leaf::Text("de".into())), false);
        assert_eq!(a.locale.as_deref(), Some("de"));
        let a = crate::proto::account(&patch(&[7, 12], &Leaf::Bool(false)), false);
        assert_eq!(a.friend_online, Some(false));
        let a = crate::proto::account(&patch(&[5, 6], &Leaf::Num(300)), false);
        assert_eq!(a.push_delay, Some(300));
        let a = crate::proto::account(&patch(&[7, 7], &Leaf::Raw(2)), false);
        assert_eq!(a.reaction_notify, Some(2));
        let a = crate::proto::account(&patch(&[6, 16], &Leaf::Bool(true)), false);
        assert_eq!(a.tts_command, Some(true));
    }
}

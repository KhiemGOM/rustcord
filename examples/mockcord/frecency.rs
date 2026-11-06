//! The account's frecency settings (settings-proto type 2), kept the way discord keeps them:
//! a patch replaces each top-level field it carries, and one that isn't a protobuf is refused.

use std::sync::Mutex;

static BLOB: Mutex<Vec<u8>> = Mutex::new(Vec::new());

const FIELDS: std::ops::RangeInclusive<u64> = 1..=13;

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut out = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*i)?;
        *i += 1;
        out |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(out);
        }
    }
    None
}

/// Each top-level field with its tag, so it can be kept or dropped whole.
fn split(b: &[u8]) -> Option<Vec<(u64, Vec<u8>)>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let key = varint(b, &mut i)?;
        match key & 7 {
            0 => {
                varint(b, &mut i)?;
            }
            1 => i += 8,
            5 => i += 4,
            2 => {
                let n = varint(b, &mut i)? as usize;
                i += n;
            }
            _ => return None,
        }
        if i > b.len() || !FIELDS.contains(&(key >> 3)) {
            return None;
        }
        out.push((key >> 3, b[start..i].to_vec()));
    }
    Some(out)
}

pub fn get() -> Vec<u8> {
    BLOB.lock().unwrap().clone()
}

/// The stored settings once `patch` is in, or None when it isn't a settings message.
pub fn patch(patch: &[u8]) -> Option<Vec<u8>> {
    let incoming = split(patch)?;
    let mut have = split(&BLOB.lock().unwrap())?;
    have.retain(|(n, _)| !incoming.iter().any(|(m, _)| m == n));
    have.extend(incoming);
    have.sort_by_key(|(n, _)| *n);
    let merged: Vec<u8> = have.into_iter().flat_map(|(_, b)| b).collect();
    *BLOB.lock().unwrap() = merged.clone();
    Some(merged)
}

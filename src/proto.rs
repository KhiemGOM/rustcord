//! Just enough protobuf to pull guild folders out of the READY settings blob
//! (PreloadedUserSettings, field 14 = GuildFolders).

use crate::state::{Folder, Id};

enum Val<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Fixed(u64),
}

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

fn fields(b: &[u8]) -> Vec<(u64, Val<'_>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some(key) = varint(b, &mut i) else { break };
        let v = match key & 7 {
            0 => match varint(b, &mut i) {
                Some(v) => Val::Int(v),
                None => break,
            },
            1 => match b.get(i..i + 8) {
                Some(s) => {
                    i += 8;
                    Val::Fixed(u64::from_le_bytes(s.try_into().unwrap()))
                }
                None => break,
            },
            2 => {
                let Some(len) = varint(b, &mut i) else { break };
                let Some(s) = b.get(i..i + len as usize) else { break };
                i += len as usize;
                Val::Bytes(s)
            }
            5 => {
                i += 4;
                continue;
            }
            _ => break,
        };
        out.push((key >> 3, v));
    }
    out
}

// google.protobuf.*Value wrappers keep their payload in field 1
fn wrapped(b: &[u8]) -> Option<Val<'_>> {
    fields(b).into_iter().find(|(f, _)| *f == 1).map(|(_, v)| v)
}

fn folder(b: &[u8]) -> Folder {
    let mut f = Folder::default();
    for (n, v) in fields(b) {
        match (n, v) {
            (1, Val::Bytes(packed)) => {
                f.guilds.extend(packed.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap()) as Id))
            }
            (1, Val::Fixed(g)) => f.guilds.push(g),
            (2, Val::Bytes(w)) => {
                if let Some(Val::Int(id)) = wrapped(w) {
                    f.id = id;
                }
            }
            (3, Val::Bytes(w)) => {
                if let Some(Val::Bytes(s)) = wrapped(w) {
                    f.name = Some(String::from_utf8_lossy(s).into_owned()).filter(|s| !s.is_empty());
                }
            }
            (4, Val::Bytes(w)) => {
                if let Some(Val::Int(c)) = wrapped(w) {
                    f.color = Some(c as u32);
                }
            }
            _ => {}
        }
    }
    f
}

pub fn guild_folders(settings: &[u8]) -> Vec<Folder> {
    let mut out = Vec::new();
    for (n, v) in fields(settings) {
        if let (14, Val::Bytes(gf)) = (n, v) {
            for (n2, v2) in fields(gf) {
                if let (1, Val::Bytes(fb)) = (n2, v2) {
                    out.push(folder(fb));
                }
            }
        }
    }
    out
}

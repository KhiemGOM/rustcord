//! PreloadedUserSettings on the wire: read the groups the privacy pages show and write
//! patches back. Groups are the top-level messages (6 text and images, 7 notifications, 8
//! privacy, 11 status); a patch replaces whole fields inside each group it names. No
//! dependencies, so the mock includes this file too.

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Int(u64),
    Bytes(Vec<u8>),
}

type Fields = Vec<(u64, Node)>;

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

fn put(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

pub fn decode(b: &[u8]) -> Fields {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some(key) = varint(b, &mut i) else { break };
        let node = match key & 7 {
            0 => varint(b, &mut i).map(Node::Int),
            1 => b.get(i..i + 8).map(|s| {
                i += 8;
                Node::Bytes(s.to_vec())
            }),
            2 => varint(b, &mut i).and_then(|n| {
                let s = b.get(i..i + n as usize)?;
                i += n as usize;
                Some(Node::Bytes(s.to_vec()))
            }),
            _ => None,
        };
        match node {
            Some(n) => out.push((key >> 3, n)),
            None => break,
        }
    }
    out
}

pub fn encode(f: &[(u64, Node)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (n, v) in f {
        match v {
            Node::Int(i) => {
                put(&mut out, n << 3);
                put(&mut out, *i);
            }
            Node::Bytes(b) => {
                put(&mut out, n << 3 | 2);
                put(&mut out, b.len() as u64);
                out.extend_from_slice(b);
            }
        }
    }
    out
}

fn group(blob: &[u8], g: u64) -> Fields {
    decode(blob).into_iter().filter(|(n, _)| *n == g).flat_map(|(_, v)| if let Node::Bytes(b) = v { decode(&b) } else { Vec::new() }).collect()
}

fn find(blob: &[u8], g: u64, f: u64) -> Option<Node> {
    group(blob, g).into_iter().rev().find(|(n, _)| *n == f).map(|(_, v)| v)
}

/// A BoolValue wrapper: false is an empty message.
pub fn flag(blob: &[u8], g: u64, f: u64) -> Option<bool> {
    match find(blob, g, f)? {
        Node::Bytes(b) => Some(decode(&b).iter().any(|(_, v)| matches!(v, Node::Int(i) if *i != 0))),
        Node::Int(i) => Some(i != 0),
    }
}

pub fn num(blob: &[u8], g: u64, f: u64) -> Option<u64> {
    match find(blob, g, f)? {
        Node::Int(i) => Some(i),
        Node::Bytes(_) => None,
    }
}

/// A number inside a wrapper message (UInt32Value and friends).
pub fn wrapped(blob: &[u8], g: u64, f: u64) -> Option<u64> {
    match find(blob, g, f)? {
        Node::Bytes(b) => decode(&b).into_iter().find_map(|(_, v)| if let Node::Int(i) = v { Some(i) } else { None }).or(Some(0)),
        Node::Int(i) => Some(i),
    }
}

/// An enum field of a message nested in a group (explicit_content_settings and its kin).
pub fn inner(blob: &[u8], g: u64, f: u64, n: u64) -> Option<u64> {
    let Node::Bytes(b) = find(blob, g, f)? else { return None };
    decode(&b).into_iter().rev().find(|(k, _)| *k == n).and_then(|(_, v)| if let Node::Int(i) = v { Some(i) } else { None })
}

/// A repeated fixed64, packed or not.
pub fn ids(blob: &[u8], g: u64, f: u64) -> Vec<u64> {
    let mut out = Vec::new();
    for (n, v) in group(blob, g) {
        if n != f {
            continue;
        }
        if let Node::Bytes(b) = v {
            out.extend(b.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())));
        }
    }
    out
}

#[derive(Default)]
pub struct Patch(Vec<(u64, Fields)>);

impl Patch {
    pub fn new() -> Self {
        Self::default()
    }

    fn put(mut self, g: u64, f: u64, v: Node) -> Self {
        match self.0.iter_mut().find(|(n, _)| *n == g) {
            Some((_, fs)) => fs.push((f, v)),
            None => self.0.push((g, vec![(f, v)])),
        }
        self
    }

    pub fn flag(self, g: u64, f: u64, on: bool) -> Self {
        let body = if on { encode(&[(1, Node::Int(1))]) } else { Vec::new() };
        self.put(g, f, Node::Bytes(body))
    }

    pub fn num(self, g: u64, f: u64, v: u64) -> Self {
        self.put(g, f, Node::Int(v))
    }

    pub fn ids(self, g: u64, f: u64, ids: &[u64]) -> Self {
        let body: Vec<u8> = ids.iter().flat_map(|i| i.to_le_bytes()).collect();
        self.put(g, f, Node::Bytes(body))
    }

    pub fn sub(self, g: u64, f: u64, parts: &[(u64, u64)]) -> Self {
        let body = encode(&parts.iter().map(|(n, v)| (*n, Node::Int(*v))).collect::<Vec<_>>());
        self.put(g, f, Node::Bytes(body))
    }

    pub fn bytes(&self) -> Vec<u8> {
        encode(&self.0.iter().map(|(g, fs)| (*g, Node::Bytes(encode(fs)))).collect::<Vec<_>>())
    }
}

/// What the account's settings look like once `patch` is applied: each field the patch
/// names replaces the same field in `base`; the rest stays.
pub fn merge(base: &[u8], patch: &[u8]) -> Vec<u8> {
    let mut groups: Vec<(u64, Fields)> = Vec::new();
    for (blob, replace) in [(base, false), (patch, true)] {
        for (g, v) in decode(blob) {
            let Node::Bytes(b) = v else { continue };
            let fs = decode(&b);
            let at = match groups.iter().position(|(n, _)| *n == g) {
                Some(i) => i,
                None => {
                    groups.push((g, Vec::new()));
                    groups.len() - 1
                }
            };
            let cur = &mut groups[at].1;
            if replace {
                cur.retain(|(n, _)| !fs.iter().any(|(m, _)| m == n));
            }
            cur.extend(fs);
        }
    }
    groups.sort_by_key(|(g, _)| *g);
    encode(&groups.into_iter().map(|(g, fs)| (g, Node::Bytes(encode(&fs)))).collect::<Vec<_>>())
}

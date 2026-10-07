use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::XChaCha20Poly1305;

pub const OPUS_PT: u8 = 0x78;

// both modes Discord still accepts; the gateway always offers xchacha, aes only on capable hardware
pub const MODES: [&str; 2] = ["aead_aes256_gcm_rtpsize", "aead_xchacha20_poly1305_rtpsize"];

pub enum Cipher {
    Aes(Box<Aes256Gcm>),
    Xcha(Box<XChaCha20Poly1305>),
}

impl Cipher {
    pub fn new(mode: &str, key: &[u8]) -> Option<Cipher> {
        match mode {
            "aead_aes256_gcm_rtpsize" => Aes256Gcm::new_from_slice(key).ok().map(|c| Cipher::Aes(Box::new(c))),
            "aead_xchacha20_poly1305_rtpsize" => {
                XChaCha20Poly1305::new_from_slice(key).ok().map(|c| Cipher::Xcha(Box::new(c)))
            }
            _ => None,
        }
    }

    /// header || ciphertext || tag || 4-byte nonce, the "rtpsize" layout
    pub fn seal(&self, header: &[u8], body: &[u8], nonce: u32) -> Vec<u8> {
        let mut n = [0u8; 24];
        n[..4].copy_from_slice(&nonce.to_be_bytes());
        let p = Payload { msg: body, aad: header };
        let ct = match self {
            Cipher::Aes(c) => c.encrypt(n[..12].into(), p),
            Cipher::Xcha(c) => c.encrypt(&n.into(), p),
        }
        .unwrap_or_default();
        let mut out = Vec::with_capacity(header.len() + ct.len() + 4);
        out.extend_from_slice(header);
        out.extend_from_slice(&ct);
        out.extend_from_slice(&nonce.to_be_bytes());
        out
    }

    /// Decrypts an RTP packet and returns the media payload with any header
    /// extension body already stripped.
    pub fn open(&self, pkt: &[u8]) -> Option<Vec<u8>> {
        let hl = header_len(pkt)?;
        if pkt.len() < hl + 16 + 4 {
            return None;
        }
        let mut n = [0u8; 24];
        n[..4].copy_from_slice(&pkt[pkt.len() - 4..]);
        let p = Payload { msg: &pkt[hl..pkt.len() - 4], aad: &pkt[..hl] };
        let mut plain = match self {
            Cipher::Aes(c) => c.decrypt(n[..12].into(), p),
            Cipher::Xcha(c) => c.decrypt(&n.into(), p),
        }
        .ok()?;
        // the extension preamble is in the clear but its body is encrypted with the payload
        if pkt[0] & 0x10 != 0 {
            let words = u16::from_be_bytes([pkt[hl - 2], pkt[hl - 1]]) as usize;
            if plain.len() < words * 4 {
                return None;
            }
            plain.drain(..words * 4);
        }
        Some(plain)
    }
}

/// Unencrypted header size: fixed header, CSRCs and the extension preamble.
pub fn header_len(pkt: &[u8]) -> Option<usize> {
    if pkt.len() < 12 || pkt[0] >> 6 != 2 {
        return None;
    }
    let mut n = 12 + 4 * (pkt[0] & 0x0f) as usize;
    if pkt[0] & 0x10 != 0 {
        n += 4;
    }
    (pkt.len() >= n).then_some(n)
}

pub fn header(pt: u8, seq: u16, ts: u32, ssrc: u32) -> [u8; 12] {
    let mut h = [0u8; 12];
    h[0] = 0x80;
    h[1] = pt;
    h[2..4].copy_from_slice(&seq.to_be_bytes());
    h[4..8].copy_from_slice(&ts.to_be_bytes());
    h[8..12].copy_from_slice(&ssrc.to_be_bytes());
    h
}

pub fn is_rtcp(pkt: &[u8]) -> bool {
    pkt.len() > 1 && (200..=206).contains(&pkt[1])
}

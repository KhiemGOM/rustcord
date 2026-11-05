use std::ffi::c_void;
use std::ptr::null_mut;
use windows_sys::Win32::Security::Cryptography::*;

pub struct Key {
    alg: BCRYPT_ALG_HANDLE,
    key: BCRYPT_KEY_HANDLE,
    spki: Vec<u8>,
}

fn ok(status: i32) -> Option<()> {
    (status >= 0).then_some(())
}

fn der(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    match body.len() {
        n if n < 0x80 => out.push(n as u8),
        n if n < 0x100 => out.extend([0x81, n as u8]),
        n => out.extend([0x82, (n >> 8) as u8, n as u8]),
    }
    out.extend_from_slice(body);
    out
}

fn der_int(be: &[u8]) -> Vec<u8> {
    let mut v: Vec<u8> = be.iter().copied().skip_while(|b| *b == 0).collect();
    if v.first().is_none_or(|b| b & 0x80 != 0) {
        v.insert(0, 0);
    }
    der(0x02, &v)
}

impl Key {
    pub fn generate() -> Option<Key> {
        unsafe {
            let mut alg: BCRYPT_ALG_HANDLE = std::mem::zeroed();
            ok(BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_RSA_ALGORITHM, null_mut(), 0))?;
            let mut key: BCRYPT_KEY_HANDLE = std::mem::zeroed();
            let made = ok(BCryptGenerateKeyPair(alg, &mut key, 2048, 0)).and_then(|_| ok(BCryptFinalizeKeyPair(key, 0)));
            if made.is_none() {
                BCryptCloseAlgorithmProvider(alg, 0);
                return None;
            }
            let mut n = 0u32;
            ok(BCryptExportKey(key, std::mem::zeroed(), BCRYPT_RSAPUBLIC_BLOB, null_mut(), 0, &mut n, 0))?;
            let mut blob = vec![0u8; n as usize];
            ok(BCryptExportKey(key, std::mem::zeroed(), BCRYPT_RSAPUBLIC_BLOB, blob.as_mut_ptr(), n, &mut n, 0))?;
            let word = |i: usize| u32::from_le_bytes(blob[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
            let (exp_len, mod_len) = (word(2), word(3));
            let exp = &blob[24..24 + exp_len];
            let modulus = &blob[24 + exp_len..24 + exp_len + mod_len];
            let rsa_key = der(0x30, &[der_int(modulus), der_int(exp)].concat());
            let oid = der(0x30, &[der(0x06, &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01]), vec![0x05, 0x00]].concat());
            let spki = der(0x30, &[oid, der(0x03, &[&[0u8][..], &rsa_key].concat())].concat());
            Some(Key { alg, key, spki })
        }
    }

    pub fn spki(&self) -> &[u8] {
        &self.spki
    }

    pub fn decrypt(&self, cipher: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let pad = BCRYPT_OAEP_PADDING_INFO { pszAlgId: BCRYPT_SHA256_ALGORITHM, pbLabel: null_mut(), cbLabel: 0 };
            let pad = &pad as *const _ as *const c_void;
            let mut n = 0u32;
            ok(BCryptDecrypt(self.key, cipher.as_ptr(), cipher.len() as u32, pad, null_mut(), 0, null_mut(), 0, &mut n, BCRYPT_PAD_OAEP))?;
            let mut out = vec![0u8; n as usize];
            ok(BCryptDecrypt(self.key, cipher.as_ptr(), cipher.len() as u32, pad, null_mut(), 0, out.as_mut_ptr(), n, &mut n, BCRYPT_PAD_OAEP))?;
            out.truncate(n as usize);
            Some(out)
        }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            BCryptDestroyKey(self.key);
            BCryptCloseAlgorithmProvider(self.alg, 0);
        }
    }
}

unsafe impl Send for Key {}
unsafe impl Sync for Key {}

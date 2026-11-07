//! Windows WebAuthn for the account's security keys and passkeys. Discord hands over the same
//! JSON a browser would pass to `navigator.credentials`, and wants the same JSON back.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use serde_json::{Value, json};
use windows::Win32::Networking::WindowsWebServices::*;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{BOOL, PCWSTR};

const ORIGIN: &str = "https://discord.com";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn unb64(s: &str) -> Vec<u8> {
    B64.decode(s.trim_end_matches('=')).unwrap_or_default()
}

fn client_data(kind: &str, challenge: &str) -> Vec<u8> {
    json!({ "type": kind, "challenge": challenge, "origin": ORIGIN, "crossOrigin": false }).to_string().into_bytes()
}

unsafe fn copy(p: *const u8, n: u32) -> Vec<u8> {
    if p.is_null() { Vec::new() } else { unsafe { std::slice::from_raw_parts(p, n as usize) }.to_vec() }
}

fn verification(o: &Value) -> u32 {
    match o.as_str() {
        Some("required") => WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED,
        Some("preferred") => WEBAUTHN_USER_VERIFICATION_REQUIREMENT_PREFERRED,
        Some("discouraged") => WEBAUTHN_USER_VERIFICATION_REQUIREMENT_DISCOURAGED,
        _ => WEBAUTHN_USER_VERIFICATION_REQUIREMENT_ANY,
    }
}

fn version(max: u32) -> u32 {
    unsafe { WebAuthNGetApiVersionNumber() }.min(max)
}

struct Creds {
    _ids: Vec<Vec<u8>>,
    _items: Vec<WEBAUTHN_CREDENTIAL_EX>,
    _ptrs: Vec<*mut WEBAUTHN_CREDENTIAL_EX>,
    list: WEBAUTHN_CREDENTIAL_LIST,
}

impl Creds {
    fn new(list: &Value) -> Creds {
        let mut ids: Vec<Vec<u8>> = list.as_array().into_iter().flatten().map(|c| unb64(c["id"].as_str().unwrap_or(""))).collect();
        let mut items: Vec<WEBAUTHN_CREDENTIAL_EX> = ids
            .iter_mut()
            .map(|id| WEBAUTHN_CREDENTIAL_EX { dwVersion: WEBAUTHN_CREDENTIAL_EX_CURRENT_VERSION, cbId: id.len() as u32, pbId: id.as_mut_ptr(), pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY, dwTransports: 0 })
            .collect();
        let mut ptrs: Vec<*mut WEBAUTHN_CREDENTIAL_EX> = items.iter_mut().map(|i| i as *mut _).collect();
        let list = WEBAUTHN_CREDENTIAL_LIST { cCredentials: ptrs.len() as u32, ppCredentials: ptrs.as_mut_ptr() };
        Creds { _ids: ids, _items: items, _ptrs: ptrs, list }
    }

    fn list(&mut self) -> *mut WEBAUTHN_CREDENTIAL_LIST {
        &mut self.list
    }
}

fn fail(e: windows::core::Error) -> String {
    if e.code().0 as u32 == 0x8009_0036 { "Cancelled.".into() } else { e.message() }
}

pub fn make(options: &str) -> Result<String, String> {
    let root: Value = serde_json::from_str(options).map_err(|e| e.to_string())?;
    let o = &root["publicKey"];
    let challenge = o["challenge"].as_str().unwrap_or("");
    let rp_id = wide(o["rp"]["id"].as_str().unwrap_or("discord.com"));
    let rp_name = wide(o["rp"]["name"].as_str().unwrap_or("Discord"));
    let rp = WEBAUTHN_RP_ENTITY_INFORMATION { dwVersion: 1, pwszId: PCWSTR(rp_id.as_ptr()), pwszName: PCWSTR(rp_name.as_ptr()), pwszIcon: PCWSTR::null() };
    let mut uid = unb64(o["user"]["id"].as_str().unwrap_or(""));
    let uname = wide(o["user"]["name"].as_str().unwrap_or(""));
    let udisplay = wide(o["user"]["displayName"].as_str().unwrap_or(""));
    let user = WEBAUTHN_USER_ENTITY_INFORMATION {
        dwVersion: 1,
        cbId: uid.len() as u32,
        pbId: uid.as_mut_ptr(),
        pwszName: PCWSTR(uname.as_ptr()),
        pwszIcon: PCWSTR::null(),
        pwszDisplayName: PCWSTR(udisplay.as_ptr()),
    };
    let mut algs: Vec<WEBAUTHN_COSE_CREDENTIAL_PARAMETER> = o["pubKeyCredParams"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["alg"].as_i64())
        .map(|a| WEBAUTHN_COSE_CREDENTIAL_PARAMETER { dwVersion: 1, pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY, lAlg: a as i32 })
        .collect();
    if algs.is_empty() {
        algs.push(WEBAUTHN_COSE_CREDENTIAL_PARAMETER { dwVersion: 1, pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY, lAlg: WEBAUTHN_COSE_ALGORITHM_ECDSA_P256_WITH_SHA256 });
    }
    let params = WEBAUTHN_COSE_CREDENTIAL_PARAMETERS { cCredentialParameters: algs.len() as u32, pCredentialParameters: algs.as_mut_ptr() };
    let mut data = client_data("webauthn.create", challenge);
    let client = WEBAUTHN_CLIENT_DATA { dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION, cbClientDataJSON: data.len() as u32, pbClientDataJSON: data.as_mut_ptr(), pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256 };
    let sel = &o["authenticatorSelection"];
    let mut exclude = Creds::new(&o["excludeCredentials"]);
    let opts = WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS {
        dwVersion: version(WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS_CURRENT_VERSION),
        dwTimeoutMilliseconds: o["timeout"].as_u64().unwrap_or(120_000) as u32,
        dwAuthenticatorAttachment: match sel["authenticatorAttachment"].as_str() {
            Some("platform") => WEBAUTHN_AUTHENTICATOR_ATTACHMENT_PLATFORM,
            Some("cross-platform") => WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM,
            _ => WEBAUTHN_AUTHENTICATOR_ATTACHMENT_ANY,
        },
        bRequireResidentKey: BOOL::from(sel["requireResidentKey"].as_bool().unwrap_or(false) || sel["residentKey"] == "required"),
        dwUserVerificationRequirement: verification(&sel["userVerification"]),
        dwAttestationConveyancePreference: match o["attestation"].as_str() {
            Some("direct") => WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_DIRECT,
            Some("indirect") => WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_INDIRECT,
            _ => WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_NONE,
        },
        pExcludeCredentialList: exclude.list(),
        bPreferResidentKey: BOOL::from(sel["residentKey"] == "preferred"),
        ..Default::default()
    };
    let att = unsafe { WebAuthNAuthenticatorMakeCredential(GetForegroundWindow(), &rp, &user, &params, &client, Some(&opts)) }.map_err(fail)?;
    let a = unsafe { &*att };
    let id = unsafe { copy(a.pbCredentialId, a.cbCredentialId) };
    let out = json!({
        "clientExtensionResults": {},
        "id": B64.encode(&id),
        "rawId": B64.encode(&id),
        "response": {
            "attestationObject": B64.encode(unsafe { copy(a.pbAttestationObject, a.cbAttestationObject) }),
            "authenticatorData": B64.encode(unsafe { copy(a.pbAuthenticatorData, a.cbAuthenticatorData) }),
            "clientDataJSON": B64.encode(&data),
            "transports": [],
        },
        "type": "public-key",
    });
    unsafe { WebAuthNFreeCredentialAttestation(Some(att)) };
    Ok(out.to_string())
}

pub fn get(options: &str) -> Result<String, String> {
    if let Ok(fake) = std::env::var("RUSTCORD_FAKE_PASSKEY") {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        return if fake == "cancel" { Err("Cancelled.".into()) } else { Ok(fake) };
    }
    let root: Value = serde_json::from_str(options).map_err(|e| e.to_string())?;
    let o = &root["publicKey"];
    let challenge = o["challenge"].as_str().unwrap_or("");
    let rp_id = wide(o["rpId"].as_str().unwrap_or("discord.com"));
    let mut data = client_data("webauthn.get", challenge);
    let client = WEBAUTHN_CLIENT_DATA { dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION, cbClientDataJSON: data.len() as u32, pbClientDataJSON: data.as_mut_ptr(), pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256 };
    let mut allow = Creds::new(&o["allowCredentials"]);
    let opts = WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS {
        dwVersion: version(WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS_CURRENT_VERSION),
        dwTimeoutMilliseconds: o["timeout"].as_u64().unwrap_or(120_000) as u32,
        dwUserVerificationRequirement: verification(&o["userVerification"]),
        pAllowCredentialList: allow.list(),
        ..Default::default()
    };
    let got = unsafe { WebAuthNAuthenticatorGetAssertion(GetForegroundWindow(), PCWSTR(rp_id.as_ptr()), &client, Some(&opts)) }.map_err(fail)?;
    let a = unsafe { &*got };
    let id = unsafe { copy(a.Credential.pbId, a.Credential.cbId) };
    let user = unsafe { copy(a.pbUserId, a.cbUserId) };
    let out = json!({
        "clientExtensionResults": {},
        "id": B64.encode(&id),
        "rawId": B64.encode(&id),
        "response": {
            "authenticatorData": B64.encode(unsafe { copy(a.pbAuthenticatorData, a.cbAuthenticatorData) }),
            "clientDataJSON": B64.encode(&data),
            "signature": B64.encode(unsafe { copy(a.pbSignature, a.cbSignature) }),
            "userHandle": if user.is_empty() { Value::Null } else { json!(B64.encode(&user)) },
        },
        "type": "public-key",
    });
    unsafe { WebAuthNFreeAssertion(got) };
    Ok(out.to_string())
}

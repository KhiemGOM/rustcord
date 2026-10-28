//! Speak Message: Windows' own voice reads a message out, as discord's text-to-speech does. One
//! voice on its own thread; a new message cuts off whatever it was still saying.

use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

static VOICE: OnceLock<Mutex<Sender<(String, u16, i32)>>> = OnceLock::new();

pub fn say(text: String) {
    say_with(text, 100, 0);
}

/// With SAPI's volume (0-100) and rate (-10-10).
pub fn say_with(text: String, volume: u16, rate: i32) {
    let tx = VOICE.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(String, u16, i32)>();
        let _ = std::thread::Builder::new().name("speech".into()).spawn(move || unsafe {
            use windows::Win32::Media::Speech::{ISpVoice, SPF_ASYNC, SPF_PURGEBEFORESPEAK, SpVoice};
            use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let Ok(voice) = CoCreateInstance::<_, ISpVoice>(&SpVoice, None, CLSCTX_ALL) else { return };
            for (text, volume, rate) in rx {
                let _ = voice.SetVolume(volume);
                let _ = voice.SetRate(rate);
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = voice.Speak(windows::core::PCWSTR(wide.as_ptr()), (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32, None);
            }
        });
        Mutex::new(tx)
    });
    if let Ok(tx) = tx.lock() {
        let _ = tx.send((text, volume, rate));
    }
}

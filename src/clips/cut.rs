//! Cuts a trimmed clip out of its file: a source reader decodes it and a sink writer re-encodes
//! the samples inside the chosen stretch.

use std::path::Path;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::core::{GUID, HSTRING};

use super::mux;
use super::ring::RATE;

type Result<T> = windows::core::Result<T>;

const TICKS_MS: i64 = 10_000;
const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const ALL: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
const EOS: u32 = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;

pub fn cut(src: &Path, dst: &Path, from_ms: i64, to_ms: i64) -> Result<()> {
    mux::start()?;
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(src.as_os_str()), &attrs)?;
        r.SetStreamSelection(ALL, false)?;
        r.SetStreamSelection(VIDEO, true)?;
        let native = r.GetNativeMediaType(VIDEO, 0)?;
        let size = native.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or((30 << 32) | 1);
        let raw = MFCreateMediaType()?;
        raw.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        raw.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        raw.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
        raw.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
        r.SetCurrentMediaType(VIDEO, None, &raw)?;
        let audio = r.SetStreamSelection(AUDIO, true).is_ok() && r.GetNativeMediaType(AUDIO, 0).is_ok();
        if audio {
            r.SetCurrentMediaType(AUDIO, None, &mux::audio_type(&MFAudioFormat_PCM, RATE as u32 * 2, 2)?)?;
        }

        let w = MFCreateSinkWriterFromURL(&HSTRING::from(dst.as_os_str()), None, None)?;
        let fps = ((rate >> 32) as f64 / (rate as u32).max(1) as f64).round().max(1.0) as u32;
        let bytes = std::fs::metadata(src).map_or(0, |m| m.len());
        let bps = (bytes * 8000 / to_ms.max(1) as u64).clamp(1_000_000, 20_000_000) as u32;
        let vs = w.AddStream(&mux::video_type(((size >> 32) as u32, size as u32), fps, bps, &[])?)?;
        w.SetInputMediaType(vs, &r.GetCurrentMediaType(VIDEO)?, None)?;
        let outs = [vs, if audio { mux::add_audio(&w)? } else { 0 }];
        w.BeginWriting()?;

        r.SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(from_ms * TICKS_MS))?;
        let (from, to) = (from_ms * TICKS_MS, to_ms * TICKS_MS);
        let mut at = [from, if audio { from } else { i64::MAX }];
        while at[0] != i64::MAX || at[1] != i64::MAX {
            let k = usize::from(at[1] < at[0]);
            let (mut flags, mut ts, mut s) = (0u32, 0i64, None);
            r.ReadSample([VIDEO, AUDIO][k], 0, None, Some(&mut flags), Some(&mut ts), Some(&mut s))?;
            if flags & EOS != 0 || ts >= to {
                at[k] = i64::MAX;
                continue;
            }
            at[k] = at[k].max(ts);
            if let Some(s) = s.filter(|_: &IMFSample| ts >= from) {
                s.SetSampleTime(ts - from)?;
                w.WriteSample(outs[k], &s)?;
            }
        }
        w.Finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::super::mux;
    use super::super::ring::RATE;

    fn length_ms(b: &[u8]) -> i64 {
        let i = b.windows(4).position(|w| w == b"mvhd").unwrap();
        let u32_at = |at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) as i64;
        if b[i + 4] == 1 {
            u64::from_be_bytes(b[i + 28..i + 36].try_into().unwrap()) as i64 * 1000 / u32_at(i + 24)
        } else {
            u32_at(i + 20) * 1000 / u32_at(i + 16)
        }
    }

    fn near_red(got: u8, want: i64) -> bool {
        let d = (got as i64 - want.rem_euclid(256)).rem_euclid(256);
        d.min(256 - d) <= 12
    }

    #[test]
    fn a_cut_keeps_only_the_chosen_stretch() {
        mux::start().unwrap();
        let frames = mux::synthetic(320, 240, 30, 150);
        let tone: Vec<i16> = (0..RATE as usize * 5).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 8000.0) as i16).collect();
        let dir = std::env::temp_dir().join(format!("rustcord-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dst) = (dir.join("a.mp4"), dir.join("b.mp4"));
        mux::write(&src, &frames, (320, 240), 30, &tone).unwrap();
        for (from, to, first, last) in [(0, 2000, 0, 59), (1000, 3000, 30, 89), (3000, 5000, 90, 149)] {
            super::cut(&src, &dst, from, to).unwrap();
            let bytes = std::fs::read(&dst).unwrap();
            assert!(bytes.windows(4).any(|w| w == b"mp4a"), "no audio in the cut");
            let pics = crate::mp4::frames(&bytes, 1 << 28).unwrap();
            let want = (to - from) as usize * 30 / 1000;
            assert!(pics.len().abs_diff(want) <= 1, "{from}..{to}: {} frames, wanted {want}", pics.len());
            let (head, tail) = (pics[0].0.pixels[0].r(), pics[pics.len() - 1].0.pixels[0].r());
            assert!(near_red(head, first * 4), "{from}..{to} began on red {head}");
            assert!(near_red(tail, last * 4), "{from}..{to} ended on red {tail}, wanted frame {last}");
            let len = length_ms(&bytes);
            assert!((len - (to - from)).abs() <= 100, "{from}..{to} runs {len} ms");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

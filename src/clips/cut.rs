//! Cuts a trimmed clip out of its file with Media Foundation's transcoder: the media session
//! plays the source between two times into a fresh MP4.

use std::path::Path;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::core::{GUID, HSTRING, Interface};

type Result<T> = windows::core::Result<T>;

const TICKS_MS: u64 = 10_000;
// the encoder holds back its last frames when the session ends, so the cut runs this far past the end asked for
const DRAIN_MS: u64 = 450;

/// The source's picture size and frame rate (as MF packs them), and whether it has sound.
fn streams(source: &IMFMediaSource) -> Result<(u64, u64, bool)> {
    unsafe {
        let pd = source.CreatePresentationDescriptor()?;
        let (mut size, mut rate, mut audio) = (0, 0, false);
        for i in 0..pd.GetStreamDescriptorCount()? {
            let (mut sel, mut sd) = (windows::core::BOOL(0), None);
            pd.GetStreamDescriptorByIndex(i, &mut sel, &mut sd)?;
            let Some(h) = sd.map(|s| s.GetMediaTypeHandler()).transpose()? else { continue };
            match h.GetMajorType()? {
                t if t == MFMediaType_Audio => audio = true,
                t if t == MFMediaType_Video => {
                    let cur = h.GetCurrentMediaType()?;
                    size = cur.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
                    rate = cur.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0);
                }
                _ => {}
            }
        }
        Ok((size, rate, audio))
    }
}

pub fn cut(src: &Path, dst: &Path, from_ms: i64, to_ms: i64) -> Result<()> {
    super::mux::start()?;
    unsafe {
        let resolver = MFCreateSourceResolver()?;
        let (mut kind, mut obj) = (MF_OBJECT_INVALID, None);
        resolver.CreateObjectFromURL(&HSTRING::from(src.as_os_str()), MF_RESOLUTION_MEDIASOURCE.0 as u32, None, &mut kind, &mut obj)?;
        let source: IMFMediaSource = obj.ok_or_else(windows::core::Error::empty)?.cast()?;
        let bytes = std::fs::metadata(src).map_or(0, |m| m.len());
        let whole = bytes * 8000 / (to_ms.max(1) as u64).max(1);
        let profile = MFCreateTranscodeProfile()?;
        let (size, rate, audio) = streams(&source)?;
        if audio {
            let mut a = None;
            MFCreateAttributes(&mut a, 4)?;
            let a = a.ok_or_else(windows::core::Error::empty)?;
            a.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            a.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)?;
            a.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            a.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 1)?;
            a.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)?;
            profile.SetAudioAttributes(&a)?;
        }
        let mut v = None;
        MFCreateAttributes(&mut v, 4)?;
        let v = v.ok_or_else(windows::core::Error::empty)?;
        v.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        if size != 0 {
            v.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
            v.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
        }
        v.SetUINT32(&MF_MT_AVG_BITRATE, whole.clamp(1_000_000, 20_000_000) as u32)?;
        profile.SetVideoAttributes(&v)?;
        let mut c = None;
        MFCreateAttributes(&mut c, 2)?;
        let c = c.ok_or_else(windows::core::Error::empty)?;
        c.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
        profile.SetContainerAttributes(&c)?;
        let topo = MFCreateTranscodeTopology(&source, &HSTRING::from(dst.as_os_str()), &profile)?;
        let nodes = topo.GetSourceNodeCollection()?;
        for i in 0..nodes.GetElementCount()? {
            let n: IMFTopologyNode = nodes.GetElement(i)?.cast()?;
            n.SetUINT64(&MF_TOPONODE_MEDIASTART, from_ms as u64 * TICKS_MS)?;
            n.SetUINT64(&MF_TOPONODE_MEDIASTOP, (to_ms as u64 + DRAIN_MS) * TICKS_MS)?;
        }
        let session = MFCreateMediaSession(None)?;
        session.SetTopology(0, &topo)?;
        session.Start(&GUID::zeroed(), &std::mem::zeroed::<PROPVARIANT>())?;
        let mut out = Ok(());
        loop {
            let ev = session.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))?;
            let kind = ev.GetType()?;
            let status = ev.GetStatus()?;
            if status.is_err() {
                out = Err(windows::core::Error::from(status));
                break;
            }
            if kind == MESessionEnded.0 as u32 {
                break;
            }
        }
        let _ = session.Close();
        while let Ok(ev) = session.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0)) {
            if ev.GetType().is_ok_and(|k| k == MESessionClosed.0 as u32) {
                break;
            }
        }
        let _ = source.Shutdown();
        let _ = session.Shutdown();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::mux;
    use super::super::ring::{RATE, TICKS};

    #[test]
    fn a_cut_keeps_only_the_chosen_stretch() {
        mux::start().unwrap();
        let frames = mux::synthetic(320, 240, 30, 150);
        let tone: Vec<i16> = (0..RATE as usize * 5).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 8000.0) as i16).collect();
        let dir = std::env::temp_dir().join(format!("rustcord-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dst) = (dir.join("a.mp4"), dir.join("b.mp4"));
        mux::write(&src, &frames, (320, 240), 30, &tone).unwrap();
        for (from, to, first) in [(0, 2000, 0), (1000, 3000, 30), (3000, 5000, 90)] {
            super::cut(&src, &dst, from, to).unwrap();
            let bytes = std::fs::read(&dst).unwrap();
            assert!(bytes.windows(4).any(|w| w == b"mp4a"), "no audio in the cut");
            let pics = crate::mp4::frames(&bytes, 1 << 28).unwrap();
            let want = (to - from) as usize * 30 / 1000;
            assert!(pics.len().abs_diff(want) <= 2, "{from}..{to}: {} frames, wanted {want}", pics.len());
            // the picture's red channel drifts 4 a frame, so it says which frame the cut began on
            let red = pics[0].0.pixels[0].r() as i32;
            assert!((red - (first * 4 % 256) as i32).abs() <= 12, "{from}..{to} began on red {red}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

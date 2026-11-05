//! Writes encoded frames and mixed audio to an MP4 through Media Foundation's sink writer: the
//! H.264 goes in as it came out of the encoder, the audio is encoded to AAC on the way.

use std::path::Path;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{GUID, HSTRING};

#[cfg(test)]
use openh264::OpenH264API;
#[cfg(test)]
use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod};
#[cfg(test)]
use openh264::formats::{RgbSliceU8, YUVBuffer};

use super::ring::{Frame, RATE, TICKS};

type Result<T> = windows::core::Result<T>;

const CHUNK: usize = 960;

pub fn start() -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL)
    }
}

fn sample(data: &[u8], time: i64, dur: i64) -> Result<IMFSample> {
    unsafe {
        let buf = MFCreateMemoryBuffer(data.len() as u32)?;
        let mut p = std::ptr::null_mut();
        buf.Lock(&mut p, None, None)?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
        buf.Unlock()?;
        buf.SetCurrentLength(data.len() as u32)?;
        let s = MFCreateSample()?;
        s.AddBuffer(&buf)?;
        s.SetSampleTime(time)?;
        s.SetSampleDuration(dur)?;
        Ok(s)
    }
}

fn header(b: &[u8]) -> Vec<u8> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 < b.len() {
        if b[i..i + 3] == [0, 0, 1] {
            starts.push(i);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (n, s) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(b.len());
        if matches!(b[s + 3] & 0x1f, 7 | 8) {
            let mut e = end;
            while e > *s && b[e - 1] == 0 && n + 1 < starts.len() {
                e -= 1;
            }
            out.extend([0, 0, 0, 1]);
            out.extend(&b[s + 3..e]);
        }
    }
    out
}

fn video_type(size: (u32, u32), fps: u32, bps: u32, seq: &[u8]) -> Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        t.SetUINT32(&MF_MT_AVG_BITRATE, bps)?;
        t.SetUINT64(&MF_MT_FRAME_SIZE, ((size.0 as u64) << 32) | size.1 as u64)?;
        t.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        if !seq.is_empty() {
            t.SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, seq)?;
        }
        Ok(t)
    }
}

fn audio_type(sub: &GUID, bytes_per_sec: u32, align: u32) -> Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        t.SetGUID(&MF_MT_SUBTYPE, sub)?;
        t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE as u32)?;
        t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 1)?;
        t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_sec)?;
        t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, align)?;
        Ok(t)
    }
}

pub fn write(path: &Path, frames: &[Frame], size: (u32, u32), fps: u32, pcm: &[i16]) -> Result<i64> {
    let first = frames.first().ok_or_else(windows::core::Error::empty)?;
    let seq = header(&first.data);
    unsafe {
        let w = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, None)?;
        let len = frames.last().map_or(0, |l| l.pts - first.pts) + TICKS / fps.max(1) as i64;
        let bps = (frames.iter().map(|f| f.data.len() as i64).sum::<i64>() * 8 * TICKS / len.max(1)).clamp(100_000, 100_000_000) as u32;
        let v = video_type(size, fps, bps, &seq)?;
        let vs = w.AddStream(&v)?;
        w.SetInputMediaType(vs, &v, None)?;
        let audio = if pcm.iter().any(|s| s.unsigned_abs() > 16) {
            let out = audio_type(&MFAudioFormat_AAC, 24_000, 1)?;
            out.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
            out.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
            let a = w.AddStream(&out)?;
            let out = audio_type(&MFAudioFormat_PCM, RATE as u32 * 2, 2)?;
            w.SetInputMediaType(a, &out, None)?;
            Some(a)
        } else {
            None
        };
        w.BeginWriting()?;
        let mut next = 0usize;
        let chunk_ticks = CHUNK as i64 * TICKS / RATE;
        for (i, f) in frames.iter().enumerate() {
            let time = f.pts - first.pts;
            if let Some(a) = audio {
                while next * CHUNK < pcm.len() && next as i64 * chunk_ticks <= time {
                    let end = ((next + 1) * CHUNK).min(pcm.len());
                    let bytes: Vec<u8> = pcm[next * CHUNK..end].iter().flat_map(|s| s.to_le_bytes()).collect();
                    w.WriteSample(a, &sample(&bytes, next as i64 * chunk_ticks, chunk_ticks)?)?;
                    next += 1;
                }
            }
            let dur = frames.get(i + 1).map_or(TICKS / fps.max(1) as i64, |n| n.pts - f.pts);
            let s = sample(&f.data, time, dur)?;
            if f.key {
                s.SetUINT32(&MFSampleExtension_CleanPoint, 1)?;
            }
            w.WriteSample(vs, &s)?;
        }
        if let Some(a) = audio {
            while next * CHUNK < pcm.len() && next as i64 * chunk_ticks <= len {
                let end = ((next + 1) * CHUNK).min(pcm.len());
                let bytes: Vec<u8> = pcm[next * CHUNK..end].iter().flat_map(|s| s.to_le_bytes()).collect();
                w.WriteSample(a, &sample(&bytes, next as i64 * chunk_ticks, chunk_ticks)?)?;
                next += 1;
            }
        }
        w.Finalize()?;
        Ok(len)
    }
}

#[cfg(test)]
pub fn synthetic(w: usize, h: usize, fps: u32, n: usize) -> Vec<Frame> {
    let cfg = EncoderConfig::new().bitrate(BitRate::from_bps(2_000_000)).max_frame_rate(FrameRate::from_hz(fps as f32)).intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2));
    let mut enc = Encoder::with_api_config(OpenH264API::from_source(), cfg).unwrap();
    let mut out = Vec::new();
    let mut rgb = vec![0u8; w * h * 3];
    for i in 0..n {
        for y in 0..h {
            for x in 0..w {
                let p = (y * w + x) * 3;
                rgb[p] = ((x + i * 4) % 256) as u8;
                rgb[p + 1] = (y % 256) as u8;
                rgb[p + 2] = 128;
            }
        }
        let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (w, h)));
        let bits = enc.encode(&yuv).unwrap();
        let data = bits.to_vec();
        if data.is_empty() {
            continue;
        }
        let key = matches!(bits.frame_type(), openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I);
        out.push(Frame { data: data.into(), pts: i as i64 * TICKS / fps as i64, key });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_an_mp4_the_decoder_reads_back() {
        start().unwrap();
        let frames = synthetic(320, 240, 30, 90);
        assert!(frames[0].key);
        let tone: Vec<i16> = (0..RATE as usize * 3).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 8000.0) as i16).collect();
        let path = std::env::temp_dir().join(format!("rustcord-clip-test-{}.mp4", std::process::id()));
        let len = write(&path, &frames, (320, 240), 30, &tone).unwrap();
        assert!((len - 3 * TICKS).abs() < TICKS / 10, "{len}");
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(crate::mp4::is_mp4(&bytes));
        assert!(bytes.windows(4).any(|w| w == b"mp4a"), "no audio track");
        let pics = crate::mp4::frames(&bytes, 1 << 28).unwrap();
        assert_eq!(pics.len(), frames.len());
        assert_eq!(pics[0].0.size, [320, 240]);
    }

    #[test]
    fn writes_video_only_without_audio() {
        start().unwrap();
        let frames = synthetic(160, 120, 30, 30);
        let path = std::env::temp_dir().join(format!("rustcord-clip-test-v-{}.mp4", std::process::id()));
        write(&path, &frames, (160, 120), 30, &[]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(!bytes.windows(4).any(|w| w == b"mp4a"));
        assert_eq!(crate::mp4::frames(&bytes, 1 << 28).unwrap().len(), 30);
    }

    #[test]
    fn header_finds_sps_and_pps() {
        let frames = synthetic(64, 64, 30, 2);
        let h = header(&frames[0].data);
        assert!(h.windows(5).any(|w| w[..4] == [0, 0, 0, 1] && w[4] & 0x1f == 7));
        assert!(h.windows(5).any(|w| w[..4] == [0, 0, 0, 1] && w[4] & 0x1f == 8));
    }
}

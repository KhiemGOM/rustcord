//! H.264 on the graphics card, the way discord's own client streams: the card's encoder (AMF,
//! NVENC, Quick Sync) through Media Foundation, fed NV12 textures on a device it shares, and the
//! card's video processor to scale and convert captures into them. Frames come back Annex B with
//! the time they went in, a frame or so later.

use egui::{Color32, ColorImage};
use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree};
use windows::Win32::System::Variant::VARIANT;
use windows::core::Interface;

type Result<T> = windows::core::Result<T>;

pub struct HwEnc {
    mft: IMFTransform,
    ctx: ID3D11DeviceContext,
    // async encoders (every hardware one) say when they want a frame and when one's ready
    events: Option<IMFMediaEventGenerator>,
    codec: Option<ICodecAPI>,
    want: u32,
    provides: bool,
    out_size: u32,
    // a few NV12 textures in turn, so one the encoder still reads isn't overwritten
    ring: Vec<ID3D11Texture2D>,
    slot: usize,
    w: usize,
    h: usize,
    dur: i64,
    // SPS and PPS from the output type, for encoders that leave them off keyframes
    header: Vec<u8>,
    done: Vec<(Vec<u8>, i64)>,
    // last, so the encoder is shut down after everything else here lets go of it
    _live: Live,
}

/// An encoder from its activation on, shut down when dropped. An async one released without
/// that keeps its work queue running, and the queue's next callback lands in freed memory.
struct Live {
    mft: IMFTransform,
    act: IMFActivate,
}

impl Drop for Live {
    fn drop(&mut self) {
        unsafe {
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            if let Ok(s) = self.mft.cast::<IMFShutdown>() {
                let _ = s.Shutdown();
            }
            let _ = self.act.ShutdownObject();
        }
    }
}

fn err(what: &str) -> windows::core::Error {
    windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), what)
}

/// The real graphics cards, in the order windows lists them: the one driving the main display first.
pub fn adapters() -> Vec<IDXGIAdapter1> {
    unsafe {
        let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() else { return Vec::new() };
        (0..)
            .map_while(|i| f.EnumAdapters1(i).ok())
            .filter(|a| a.GetDesc1().is_ok_and(|d| d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0))
            .collect()
    }
}

/// A device on one card (the default without one) that capture, the video processor and the
/// encoder can all share.
pub fn device(adapter: Option<&IDXGIAdapter1>) -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    unsafe {
        let a: Option<IDXGIAdapter> = adapter.and_then(|a| a.cast().ok());
        let kind = if a.is_some() { D3D_DRIVER_TYPE_UNKNOWN } else { D3D_DRIVER_TYPE_HARDWARE };
        let (mut dev, mut ctx) = (None, None);
        D3D11CreateDevice(
            a.as_ref(),
            kind,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut dev),
            None,
            Some(&mut ctx),
        )?;
        let (Some(dev), Some(ctx)) = (dev, ctx) else { return Err(err("no d3d11 device")) };
        // the encoder works the device from its own threads
        if let Ok(mt) = dev.cast::<ID3D11Multithread>() {
            let _ = mt.SetMultithreadProtected(true);
        }
        Ok((dev, ctx))
    }
}

// NVIDIA's queues work that runs after the encoder is gone, shutdown or not, unless it's
// drained first: what's inside goes out (and is thrown away) and it says when it's done
impl Drop for HwEnc {
    fn drop(&mut self) {
        let Some(ev) = self.events.clone() else { return };
        unsafe {
            if self.mft.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0).is_err() {
                return;
            }
            let until = Instant::now() + Duration::from_millis(500);
            while Instant::now() < until {
                match ev.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                    Ok(e) => match e.GetType().map(|t| MF_EVENT_TYPE(t as i32)) {
                        Ok(t) if t == METransformDrainComplete => break,
                        Ok(t) if t == METransformHaveOutput => {
                            if self.output().is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    },
                    Err(_) => std::thread::sleep(Duration::from_millis(1)),
                }
            }
        }
    }
}

impl HwEnc {
    /// An encoder for frames that start out in memory, on the first card whose encoder works.
    pub fn any(w: usize, h: usize, fps: u32, bps: u32) -> Result<HwEnc> {
        let mut last = err("no hardware h264 encoder");
        for a in adapters() {
            match device(Some(&a)).and_then(|(dev, ctx)| HwEnc::on(&dev, &ctx, &a, w, h, fps, bps)) {
                Ok(e) => return Ok(e),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// An encoder on `adapter`, which `dev` belongs to, that has shown it really encodes.
    pub fn on(dev: &ID3D11Device, ctx: &ID3D11DeviceContext, adapter: &IDXGIAdapter1, w: usize, h: usize, fps: u32, bps: u32) -> Result<HwEnc> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            let mut token = 0u32;
            let mut mgr = None;
            MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
            let mgr = mgr.ok_or_else(|| err("no dxgi device manager"))?;
            mgr.ResetDevice(dev, token)?;
            let ring = (0..6).map(|_| nv12_texture(dev, w, h)).collect::<Result<Vec<_>>>()?;
            let mut last = err("no hardware h264 encoder");
            // one that doesn't say which card it's for can still be the wrong one, which opens
            // fine and never answers, so each is tried before it's trusted
            for act in encoders(adapter) {
                let (live, events, codec, provides, out_size, header) = match open(&act, &mgr, w, h, fps, bps) {
                    Ok(o) => o,
                    Err(e) => {
                        last = e;
                        continue;
                    }
                };
                let want = if events.is_some() { 0 } else { 1 };
                let dur = 10_000_000 / fps.max(1) as i64;
                let mut e = HwEnc { mft: live.mft.clone(), ctx: ctx.clone(), events, codec, want, provides, out_size, ring: ring.clone(), slot: 0, w, h, dur, header, done: Vec::new(), _live: live };
                if e.proves() && e.restart().is_ok() {
                    return Ok(e);
                }
                last = err("hardware encoder never answered");
            }
            Err(last)
        }
    }

    // the probe's frames are still on their way out, and the first real one would only make
    // sense after them: throw them away and start the stream over
    fn restart(&mut self) -> Result<()> {
        unsafe {
            self.mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)?;
            if let Some(ev) = &self.events {
                while ev.GetEvent(MF_EVENT_FLAG_NO_WAIT).is_ok() {}
                self.want = 0;
            }
            self.done.clear();
            self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
        }
    }

    // black frames in until one comes out, for a little over half a second
    fn proves(&mut self) -> bool {
        let mut black = vec![16u8; self.w * self.h * 3 / 2];
        black[self.w * self.h..].fill(128);
        let until = Instant::now() + Duration::from_millis(600);
        let mut t = 0;
        while Instant::now() < until && self.done.is_empty() {
            match self.ready(Duration::from_millis(20)) {
                Ok(true) => {
                    self.upload(&black);
                    if self.submit(t, t == 0).is_err() {
                        return false;
                    }
                    t += self.dur;
                }
                Ok(false) => {}
                Err(_) => return false,
            }
        }
        !self.done.is_empty()
    }

    /// Which of the textures the next frame goes into.
    pub fn slot(&self) -> usize {
        self.slot % self.ring.len()
    }

    /// NV12 from memory into the next texture: luma rows, then chroma, at one pitch.
    pub fn upload(&mut self, nv12: &[u8]) {
        unsafe { self.ctx.UpdateSubresource(&self.ring[self.slot()], 0, None, nv12.as_ptr().cast(), self.w as u32, 0) };
    }

    /// Whether the encoder will take a frame, waiting up to `wait` for it to say so. Finished
    /// frames are collected on the way.
    pub fn ready(&mut self, wait: Duration) -> Result<bool> {
        let until = Instant::now() + wait;
        loop {
            self.drain()?;
            if self.want > 0 || Instant::now() >= until {
                return Ok(self.want > 0);
            }
            std::thread::sleep(Duration::from_micros(300));
        }
    }

    /// Hands the next texture to the encoder, stamped `time` (100ns units).
    pub fn submit(&mut self, time: i64, key: bool) -> Result<()> {
        unsafe {
            let buf = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, &self.ring[self.slot()], 0, false)?;
            let s = MFCreateSample()?;
            s.AddBuffer(&buf)?;
            s.SetSampleTime(time)?;
            s.SetSampleDuration(self.dur)?;
            if key && let Some(c) = &self.codec {
                let _ = c.SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &VARIANT::from(1u32));
            }
            self.mft.ProcessInput(0, &s, 0)?;
            self.slot += 1;
            if self.events.is_some() {
                self.want -= 1;
            } else {
                loop {
                    match self.output() {
                        Ok(()) => {}
                        Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => break,
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        self.drain()
    }

    /// Frames finished since last asked, each with the time it went in as.
    pub fn take(&mut self) -> Vec<(Vec<u8>, i64)> {
        std::mem::take(&mut self.done)
    }

    fn drain(&mut self) -> Result<()> {
        let Some(ev) = self.events.clone() else { return Ok(()) };
        unsafe {
            while let Ok(e) = ev.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                let t = MF_EVENT_TYPE(e.GetType()? as i32);
                if t == METransformNeedInput {
                    self.want += 1;
                } else if t == METransformHaveOutput {
                    self.output()?;
                }
            }
        }
        Ok(())
    }

    fn output(&mut self) -> Result<()> {
        unsafe {
            let sample = if self.provides {
                None
            } else {
                let s = MFCreateSample()?;
                s.AddBuffer(&MFCreateMemoryBuffer(self.out_size)?)?;
                Some(s)
            };
            let mut db = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(sample), dwStatus: 0, pEvents: ManuallyDrop::new(None) }];
            let mut status = 0u32;
            let r = self.mft.ProcessOutput(0, &mut db, &mut status);
            let sample = ManuallyDrop::take(&mut db[0].pSample);
            drop(ManuallyDrop::take(&mut db[0].pEvents));
            match r {
                Ok(()) => {}
                // the encoder settled its output format: take what it offers and carry on
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    if let Ok(t) = self.mft.GetOutputAvailableType(0, 0) {
                        self.mft.SetOutputType(0, &t, 0)?;
                        if let Some(hd) = blob(&t, &MF_MT_MPEG_SEQUENCE_HEADER) {
                            self.header = hd;
                        }
                    }
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
            let Some(sample) = sample else { return Ok(()) };
            let time = sample.GetSampleTime().unwrap_or(0);
            let buf = sample.ConvertToContiguousBuffer()?;
            let mut p = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut p, None, Some(&mut len))?;
            let bytes = std::slice::from_raw_parts(p, len as usize);
            let mut frame = Vec::with_capacity(bytes.len() + self.header.len());
            let key = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
            if key && !self.header.is_empty() && !has_sps(bytes) {
                frame.extend_from_slice(&self.header);
            }
            frame.extend_from_slice(bytes);
            buf.Unlock()?;
            if !frame.is_empty() {
                self.done.push((frame, time));
            }
        }
        Ok(())
    }
}

// the PCI path of the card an encoder runs on (PCI\VEN_1002&DEV_7550&...), which AMD's driver
// sets on its encoders
const DEVICE_PATH: windows::core::GUID = windows::core::GUID::from_u128(0x556155e0_8b27_44ac_8dbf_2547b9bd7112);

/// The hardware H.264 encoders for one card, its own first. Ones that say they're another
/// card's are left out: AMD lists every card's encoders for every card, and one tried on the
/// wrong card takes a frame, never answers, and leaves the right one hung as well.
fn encoders(adapter: &IDXGIAdapter1) -> Vec<IMFActivate> {
    let Ok(d) = (unsafe { adapter.GetDesc1() }) else { return Vec::new() };
    let luid = ((d.AdapterLuid.HighPart as u32 as u64) << 32) | d.AdapterLuid.LowPart as u64;
    let this = format!("VEN_{:04X}&DEV_{:04X}", d.VendorId, d.DeviceId);
    let list = |luid: Option<u64>| unsafe {
        let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
        let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
        let mut attrs = None;
        let _ = MFCreateAttributes(&mut attrs, 1);
        let Some(attrs) = attrs else { return Vec::new() };
        if let Some(l) = luid {
            let _ = attrs.SetUINT64(&MFT_ENUM_ADAPTER_LUID, l);
        }
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut n = 0u32;
        let flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
        if MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, flags, Some(&input), Some(&output), &attrs, &mut list, &mut n).is_err() || list.is_null() {
            return Vec::new();
        }
        let found: Vec<IMFActivate> = std::slice::from_raw_parts(list, n as usize).iter().flatten().cloned().collect();
        for i in 0..n as usize {
            drop(list.add(i).read());
        }
        CoTaskMemFree(Some(list as *const _));
        found
    };
    // drivers older than the per-card listing give nothing when asked for one card
    let mut found = list(Some(luid));
    if found.is_empty() {
        found = list(None);
    }
    let path = |a: &IMFActivate| unsafe {
        let mut p = windows::core::PWSTR::null();
        let mut len = 0u32;
        a.GetAllocatedString(&DEVICE_PATH, &mut p, &mut len).ok()?;
        let s = p.to_string().unwrap_or_default().to_uppercase();
        CoTaskMemFree(Some(p.0 as *const _));
        Some(s)
    };
    let (mut mine, mut unknown) = (Vec::new(), Vec::new());
    for a in found {
        match path(&a) {
            Some(p) if p.contains(&this) => mine.push(a),
            Some(_) => {}
            None => unknown.push(a),
        }
    }
    mine.extend(unknown);
    mine
}

fn texture(dev: &ID3D11Device, w: usize, h: usize, format: DXGI_FORMAT, usage: D3D11_USAGE, bind: u32, cpu: u32) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w as u32,
        Height: h as u32,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: usage,
        BindFlags: bind,
        CPUAccessFlags: cpu,
        MiscFlags: 0,
    };
    let mut t = None;
    unsafe { dev.CreateTexture2D(&desc, None, Some(&mut t))? };
    t.ok_or_else(|| err("no texture"))
}

fn nv12_texture(dev: &ID3D11Device, w: usize, h: usize) -> Result<ID3D11Texture2D> {
    // the video processor draws into them; older cards that can't still take uploads
    let rt = (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32;
    texture(dev, w, h, DXGI_FORMAT_NV12, D3D11_USAGE_DEFAULT, rt, 0).or_else(|_| texture(dev, w, h, DXGI_FORMAT_NV12, D3D11_USAGE_DEFAULT, 0, 0))
}

#[allow(clippy::type_complexity)]
unsafe fn open(
    act: &IMFActivate,
    mgr: &IMFDXGIDeviceManager,
    w: usize,
    h: usize,
    fps: u32,
    bps: u32,
) -> Result<(Live, Option<IMFMediaEventGenerator>, Option<ICodecAPI>, bool, u32, Vec<u8>)> {
    unsafe {
        let mft: IMFTransform = act.ActivateObject()?;
        // shut down on every way out from here, a refused one as much as one that's used
        let live = Live { mft: mft.clone(), act: act.clone() };
        let attrs = mft.GetAttributes()?;
        let events = if attrs.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0 {
            attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
            Some(mft.cast::<IMFMediaEventGenerator>()?)
        } else {
            None
        };
        let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);
        mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, mgr.as_raw() as usize)?;
        let codec = mft.cast::<ICodecAPI>().ok();
        let tune = |c: &ICodecAPI| {
            let set = |api: windows::core::GUID, v: VARIANT| {
                let _ = c.SetValue(&api, &v);
            };
            // CBR pads a still screen out to the full rate with filler (AMD's does, anyway); peak
            // constrained VBR spends what the picture needs, up to the same ceiling
            let vbr = VARIANT::from(eAVEncCommonRateControlMode_PeakConstrainedVBR.0 as u32);
            if c.SetValue(&CODECAPI_AVEncCommonRateControlMode, &vbr).is_err() {
                set(CODECAPI_AVEncCommonRateControlMode, VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32));
            }
            set(CODECAPI_AVEncCommonMeanBitRate, VARIANT::from(bps));
            set(CODECAPI_AVEncCommonMaxBitRate, VARIANT::from(bps));
            // a keyframe every couple of seconds so late joiners don't wait on a PLI round trip
            set(CODECAPI_AVEncMPVGOPSize, VARIANT::from(fps * 2));
            set(CODECAPI_AVEncMPVDefaultBPictureCount, VARIANT::from(0u32));
            set(CODECAPI_AVLowLatencyMode, VARIANT::from(true));
        };
        // some encoders size their rate control off what's set before the types, some reset it
        // when the types go on, so it's set both sides
        if let Some(c) = &codec {
            tune(c);
        }
        let size = ((w as u64) << 32) | h as u64;
        let rate = ((fps as u64) << 32) | 1;
        // encoders take their output type first
        let out = MFCreateMediaType()?;
        out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        out.SetUINT32(&MF_MT_AVG_BITRATE, bps)?;
        out.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
        out.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
        out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        // constrained baseline: no B frames, what every client decodes
        out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_ConstrainedBase.0 as u32)?;
        if mft.SetOutputType(0, &out, 0).is_err() {
            out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)?;
            mft.SetOutputType(0, &out, 0)?;
        }
        let inp = MFCreateMediaType()?;
        inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        inp.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
        inp.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
        inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        mft.SetInputType(0, &inp, 0)?;
        if let Some(c) = &codec {
            tune(c);
        }
        let info = mft.GetOutputStreamInfo(0)?;
        let provides = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32 != 0;
        let header = mft.GetOutputCurrentType(0).ok().and_then(|t| blob(&t, &MF_MT_MPEG_SEQUENCE_HEADER)).unwrap_or_default();
        mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
        mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
        Ok((live, events, codec, provides, info.cbSize.max(1 << 20), header))
    }
}

fn blob(t: &IMFMediaType, key: &windows::core::GUID) -> Option<Vec<u8>> {
    unsafe {
        let n = t.GetBlobSize(key).ok()?;
        let mut v = vec![0u8; n as usize];
        t.GetBlob(key, &mut v, None).ok()?;
        Some(v)
    }
}

// whether an Annex B frame carries its own sequence parameter set
fn has_sps(b: &[u8]) -> bool {
    b.windows(4).any(|w| w[..3] == [0, 0, 1] && w[3] & 0x1f == 7)
}

/// Captures scaled and converted into the encoder's textures by the card's video processor, so
/// a frame never comes back to the cpu. Also draws the smaller copy for our own tile.
pub struct Convert {
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    ring: Vec<ID3D11Texture2D>,
    fps: u32,
    size: (u32, u32),
    look: (u32, u32),
    // built for the capture's size, and again when the window is resized
    vp: Option<Vp>,
}

struct Vp {
    vp: ID3D11VideoProcessor,
    // the newest capture, copied out so the capture gets its buffer straight back
    frame: ID3D11Texture2D,
    input: ID3D11VideoProcessorInputView,
    size: (u32, u32),
    outs: Vec<ID3D11VideoProcessorOutputView>,
    look: ID3D11VideoProcessorOutputView,
    look_tex: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    pending: bool,
}

impl Convert {
    pub fn new(dev: &ID3D11Device, ctx: &ID3D11DeviceContext, enc: &HwEnc, fps: u32, look: (usize, usize)) -> Result<Convert> {
        Ok(Convert {
            dev: dev.clone(),
            ctx: ctx.clone(),
            vdev: dev.cast()?,
            vctx: ctx.cast()?,
            ring: enc.ring.clone(),
            fps,
            size: (enc.w as u32, enc.h as u32),
            look: (look.0 as u32, look.1 as u32),
            vp: None,
        })
    }

    /// Takes the newest capture: the top-left `w` x `h` of `src`.
    pub fn put(&mut self, src: &ID3D11Texture2D, w: u32, h: u32) -> Result<()> {
        if self.vp.as_ref().is_none_or(|v| v.size != (w, h)) {
            self.vp = Some(self.build(w, h)?);
        }
        let Some(v) = &self.vp else { return Ok(()) };
        let bx = D3D11_BOX { left: 0, top: 0, front: 0, right: w, bottom: h, back: 1 };
        unsafe { self.ctx.CopySubresourceRegion(&v.frame, 0, 0, 0, 0, src, 0, Some(&bx)) };
        Ok(())
    }

    pub fn has(&self) -> bool {
        self.vp.is_some()
    }

    /// The last capture into the encoder's texture `slot`.
    pub fn draw(&self, slot: usize) -> Result<()> {
        let Some(v) = &self.vp else { return Ok(()) };
        unsafe { blt(&self.vctx, v, &v.outs[slot], self.size, true) }
    }

    /// Our own tile's picture. It's the frame drawn the time before, read back now, so the cpu
    /// never waits on the card.
    pub fn preview(&mut self) -> Option<ColorImage> {
        let v = self.vp.as_mut()?;
        let (w, h) = self.look;
        unsafe {
            let img = if v.pending {
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                // still drawing: try again next time rather than wait
                self.ctx.Map(&v.staging, 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32, Some(&mut m)).ok()?;
                let src = std::slice::from_raw_parts(m.pData as *const u8, m.RowPitch as usize * h as usize);
                let px = src.chunks_exact(m.RowPitch as usize).flat_map(|row| row[..w as usize * 4].chunks_exact(4).map(|p| Color32::from_rgb(p[2], p[1], p[0]))).collect();
                self.ctx.Unmap(&v.staging, 0);
                Some(ColorImage::new([w as usize, h as usize], px))
            } else {
                None
            };
            if blt(&self.vctx, v, &v.look, self.look, false).is_ok() {
                self.ctx.CopyResource(&v.staging, &v.look_tex);
                v.pending = true;
            }
            img
        }
    }

    fn build(&self, w: u32, h: u32) -> Result<Vp> {
        unsafe {
            let rate = DXGI_RATIONAL { Numerator: self.fps, Denominator: 1 };
            let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: rate,
                InputWidth: w,
                InputHeight: h,
                OutputFrameRate: rate,
                OutputWidth: self.size.0,
                OutputHeight: self.size.1,
                Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
            };
            let en = self.vdev.CreateVideoProcessorEnumerator(&desc)?;
            let vp = self.vdev.CreateVideoProcessor(&en, 0)?;
            // no driver "enhancements" on a screen
            self.vctx.VideoProcessorSetStreamAutoProcessingMode(&vp, 0, false);
            self.vctx.VideoProcessorSetStreamFrameFormat(&vp, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            self.vctx.VideoProcessorSetStreamColorSpace(&vp, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
            self.vctx.VideoProcessorSetStreamSourceRect(&vp, 0, true, Some(&RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 }));
            let rt = (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32;
            let frame = texture(&self.dev, w as usize, h as usize, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT, rt, 0)?;
            let iv = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } },
            };
            let mut input = None;
            self.vdev.CreateVideoProcessorInputView(&frame, &en, &iv, Some(&mut input))?;
            let ov = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let view = |t: &ID3D11Texture2D| {
                let mut o = None;
                self.vdev.CreateVideoProcessorOutputView(t, &en, &ov, Some(&mut o))?;
                o.ok_or_else(|| err("no output view"))
            };
            let outs = self.ring.iter().map(view).collect::<Result<Vec<_>>>()?;
            let (lw, lh) = (self.look.0 as usize, self.look.1 as usize);
            let look_tex = texture(&self.dev, lw, lh, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET.0 as u32, 0)?;
            let look = view(&look_tex)?;
            let staging = texture(&self.dev, lw, lh, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32)?;
            let input = input.ok_or_else(|| err("no input view"))?;
            Ok(Vp { vp, frame, input, size: (w, h), outs, look, look_tex, staging, pending: false })
        }
    }
}

unsafe fn blt(vctx: &ID3D11VideoContext, v: &Vp, out: &ID3D11VideoProcessorOutputView, (w, h): (u32, u32), yuv: bool) -> Result<()> {
    unsafe {
        let r = RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 };
        vctx.VideoProcessorSetStreamDestRect(&v.vp, 0, true, Some(&r));
        vctx.VideoProcessorSetOutputTargetRect(&v.vp, true, Some(&r));
        // BT.601 studio range for the encoder, the same as the software path and what decoders
        // assume when the stream doesn't say; full range RGB for the preview
        let cs = if yuv { 1 << 4 } else { 0 };
        vctx.VideoProcessorSetOutputColorSpace(&v.vp, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: cs });
        let stream = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: ManuallyDrop::new(Some(v.input.clone())), ..Default::default() };
        let res = vctx.VideoProcessorBlt(&v.vp, out, 0, std::slice::from_ref(&stream));
        drop(ManuallyDrop::into_inner(stream.pInputSurface));
        res
    }
}

/// BGRA to NV12, BT.601 limited range like openh264 assumes, chroma averaged over each 2x2.
pub fn nv12(bgra: &[u8], w: usize, h: usize, out: &mut Vec<u8>) {
    out.resize(w * h * 3 / 2, 0);
    let (y, uv) = out.split_at_mut(w * h);
    for (src, dst) in bgra.chunks_exact(w * 4).zip(y.chunks_exact_mut(w)) {
        for (d, p) in dst.iter_mut().zip(src.chunks_exact(4)) {
            let (b, g, r) = (p[0] as i32, p[1] as i32, p[2] as i32);
            *d = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16) as u8;
        }
    }
    for (rows, dst) in bgra.chunks_exact(w * 8).zip(uv.chunks_exact_mut(w)) {
        let (a, b2) = rows.split_at(w * 4);
        for (x, d) in dst.chunks_exact_mut(2).enumerate() {
            let mut s = [0i32; 3];
            for p in [&a[x * 8..], &a[x * 8 + 4..], &b2[x * 8..], &b2[x * 8 + 4..]] {
                s[0] += p[0] as i32;
                s[1] += p[1] as i32;
                s[2] += p[2] as i32;
            }
            let (b, g, r) = (s[0] / 4, s[1] / 4, s[2] / 4);
            d[0] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128) as u8;
            d[1] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // none on a machine without a hardware encoder (CI's runners): those tests pass vacuously
    fn encoder() -> Option<HwEnc> {
        HwEnc::any(1280, 720, 30, 2_500_000).ok()
    }

    // black frames in, as the camera's would go, until one is refused; some are still inside
    // the encoder when this returns
    fn feed(e: &mut HwEnc, n: usize) {
        let mut black = vec![16u8; e.w * e.h * 3 / 2];
        black[e.w * e.h..].fill(128);
        let until = Instant::now() + Duration::from_secs(2);
        let (mut sent, mut t) = (0, 0);
        while sent < n && Instant::now() < until {
            match e.ready(Duration::from_millis(20)) {
                Ok(true) => {
                    e.upload(&black);
                    if e.submit(t, sent == 0).is_err() {
                        return;
                    }
                    t += e.dur;
                    sent += 1;
                }
                Ok(false) => {}
                Err(_) => return,
            }
        }
    }

    #[test]
    fn a_new_encoder_opens_right_after_one_is_dropped() {
        let Some(mut e) = encoder() else { return };
        feed(&mut e, 3);
        drop(e);
        assert!(encoder().is_some(), "the card's encoder didn't open a second time");
    }

    // an async encoder released without being shut down keeps its work queue going, which then
    // calls into what was freed: the crash on switching or turning off the camera in a call
    #[test]
    fn a_dropped_encoder_is_shut_down() {
        let Some(mut e) = encoder() else { return };
        if e.events.is_none() {
            return;
        }
        feed(&mut e, 3);
        let mft = e.mft.clone();
        drop(e);
        // NVIDIA's answers MF_E_TRANSFORM_ASYNC_LOCKED to calls after it, not MF_E_SHUTDOWN
        let status = unsafe { mft.cast::<IMFShutdown>().and_then(|s| s.GetShutdownStatus()) };
        assert_eq!(status.ok(), Some(MFSHUTDOWN_COMPLETED));
    }

    #[test]
    fn encoders_dropped_mid_stream_over_and_over() {
        let threads: Vec<_> = (0..3)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..8 {
                        let Some(mut e) = encoder() else { return };
                        feed(&mut e, 4);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
    }
}

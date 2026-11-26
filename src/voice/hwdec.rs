//! H.264 on the graphics card for what we watch: Microsoft's decoder drives the card's video engine
//! and the frames stay in its memory. Only a size that still covers the screen comes back to the cpu,
//! made by averaging (see `Mips`), because the card's video processor scales noticeably softer.

use egui::{Color32, ColorImage};
use std::mem::ManuallyDrop;
use std::time::Duration;

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::{GUID, Interface};

use super::hwenc;

type Result<T> = windows::core::Result<T>;

const DECODER: GUID = GUID::from_u128(0x62ce7e72_4c71_4d20_b15d_452831a87d9d);
const FAILED: windows::core::HRESULT = windows::core::HRESULT(0x80004005u32 as i32);

fn err(what: &str) -> windows::core::Error {
    windows::core::Error::new(FAILED, what)
}

/// The device every stream on a connection decodes on: the painter's own when available, so no
/// second device is created and decoded frames stay on the card without a CPU copy.
pub struct Gpu {
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    mgr: IMFDXGIDeviceManager,
    pub epoch: Option<u32>,
}

impl Gpu {
    pub fn new() -> Result<Gpu> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            let (dev, ctx, epoch) = match eframe::d3d11_device() {
                Some((d, c, g)) => (d, c, Some(g)),
                None => {
                    let (d, c) = hwenc::device(None)?;
                    (d, c, None)
                }
            };
            let (mut token, mut mgr) = (0u32, None);
            MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
            let mgr = mgr.ok_or_else(|| err("no dxgi device manager"))?;
            mgr.ResetDevice(&dev, token)?;
            Ok(Gpu { dev, ctx, mgr, epoch })
        }
    }

    pub fn stale(&self) -> bool {
        self.epoch.is_some_and(|g| g != eframe::device_resets())
    }
}

struct MtGuard(Option<ID3D11Multithread>);

impl MtGuard {
    fn enter(mt: &Option<ID3D11Multithread>) -> Self {
        if let Some(m) = mt {
            unsafe { m.Enter() };
        }
        Self(mt.clone())
    }
}

impl Drop for MtGuard {
    fn drop(&mut self) {
        if let Some(m) = &self.0 {
            unsafe { m.Leave() };
        }
    }
}

/// How many times a picture can be halved and still cover `want`.
pub fn halvings((w, h): (usize, usize), want: (usize, usize)) -> u32 {
    (w / want.0.max(1)).min(h / want.1.max(1)).clamp(1, 16).ilog2()
}

/// A full-size picture on the card and its halves, each the average of the one above: as sharp as
/// averaging on the cpu, for no cpu. Only the smallest comes back.
pub struct Mips {
    ctx: ID3D11DeviceContext,
    pub tex: ID3D11Texture2D,
    srv: ID3D11ShaderResourceView,
    staging: ID3D11Texture2D,
    format: DXGI_FORMAT,
    full: (u32, u32),
    steps: u32,
}

impl Mips {
    pub fn new(dev: &ID3D11Device, ctx: &ID3D11DeviceContext, full: (u32, u32), steps: u32, format: DXGI_FORMAT) -> Result<Mips> {
        unsafe {
            let desc = |w, h, levels, usage, bind, cpu, misc| D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: levels,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: usage,
                BindFlags: bind,
                CPUAccessFlags: cpu,
                MiscFlags: misc,
            };
            let misc = if steps > 0 { D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32 } else { 0 };
            let bind = (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32;
            let (mut tex, mut staging, mut srv) = (None, None, None);
            dev.CreateTexture2D(&desc(full.0, full.1, steps + 1, D3D11_USAGE_DEFAULT, bind, 0, misc), None, Some(&mut tex))?;
            let (w, h) = ((full.0 >> steps).max(1), (full.1 >> steps).max(1));
            dev.CreateTexture2D(&desc(w, h, 1, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32, 0), None, Some(&mut staging))?;
            let tex: ID3D11Texture2D = tex.ok_or_else(|| err("no texture"))?;
            dev.CreateShaderResourceView(&tex, None, Some(&mut srv))?;
            Ok(Mips { ctx: ctx.clone(), tex, srv: srv.ok_or_else(|| err("no view"))?, staging: staging.ok_or_else(|| err("no texture"))?, format, full, steps })
        }
    }

    pub fn fits(&self, full: (u32, u32), steps: u32, format: DXGI_FORMAT) -> bool {
        self.full == full && self.steps == steps && self.format == format
    }

    /// The picture in `tex`, at its smallest.
    pub fn read(&self) -> Result<Option<ColorImage>> {
        unsafe {
            if self.steps > 0 {
                self.ctx.GenerateMips(&self.srv);
            }
            self.ctx.CopySubresourceRegion(&self.staging, 0, 0, 0, 0, &self.tex, self.steps, None);
            self.ctx.Flush();
            // waits with the thread asleep, where Map's own wait spins in some drivers
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            let mut tries = 0;
            while self.ctx.Map(&self.staging, 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32, Some(&mut m)).is_err() {
                tries += 1;
                if tries > 50 {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let (w, h) = (((self.full.0 >> self.steps).max(1)) as usize, ((self.full.1 >> self.steps).max(1)) as usize);
            let src = std::slice::from_raw_parts(m.pData as *const u8, m.RowPitch as usize * h);
            let px = src.chunks_exact(m.RowPitch as usize).flat_map(|row| row[..w * 4].chunks_exact(4).map(|c| Color32::from_rgb(c[2], c[1], c[0]))).collect();
            self.ctx.Unmap(&self.staging, 0);
            Ok(Some(ColorImage::new([w, h], px)))
        }
    }
}

enum ScalerOut {
    Direct { id: egui::TextureId, _tex: ID3D11Texture2D },
    Mips(Mips),
}

impl Drop for ScalerOut {
    fn drop(&mut self) {
        if let ScalerOut::Direct { id, .. } = self {
            eframe::unregister_d3d11_texture(*id);
        }
    }
}

// NV12 to BGRA at full size, which the card's video processor does cleanly
struct Scaler {
    from: (u32, u32),
    steps: u32,
    en: ID3D11VideoProcessorEnumerator,
    vp: ID3D11VideoProcessor,
    out: ID3D11VideoProcessorOutputView,
    target: ScalerOut,
    // the decoder cycles through a few array slices, each with a view of its own
    inputs: Vec<(usize, u32, ID3D11VideoProcessorInputView)>,
}

pub struct HwDec {
    mft: IMFTransform,
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    mt: Option<ID3D11Multithread>,
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    painter_gen: Option<u32>,
    // the picture inside the decoder's surface, which is rounded up to whole macroblocks
    size: (u32, u32),
    // what the stream says its colours are, as the video processor wants it
    color: u32,
    latest: Option<IMFSample>,
    scaler: Option<Scaler>,
    time: i64,
    // frames in with nothing out yet, to notice a decoder that isn't going to deliver
    pub starved: u32,
}

impl HwDec {
    pub fn new(gpu: &Gpu) -> Result<HwDec> {
        unsafe {
            // RUSTCORD_NO_CARD_DECODE: as on a machine whose card can't, to check the software path
            if std::env::var_os("RUSTCORD_NO_CARD_DECODE").is_some() {
                return Err(err("told not to"));
            }
            let mft: IMFTransform = CoCreateInstance(&DECODER, None, CLSCTX_INPROC_SERVER)?;
            let attrs = mft.GetAttributes()?;
            if attrs.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) == 0 {
                return Err(err("decoder can't use the card"));
            }
            attrs.SetUINT32(&MF_LOW_LATENCY, 1)?;
            // keep the decoder's surface pool lean and trimmable when the stream ends
            let _ = attrs.SetUINT32(&MF_SA_MINIMUM_OUTPUT_SAMPLE_COUNT, 1);
            let _ = attrs.SetUINT32(&MF_SA_D3D11_SHARED_WITHOUT_MUTEX, 1);
            mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, gpu.mgr.as_raw() as usize)?;
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            mft.SetInputType(0, &input, 0)?;
            let mut d = HwDec {
                mft,
                dev: gpu.dev.clone(),
                ctx: gpu.ctx.clone(),
                mt: gpu.dev.cast().ok(),
                vdev: gpu.dev.cast()?,
                vctx: gpu.ctx.cast()?,
                painter_gen: gpu.epoch,
                size: (0, 0),
                color: 0,
                latest: None,
                scaler: None,
                time: 0,
                starved: 0,
            };
            d.pick_output()?;
            d.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            d.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(d)
        }
    }

    unsafe fn pick_output(&mut self) -> Result<()> {
        unsafe {
            for i in 0.. {
                let t = self.mft.GetOutputAvailableType(0, i)?;
                if t.GetGUID(&MF_MT_SUBTYPE)? != MFVideoFormat_NV12 {
                    continue;
                }
                self.mft.SetOutputType(0, &t, 0)?;
                let size = t.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
                let mut area = MFVideoArea::default();
                let got = t.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, std::slice::from_raw_parts_mut(&raw mut area as *mut u8, size_of::<MFVideoArea>()), None);
                self.size = if got.is_ok() && area.Area.cx > 0 && area.Area.cy > 0 { (area.Area.cx as u32, area.Area.cy as u32) } else { ((size >> 32) as u32, size as u32) };
                // BT.601 studio range unless the stream says otherwise: what hwenc sends, and what
                // the software decoder takes everything to be
                let bt709 = t.GetUINT32(&MF_MT_YUV_MATRIX).is_ok_and(|m| m == MFVideoTransferMatrix_BT709.0 as u32);
                let full = t.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE).is_ok_and(|r| r == MFNominalRange_0_255.0 as u32);
                self.color = (bt709 as u32) << 2 | if full { 2 << 4 } else { 1 << 4 };
                self.scaler = None;
                return Ok(());
            }
            Err(err("decoder offers no nv12"))
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.size.0 as usize, self.size.1 as usize)
    }

    /// Feeds one frame. True if a new picture came out, which `picture` can then show.
    pub fn decode(&mut self, frame: &[u8]) -> Result<bool> {
        let len = u32::try_from(frame.len()).map_err(|_| err("frame too large"))?;
        if len == 0 {
            return Ok(false);
        }
        unsafe {
            let buf = MFCreateMemoryBuffer(len)?;
            let mut p = std::ptr::null_mut();
            buf.Lock(&mut p, None, None)?;
            if p.is_null() {
                let _ = buf.Unlock();
                return Err(err("null buffer"));
            }
            std::ptr::copy_nonoverlapping(frame.as_ptr(), p, frame.len());
            buf.Unlock()?;
            buf.SetCurrentLength(len)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buf)?;
            self.time += 166_666;
            sample.SetSampleTime(self.time)?;
            sample.SetSampleDuration(166_666)?;
            let mut out = self.drain()?;
            if let Err(e) = self.mft.ProcessInput(0, &sample, 0) {
                if e.code() != MF_E_NOTACCEPTING {
                    return Err(e);
                }
                out |= self.drain()?;
                self.mft.ProcessInput(0, &sample, 0)?;
            }
            out |= self.drain()?;
            self.starved = if out { 0 } else { self.starved + 1 };
            Ok(out)
        }
    }

    unsafe fn drain(&mut self) -> Result<bool> {
        unsafe {
            let mut any = false;
            loop {
                let mut db = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(None), dwStatus: 0, pEvents: ManuallyDrop::new(None) }];
                let mut status = 0u32;
                let r = self.mft.ProcessOutput(0, &mut db, &mut status);
                let sample = ManuallyDrop::take(&mut db[0].pSample);
                drop(ManuallyDrop::take(&mut db[0].pEvents));
                match r {
                    Ok(()) => {
                        if sample.is_some() {
                            self.latest = sample;
                            any = true;
                        }
                    }
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(any),
                    Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => self.pick_output()?,
                    Err(e) => return Err(e),
                }
            }
        }
    }

    /// The newest frame, if there's been one since the last time, big enough to cover `want`.
    /// When sharing the painter's device, stays on the GPU as an external texture (zero CPU copy).
    pub fn picture(&mut self, want: (usize, usize)) -> Result<Option<super::video::Frame>> {
        unsafe {
            let steps = if self.painter_gen.is_some() { 0 } else { halvings(self.size(), want) };
            if self.scaler.as_ref().is_none_or(|s| s.steps != steps || s.from != self.size) {
                self.scaler = Some(self.scaler_for(steps)?);
            }
            let Some(sample) = self.latest.take() else { return Ok(None) };
            let dxgi = sample.GetBufferByIndex(0)?.cast::<IMFDXGIBuffer>()?;
            let mut tex: Option<ID3D11Texture2D> = None;
            dxgi.GetResource(&ID3D11Texture2D::IID, &raw mut tex as *mut *mut _)?;
            let tex = tex.ok_or_else(|| err("no decoded surface"))?;
            let slice = dxgi.GetSubresourceIndex()?;
            let input = self.input(&tex, slice)?;
            let Some(s) = &self.scaler else { return Ok(None) };
            {
                let _lock = MtGuard::enter(&self.mt);
                let stream = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: ManuallyDrop::new(Some(input)), ..Default::default() };
                let res = self.vctx.VideoProcessorBlt(&s.vp, &s.out, 0, std::slice::from_ref(&stream));
                drop(ManuallyDrop::into_inner(stream.pInputSurface));
                res?;
            }
            match &s.target {
                ScalerOut::Direct { id, .. } => Ok(Some(super::video::Frame::gpu(*id, [s.from.0 as usize, s.from.1 as usize], egui::vec2(s.from.0 as f32, s.from.1 as f32)))),
                ScalerOut::Mips(mips) => Ok(mips.read()?.map(|mut i| {
                    i.source_size = egui::vec2(s.from.0 as f32, s.from.1 as f32);
                    super::video::Frame::Cpu(std::sync::Arc::new(i))
                })),
            }
        }
    }

    unsafe fn input(&mut self, tex: &ID3D11Texture2D, slice: u32) -> Result<ID3D11VideoProcessorInputView> {
        unsafe {
            let Some(s) = &mut self.scaler else { return Err(err("no scaler")) };
            let key = tex.as_raw() as usize;
            if let Some(v) = s.inputs.iter().find(|v| v.0 == key && v.1 == slice) {
                return Ok(v.2.clone());
            }
            let desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: slice } },
            };
            let mut view = None;
            self.vdev.CreateVideoProcessorInputView(tex, &s.en, &desc, Some(&mut view))?;
            let view = view.ok_or_else(|| err("no input view"))?;
            s.inputs.push((key, slice, view.clone()));
            Ok(view)
        }
    }

    unsafe fn scaler_for(&self, steps: u32) -> Result<Scaler> {
        unsafe {
            let from = self.size;
            let rate = DXGI_RATIONAL { Numerator: 60, Denominator: 1 };
            let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: rate,
                InputWidth: from.0,
                InputHeight: from.1,
                OutputFrameRate: rate,
                OutputWidth: from.0,
                OutputHeight: from.1,
                Usage: D3D11_VIDEO_USAGE_OPTIMAL_QUALITY,
            };
            let en = self.vdev.CreateVideoProcessorEnumerator(&desc)?;
            let vp = self.vdev.CreateVideoProcessor(&en, 0)?;
            let (vctx, whole) = (&self.vctx, rect(from));
            vctx.VideoProcessorSetStreamAutoProcessingMode(&vp, 0, false);
            vctx.VideoProcessorSetStreamFrameFormat(&vp, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            vctx.VideoProcessorSetStreamColorSpace(&vp, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: self.color });
            vctx.VideoProcessorSetOutputColorSpace(&vp, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
            vctx.VideoProcessorSetStreamSourceRect(&vp, 0, true, Some(&whole));
            vctx.VideoProcessorSetStreamDestRect(&vp, 0, true, Some(&whole));
            vctx.VideoProcessorSetOutputTargetRect(&vp, true, Some(&whole));
            let (out_tex, target) = if let Some(epoch) = self.painter_gen {
                let mut td = D3D11_TEXTURE2D_DESC {
                    Width: from.0,
                    Height: from.1,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                    CPUAccessFlags: 0,
                    MiscFlags: D3D11_RESOURCE_MISC_SHARED.0 as u32,
                };
                let mut t = None;
                if self.dev.CreateTexture2D(&td, None, Some(&mut t)).is_err() {
                    td.MiscFlags = 0;
                    self.dev.CreateTexture2D(&td, None, Some(&mut t))?;
                }
                let t = t.ok_or_else(|| err("no texture"))?;
                let mut srv = None;
                self.dev.CreateShaderResourceView(&t, None, Some(&mut srv))?;
                let id = eframe::register_d3d11_texture(srv.ok_or_else(|| err("no view"))?, epoch);
                (t.clone(), ScalerOut::Direct { id, _tex: t })
            } else {
                let mips = Mips::new(&self.dev, &self.ctx, from, steps, DXGI_FORMAT_B8G8R8A8_UNORM)?;
                (mips.tex.clone(), ScalerOut::Mips(mips))
            };
            let ov = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let mut out = None;
            self.vdev.CreateVideoProcessorOutputView(&out_tex, &en, &ov, Some(&mut out))?;
            let out = out.ok_or_else(|| err("no output view"))?;
            Ok(Scaler { from, steps, en, vp, out, target, inputs: Vec::new() })
        }
    }
}

fn rect((w, h): (u32, u32)) -> RECT {
    RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 }
}

impl Drop for HwDec {
    fn drop(&mut self) {
        unsafe {
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_comes_back_always_covers_the_screen() {
        for source in [(1920, 1080), (1280, 720), (3840, 2160), (1080, 1920), (320, 180)] {
            for want in [(1, 1), (240, 135), (889, 500), (960, 540), (961, 541), (1920, 1080), (5000, 5000)] {
                let steps = halvings(source, want);
                let got = (source.0 >> steps, source.1 >> steps);
                assert!(got.0 >= want.0.min(source.0) && got.1 >= want.1.min(source.1), "{source:?} for {want:?} came back {got:?}");
                assert!(got.0 > 0 && got.1 > 0);
            }
        }
        assert_eq!(halvings((1920, 1080), (889, 500)), 1);
        assert_eq!(halvings((1920, 1080), (1920, 1080)), 0);
    }
}

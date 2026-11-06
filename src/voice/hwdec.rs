//! H.264 on the graphics card for what we watch: Microsoft's decoder drives the card's video engine,
//! the frames stay in its memory, and its video processor makes BGRA at the size they're on screen.
//! Only that small picture comes back to the cpu, read a frame late so the cpu never waits on the card.

use egui::{Color32, ColorImage};
use std::mem::ManuallyDrop;

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
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

/// The device every stream on a connection decodes on.
pub struct Gpu {
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    mgr: IMFDXGIDeviceManager,
}

impl Gpu {
    pub fn new() -> Result<Gpu> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            let (dev, ctx) = hwenc::device(None)?;
            let (mut token, mut mgr) = (0u32, None);
            MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
            let mgr = mgr.ok_or_else(|| err("no dxgi device manager"))?;
            mgr.ResetDevice(&dev, token)?;
            Ok(Gpu { dev, ctx, mgr })
        }
    }
}

struct Scaler {
    from: (u32, u32),
    to: (u32, u32),
    en: ID3D11VideoProcessorEnumerator,
    vp: ID3D11VideoProcessor,
    out: ID3D11VideoProcessorOutputView,
    target: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    // the decoder cycles through a few array slices, each with a view of its own
    inputs: Vec<(usize, u32, ID3D11VideoProcessorInputView)>,
    pending: bool,
}

pub struct HwDec {
    mft: IMFTransform,
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    // the picture inside the decoder's surface, which is rounded up to whole macroblocks
    size: (u32, u32),
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
            mft.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, gpu.mgr.as_raw() as usize)?;
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            mft.SetInputType(0, &input, 0)?;
            let mut d = HwDec {
                mft,
                dev: gpu.dev.clone(),
                ctx: gpu.ctx.clone(),
                vdev: gpu.dev.cast()?,
                vctx: gpu.ctx.cast()?,
                size: (0, 0),
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
                self.size = if got.is_ok() && area.Area.cx > 0 { (area.Area.cx as u32, area.Area.cy as u32) } else { ((size >> 32) as u32, size as u32) };
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
        unsafe {
            let buf = MFCreateMemoryBuffer(frame.len() as u32)?;
            let mut p = std::ptr::null_mut();
            buf.Lock(&mut p, None, None)?;
            std::ptr::copy_nonoverlapping(frame.as_ptr(), p, frame.len());
            buf.Unlock()?;
            buf.SetCurrentLength(frame.len() as u32)?;
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

    /// The last frame finished, and the newest one started on its way, at `size`.
    pub fn picture(&mut self, size: (usize, usize)) -> Result<Option<ColorImage>> {
        unsafe {
            let to = (size.0 as u32, size.1 as u32);
            if self.scaler.as_ref().is_none_or(|s| s.to != to || s.from != self.size) {
                self.scaler = Some(self.scaler_for(to)?);
            }
            let Some(s) = &self.scaler else { return Ok(None) };
            let mut img = None;
            if s.pending {
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                // still drawing: leave it be rather than start another on top
                if self.ctx.Map(&s.staging, 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32, Some(&mut m)).is_err() {
                    return Ok(None);
                }
                let src = std::slice::from_raw_parts(m.pData as *const u8, m.RowPitch as usize * size.1);
                let px = src.chunks_exact(m.RowPitch as usize).flat_map(|row| row[..size.0 * 4].chunks_exact(4).map(|c| Color32::from_rgb(c[2], c[1], c[0]))).collect();
                self.ctx.Unmap(&s.staging, 0);
                img = Some(ColorImage::new([size.0, size.1], px));
            }
            let Some(sample) = self.latest.take() else { return Ok(img) };
            let dxgi = sample.GetBufferByIndex(0)?.cast::<IMFDXGIBuffer>()?;
            let mut tex: Option<ID3D11Texture2D> = None;
            dxgi.GetResource(&ID3D11Texture2D::IID, &raw mut tex as *mut *mut _)?;
            let tex = tex.ok_or_else(|| err("no decoded surface"))?;
            let slice = dxgi.GetSubresourceIndex()?;
            let input = self.input(&tex, slice)?;
            let Some(s) = &mut self.scaler else { return Ok(img) };
            let stream = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: ManuallyDrop::new(Some(input)), ..Default::default() };
            let res = self.vctx.VideoProcessorBlt(&s.vp, &s.out, 0, std::slice::from_ref(&stream));
            drop(ManuallyDrop::into_inner(stream.pInputSurface));
            res?;
            self.ctx.CopyResource(&s.staging, &s.target);
            s.pending = true;
            Ok(img)
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

    unsafe fn scaler_for(&self, to: (u32, u32)) -> Result<Scaler> {
        unsafe {
            let from = self.size;
            let rate = DXGI_RATIONAL { Numerator: 60, Denominator: 1 };
            let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: rate,
                InputWidth: from.0,
                InputHeight: from.1,
                OutputFrameRate: rate,
                OutputWidth: to.0,
                OutputHeight: to.1,
                Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
            };
            let en = self.vdev.CreateVideoProcessorEnumerator(&desc)?;
            let vp = self.vdev.CreateVideoProcessor(&en, 0)?;
            let (vctx, source, dest) = (&self.vctx, rect(from), rect(to));
            vctx.VideoProcessorSetStreamAutoProcessingMode(&vp, 0, false);
            vctx.VideoProcessorSetStreamFrameFormat(&vp, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            // BT.601 studio range in, as hwenc sends and as the software decoder assumes; full range RGB out
            vctx.VideoProcessorSetStreamColorSpace(&vp, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 1 << 4 });
            vctx.VideoProcessorSetOutputColorSpace(&vp, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
            vctx.VideoProcessorSetStreamSourceRect(&vp, 0, true, Some(&source));
            vctx.VideoProcessorSetStreamDestRect(&vp, 0, true, Some(&dest));
            vctx.VideoProcessorSetOutputTargetRect(&vp, true, Some(&dest));
            let target = self.bgra(to, D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET.0 as u32, 0)?;
            let staging = self.bgra(to, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32)?;
            let ov = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let mut out = None;
            self.vdev.CreateVideoProcessorOutputView(&target, &en, &ov, Some(&mut out))?;
            let out = out.ok_or_else(|| err("no output view"))?;
            Ok(Scaler { from, to, en, vp, out, target, staging, inputs: Vec::new(), pending: false })
        }
    }

    unsafe fn bgra(&self, (w, h): (u32, u32), usage: D3D11_USAGE, bind: u32, cpu: u32) -> Result<ID3D11Texture2D> {
        unsafe {
            let d = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: usage,
                BindFlags: bind,
                CPUAccessFlags: cpu,
                MiscFlags: 0,
            };
            let mut t = None;
            self.dev.CreateTexture2D(&d, None, Some(&mut t))?;
            t.ok_or_else(|| err("no texture"))
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

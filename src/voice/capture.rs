//! Frame sources for video: the desktop via DXGI duplication, a camera via Media Foundation,
//! and synthetic patterns for the e2e run. Everything comes out as tightly packed BGRA.

use std::time::Instant;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree};
use windows::core::Interface;

use super::video::Source;

pub enum Grab {
    Fake { screen: bool, t0: Instant },
    Screen(Box<Screen>),
    Camera(Box<Camera>),
}

impl Grab {
    pub fn fake(screen: bool) -> Grab {
        Grab::Fake { screen, t0: Instant::now() }
    }

    pub fn open(src: Source) -> Result<Grab, String> {
        let r = unsafe {
            match src {
                Source::Screen => Screen::open().map(|s| Grab::Screen(Box::new(s))),
                Source::Camera => Camera::open().map(|c| Grab::Camera(Box::new(c))),
            }
        };
        r.map_err(|e| e.message().to_string())
    }

    pub fn size(&self) -> (usize, usize) {
        match self {
            Grab::Fake { screen: true, .. } => (1280, 720),
            Grab::Fake { .. } => (640, 480),
            Grab::Screen(s) => (s.w, s.h),
            Grab::Camera(c) => (c.w, c.h),
        }
    }

    /// Fills `out` (w*h BGRA) with the latest frame, scaled. False if nothing new came in.
    pub fn frame(&mut self, out: &mut [u8], w: usize, h: usize) -> bool {
        match self {
            Grab::Fake { screen, t0 } => {
                pattern(out, w, h, t0.elapsed().as_secs_f32(), *screen);
                true
            }
            Grab::Screen(s) => unsafe { s.frame(out, w, h) },
            Grab::Camera(c) => unsafe { c.frame(out, w, h) },
        }
    }
}

// Moving colour bars for the camera, scrolling "text" lines for the screen, so motion and
// both kinds of content get through the encoder.
fn pattern(out: &mut [u8], w: usize, h: usize, t: f32, screen: bool) {
    let shift = (t * 60.0) as usize;
    for y in 0..h {
        for x in 0..w {
            let px = &mut out[(y * w + x) * 4..][..4];
            let c = if screen {
                let line = (y + shift) / 18;
                let on = (y + shift) % 18 < 10 && (x / 9 + line * 7) % 11 != 0 && x % 640 < 120 + (line * 53) % 480;
                if on { [40, 40, 40] } else { [0xf2, 0xf3, 0xf5] }
            } else {
                const BARS: [[u8; 3]; 6] = [[0x58, 0x65, 0xf2], [0x3b, 0xa5, 0x5c], [0x1a, 0xa6, 0xfa], [0x45, 0x42, 0xed], [0x9e, 0x45, 0xeb], [0x9c, 0xbc, 0x1a]];
                BARS[((x + shift) * 6 / w.max(1)) % 6]
            };
            px.copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

// nearest-neighbour scale of a pitched BGRA image into a packed one
fn scale(src: &[u8], sw: usize, sh: usize, pitch: usize, flip: bool, out: &mut [u8], w: usize, h: usize) {
    for y in 0..h {
        let sy = y * sh / h;
        let sy = if flip { sh - 1 - sy } else { sy };
        let row = &src[sy * pitch..];
        for x in 0..w {
            let sx = x * sw / w;
            let o = (y * w + x) * 4;
            out[o..o + 3].copy_from_slice(&row[sx * 4..sx * 4 + 3]);
            out[o + 3] = 255;
        }
    }
}

pub struct Screen {
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    dup: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    w: usize,
    h: usize,
    have: bool,
}

impl Screen {
    unsafe fn open() -> windows::core::Result<Screen> {
        unsafe {
            let (mut dev, mut ctx) = (None, None);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut dev),
                None,
                Some(&mut ctx),
            )?;
            let (Some(dev), Some(ctx)) = (dev, ctx) else { return Err(windows::core::Error::empty()) };
            let dup = Self::duplicate(&dev)?;
            let desc = dup.GetDesc();
            Ok(Screen { dev, ctx, dup, staging: None, w: desc.ModeDesc.Width as usize, h: desc.ModeDesc.Height as usize, have: false })
        }
    }

    unsafe fn duplicate(dev: &ID3D11Device) -> windows::core::Result<IDXGIOutputDuplication> {
        unsafe {
            let output: IDXGIOutput1 = dev.cast::<IDXGIDevice>()?.GetAdapter()?.EnumOutputs(0)?.cast()?;
            output.DuplicateOutput(dev)
        }
    }

    unsafe fn frame(&mut self, out: &mut [u8], w: usize, h: usize) -> bool {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res: Option<IDXGIResource> = None;
            match self.dup.AcquireNextFrame(0, &mut info, &mut res) {
                Ok(()) => {}
                // nothing changed on screen: resend what we have so the stream doesn't stall
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return self.have,
                Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                    if let Ok(d) = Self::duplicate(&self.dev) {
                        self.dup = d;
                    }
                    return self.have;
                }
                Err(_) => return self.have,
            }
            let ok = self.copy(res, out, w, h);
            let _ = self.dup.ReleaseFrame();
            self.have |= ok;
            self.have
        }
    }

    unsafe fn copy(&mut self, res: Option<IDXGIResource>, out: &mut [u8], w: usize, h: usize) -> bool {
        unsafe {
            let Some(tex) = res.and_then(|r| r.cast::<ID3D11Texture2D>().ok()) else { return false };
            if self.staging.is_none() {
                let mut d = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut d);
                d.Usage = D3D11_USAGE_STAGING;
                d.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                d.BindFlags = 0;
                d.MiscFlags = 0;
                d.MipLevels = 1;
                d.ArraySize = 1;
                d.SampleDesc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
                let mut t = None;
                if self.dev.CreateTexture2D(&d, None, Some(&mut t)).is_err() {
                    return false;
                }
                self.staging = t;
            }
            let Some(st) = &self.staging else { return false };
            self.ctx.CopyResource(st, &tex);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            if self.ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut m)).is_err() {
                return false;
            }
            let pitch = m.RowPitch as usize;
            let src = std::slice::from_raw_parts(m.pData as *const u8, pitch * self.h);
            scale(src, self.w, self.h, pitch, false, out, w, h);
            self.ctx.Unmap(st, 0);
            true
        }
    }
}

pub struct Camera {
    reader: IMFSourceReader,
    w: usize,
    h: usize,
    stride: i32,
}

impl Camera {
    unsafe fn open() -> windows::core::Result<Camera> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 1)?;
            let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
            attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID)?;
            let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut n = 0u32;
            MFEnumDeviceSources(&attrs, &mut list, &mut n)?;
            let devs = std::slice::from_raw_parts(list, n as usize).to_vec();
            CoTaskMemFree(Some(list as *const _));
            let Some(Some(first)) = devs.into_iter().next() else {
                return Err(windows::core::Error::new(windows::core::HRESULT(0x80070490u32 as i32), "no camera found"));
            };
            let source: IMFMediaSource = first.ActivateObject()?;
            let mut ra = None;
            MFCreateAttributes(&mut ra, 1)?;
            let ra = ra.ok_or_else(windows::core::Error::empty)?;
            // lets the reader convert whatever the camera speaks (usually MJPG or NV12) to RGB32
            ra.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
            let reader = MFCreateSourceReaderFromMediaSource(&source, &ra)?;
            let mt = MFCreateMediaType()?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            reader.SetCurrentMediaType(stream, None, &mt)?;
            let cur = reader.GetCurrentMediaType(stream)?;
            let size = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let (w, h) = ((size >> 32) as usize, (size & 0xffff_ffff) as usize);
            let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or(w as i32 * 4);
            Ok(Camera { reader, w, h, stride })
        }
    }

    unsafe fn frame(&mut self, out: &mut [u8], w: usize, h: usize) -> bool {
        unsafe {
            let mut flags = 0u32;
            let mut sample = None;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            if self.reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample)).is_err() {
                return false;
            }
            let Some(buf) = sample.and_then(|s| s.ConvertToContiguousBuffer().ok()) else { return false };
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            if buf.Lock(&mut p, None, Some(&mut len)).is_err() {
                return false;
            }
            let pitch = self.stride.unsigned_abs() as usize;
            if len as usize >= pitch * self.h {
                let src = std::slice::from_raw_parts(p, pitch * self.h);
                // a negative default stride means the rows come bottom-up
                scale(src, self.w, self.h, pitch, self.stride < 0, out, w, h);
            }
            let _ = buf.Unlock();
            len as usize >= pitch * self.h
        }
    }
}

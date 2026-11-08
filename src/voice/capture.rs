//! Frame sources for video: a window or monitor through Windows.Graphics.Capture (the desktop
//! via DXGI duplication if that fails), a camera via Media Foundation, and synthetic patterns
//! for the e2e run. Everything comes out as tightly packed BGRA.

use std::time::Instant;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree};
use windows::core::Interface;

use super::effects;
use super::video::Source;

pub enum Grab {
    Fake { screen: bool, t0: Instant },
    Screen(Box<Screen>),
    Wgc(Box<Wgc>),
    Camera(Box<Camera>),
}

impl Grab {
    pub fn fake(screen: bool) -> Grab {
        if !screen {
            effects::note_open(effects::device().filter(|d| effects::FAKE_CAMERAS.contains(&d.as_str())).unwrap_or_else(|| effects::FAKE_CAMERAS[0].into()));
        }
        Grab::Fake { screen, t0: Instant::now() }
    }

    pub fn open(src: Source) -> Result<Grab, String> {
        let r = unsafe {
            match src {
                Source::Window(h) => device().and_then(|(d, c)| Wgc::open(&d, &c, h, false)).map(|s| Grab::Wgc(Box::new(s))),
                Source::Monitor(m) => device()
                    .and_then(|(d, c)| Wgc::open(&d, &c, m, true))
                    .map(|s| Grab::Wgc(Box::new(s)))
                    .or_else(|_| Screen::open().map(|s| Grab::Screen(Box::new(s)))),
                Source::Camera => {
                    super::preview::settle();
                    Camera::open(None, true).map(|c| Grab::Camera(Box::new(c)))
                }
                Source::Device(i) => {
                    super::preview::settle();
                    Camera::open(Some(i), false).map(|c| Grab::Camera(Box::new(c)))
                }
            }
        };
        r.map_err(|e| e.message().to_string())
    }

    /// The camera's ReadSample blocks until the next frame, so it sets its own pace.
    pub fn paced(&self) -> bool {
        matches!(self, Grab::Camera(_))
    }

    pub fn size(&self) -> (usize, usize) {
        match self {
            Grab::Fake { screen: true, .. } => (1280, 720),
            Grab::Fake { .. } => (640, 480),
            Grab::Screen(s) => (s.w, s.h),
            Grab::Wgc(s) => (s.w, s.h),
            Grab::Camera(c) => (c.w, c.h),
        }
    }

    /// Fills `out` (w*h BGRA) with the latest frame, scaled. False if nothing new came in.
    pub fn frame(&mut self, out: &mut [u8], w: usize, h: usize) -> bool {
        match self {
            Grab::Fake { screen, t0 } => {
                pattern(out, w, h, t0.elapsed().as_secs_f32(), *screen);
                if !*screen {
                    effects::apply(out, w, h);
                }
                true
            }
            Grab::Screen(s) => unsafe { s.frame(out, w, h) },
            Grab::Wgc(s) => unsafe { s.frame(out, w, h) },
            Grab::Camera(c) => {
                let got = unsafe { c.frame(out, w, h) };
                if got && c.fx {
                    effects::apply(out, w, h);
                }
                got
            }
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
            let (dev, ctx) = device()?;
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

fn device() -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let (mut dev, mut ctx) = (None, None);
    unsafe {
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut dev), None, Some(&mut ctx))?;
    }
    match (dev, ctx) {
        (Some(d), Some(c)) => Ok((d, c)),
        _ => Err(windows::core::Error::empty()),
    }
}

/// One window or monitor through Windows.Graphics.Capture, which sees GPU-drawn windows that GDI
/// can't and keeps going when they're covered.
pub struct Wgc {
    dev: ID3D11Device,
    ctx: ID3D11DeviceContext,
    d3d: windows::Graphics::DirectX::Direct3D11::IDirect3DDevice,
    pool: windows::Graphics::Capture::Direct3D11CaptureFramePool,
    _session: windows::Graphics::Capture::GraphicsCaptureSession,
    staging: Option<(ID3D11Texture2D, u32, u32)>,
    size: windows::Graphics::SizeInt32,
    pub w: usize,
    pub h: usize,
    have: bool,
}

impl Wgc {
    pub unsafe fn open(dev: &ID3D11Device, ctx: &ID3D11DeviceContext, target: isize, monitor: bool) -> windows::core::Result<Wgc> {
        use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
        use windows::Graphics::DirectX::DirectXPixelFormat;
        use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let insp = windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice(&dev.cast::<IDXGIDevice>()?)?;
            let d3d: windows::Graphics::DirectX::Direct3D11::IDirect3DDevice = insp.cast()?;
            let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
            let item: GraphicsCaptureItem = if monitor {
                interop.CreateForMonitor(windows::Win32::Graphics::Gdi::HMONITOR(target as _))?
            } else {
                interop.CreateForWindow(windows::Win32::Foundation::HWND(target as _))?
            };
            let size = item.Size()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)?;
            let session = pool.CreateCaptureSession(&item)?;
            // windows 11 lets us leave off the yellow frame; older ones say no and draw it
            let _ = session.SetIsBorderRequired(false);
            session.StartCapture()?;
            Ok(Wgc { dev: dev.clone(), ctx: ctx.clone(), d3d, pool, _session: session, staging: None, size, w: size.Width.max(2) as usize, h: size.Height.max(2) as usize, have: false })
        }
    }

    /// The newest frame if one came in since last asked, its texture, and how much of that is
    /// picture. The texture is only the capture's until the frame is dropped.
    pub unsafe fn latest(&mut self) -> Option<(windows::Graphics::Capture::Direct3D11CaptureFrame, ID3D11Texture2D, (u32, u32))> {
        use windows::Graphics::DirectX::DirectXPixelFormat;
        use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
        unsafe {
            // only the newest frame matters
            let mut last = None;
            while let Ok(f) = self.pool.TryGetNextFrame() {
                last = Some(f);
            }
            let f = last?;
            let cs = f.ContentSize().ok()?;
            if cs != self.size {
                self.size = cs;
                let _ = self.pool.Recreate(&self.d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, cs);
            }
            let tex = f.Surface().ok()?.cast::<IDirect3DDxgiInterfaceAccess>().ok()?.GetInterface::<ID3D11Texture2D>().ok()?;
            let mut d = D3D11_TEXTURE2D_DESC::default();
            tex.GetDesc(&mut d);
            // the window can be smaller than the pool's buffer until it's recreated
            let size = ((cs.Width.max(1) as u32).min(d.Width), (cs.Height.max(1) as u32).min(d.Height));
            Some((f, tex, size))
        }
    }

    unsafe fn frame(&mut self, out: &mut [u8], w: usize, h: usize) -> bool {
        unsafe {
            // a still window sends nothing, so the last frame goes again
            let Some((_f, tex, (cw, ch))) = self.latest() else { return self.have };
            let mut d = D3D11_TEXTURE2D_DESC::default();
            tex.GetDesc(&mut d);
            if self.staging.as_ref().is_none_or(|s| (s.1, s.2) != (d.Width, d.Height)) {
                let mut sd = d;
                sd.Usage = D3D11_USAGE_STAGING;
                sd.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                sd.BindFlags = 0;
                sd.MiscFlags = 0;
                sd.MipLevels = 1;
                sd.ArraySize = 1;
                sd.SampleDesc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
                let mut t = None;
                if self.dev.CreateTexture2D(&sd, None, Some(&mut t)).is_err() {
                    return self.have;
                }
                self.staging = t.map(|t| (t, d.Width, d.Height));
            }
            let Some((st, ..)) = &self.staging else { return self.have };
            self.ctx.CopyResource(st, &tex);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            if self.ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut m)).is_err() {
                return self.have;
            }
            let (cw, ch) = (cw as usize, ch as usize);
            let pitch = m.RowPitch as usize;
            let src = std::slice::from_raw_parts(m.pData as *const u8, pitch * ch);
            scale(src, cw, ch, pitch, false, out, w, h);
            self.ctx.Unmap(st, 0);
            self.have = true;
            true
        }
    }
}

/// The capture devices' names, in the order `Camera::open` counts them.
pub fn cameras() -> Vec<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(devs) = devices() else { return Vec::new() };
        devs.iter().flatten().map(|d| name(d)).collect()
    }
}

unsafe fn name(d: &IMFActivate) -> String {
    unsafe {
        let mut p = windows::core::PWSTR::null();
        let mut n = 0u32;
        match d.GetAllocatedString(&MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &mut p, &mut n) {
            Ok(()) => {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                s
            }
            Err(_) => "Camera".to_string(),
        }
    }
}

unsafe fn devices() -> windows::core::Result<Vec<Option<IMFActivate>>> {
    unsafe {
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
        Ok(devs)
    }
}

pub struct Camera {
    reader: IMFSourceReader,
    fx: bool,
    w: usize,
    h: usize,
    stride: i32,
}

impl Camera {
    /// The device at `index`, or with the name chosen in settings, or the first. `fx` is for
    /// the call camera, which gets the video background.
    unsafe fn open(index: Option<usize>, fx: bool) -> windows::core::Result<Camera> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let devs = devices()?;
            let index = index.unwrap_or_else(|| effects::device().and_then(|want| devs.iter().flatten().position(|d| name(d) == want)).unwrap_or(0));
            let Some(Some(first)) = devs.into_iter().nth(index) else {
                return Err(windows::core::Error::new(windows::core::HRESULT(0x80070490u32 as i32), "no camera found"));
            };
            effects::note_open(name(&first));
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
            Ok(Camera { reader, fx, w, h, stride })
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

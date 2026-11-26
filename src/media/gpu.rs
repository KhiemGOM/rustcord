use egui::{Color32, ColorImage};
use std::sync::Mutex;
use std::sync::atomic::Ordering::Relaxed;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_RESOURCE_MISC_SHARED,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIResource;
use windows::Win32::Media::MediaFoundation::{
    IMFDXGIBuffer, IMFDXGIDeviceManager, IMFMediaType, IMFSample, IMFSourceReader, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_ROTATION, MF_MT_YUV_MATRIX, MFCreateDXGIDeviceManager,
    MFCreateMediaType, MFMediaType_Video, MFNominalRange_0_255, MFVideoFormat_NV12, MFVideoFormat_RGB32, MFVideoTransferMatrix_BT709,
};
use windows::core::Interface;

use super::{SCALES, UPLOADS, VIDEO};
use crate::voice::hwdec::{Mips, halvings};

pub(super) struct Pic {
    pub w: usize,
    pub h: usize,
    pub stride: i32,
    pub nv12: bool,
    pub bt709: bool,
    pub full_range: bool,
}

pub(super) unsafe fn video_type(r: &IMFSourceReader) -> windows::core::Result<Pic> {
    unsafe {
        let cur = r.GetCurrentMediaType(VIDEO)?;
        let size = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (w, h) = ((size >> 32) as usize, (size & 0xffff_ffff) as usize);
        let nv12 = cur.GetGUID(&MF_MT_SUBTYPE).is_ok_and(|g| g == MFVideoFormat_NV12);
        let bt709 = cur.GetUINT32(&MF_MT_YUV_MATRIX).map_or(h >= 720, |m| m == MFVideoTransferMatrix_BT709.0 as u32);
        let full_range = cur.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE).is_ok_and(|v| v == MFNominalRange_0_255.0 as u32);
        let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or(w as i32 * if nv12 { 1 } else { 4 });
        Ok(Pic { w, h, stride, nv12, bt709, full_range })
    }
}

pub(super) fn fit((w, h): (usize, usize), want: (usize, usize)) -> (usize, usize) {
    let k = SCALES.into_iter().find(|k| w as f32 * k >= want.0 as f32 && h as f32 * k >= want.1 as f32).unwrap_or(1.0);
    (((w as f32 * k) as usize & !1).max(2), ((h as f32 * k) as usize & !1).max(2))
}

pub(super) unsafe fn display(t: &IMFMediaType, (w, h): (usize, usize)) -> (usize, usize) {
    unsafe {
        let (pn, pd) = t.GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO).map_or((1, 1), |p| ((p >> 32) as usize, (p & 0xffff_ffff) as usize));
        let (w, h) = if pn == 0 || pd == 0 { (w, h) } else if pn >= pd { (w * pn / pd, h) } else { (w, h * pd / pn) };
        match t.GetUINT32(&MF_MT_VIDEO_ROTATION) {
            Ok(90 | 270) => (h, w),
            _ => (w, h),
        }
    }
}

pub(super) unsafe fn output(r: &IMFSourceReader, (w, h): (usize, usize), native: u64, prefer_nv12: bool) -> windows::core::Result<()> {
    unsafe {
        let try_fmt = |sub: &windows::core::GUID| -> windows::core::Result<()> {
            let mt = MFCreateMediaType()?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            mt.SetGUID(&MF_MT_SUBTYPE, sub)?;
            mt.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, 1 << 32 | 1)?;
            let size = (w as u64) << 32 | h as u64;
            if size != native {
                mt.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
            }
            if r.SetCurrentMediaType(VIDEO, None, &mt).is_err()
                && size != native
                && (native >> 32) <= 4096
                && (native & 0xffff_ffff) <= 4096
            {
                mt.SetUINT64(&MF_MT_FRAME_SIZE, native)?;
                r.SetCurrentMediaType(VIDEO, None, &mt)?;
            }
            Ok(())
        };
        if prefer_nv12 && try_fmt(&MFVideoFormat_NV12).is_ok() {
            return Ok(());
        }
        try_fmt(&MFVideoFormat_RGB32)
    }
}

pub(super) unsafe fn to_image(sample: &IMFSample, pic: &Pic) -> Option<ColorImage> {
    unsafe {
        let buf = sample.ConvertToContiguousBuffer().ok()?;
        let (mut p, mut len) = (std::ptr::null_mut::<u8>(), 0u32);
        buf.Lock(&mut p, None, Some(&mut len)).ok()?;
        let pitch = pic.stride.unsigned_abs() as usize;
        let row_bytes = pic.w.checked_mul(4);
        let total_bytes = pitch.checked_mul(pic.h);
        let px_count = pic.w.checked_mul(pic.h);
        let img = (!p.is_null()
            && pic.w > 0
            && pic.h > 0
            && row_bytes.is_some_and(|rb| pitch >= rb)
            && total_bytes.is_some_and(|tb| len as usize >= tb)
            && px_count.is_some_and(|n| n <= 4096 * 4096))
        .then(|| {
            let src = std::slice::from_raw_parts(p, len as usize);
            let mut px = Vec::with_capacity(pic.w * pic.h);
            for y in 0..pic.h {
                let row = if pic.stride < 0 { pic.h - 1 - y } else { y } * pitch;
                px.extend(src[row..row + pic.w * 4].chunks_exact(4).map(|c| Color32::from_rgb(c[2], c[1], c[0])));
            }
            ColorImage::new([pic.w, pic.h], px)
        });
        let _ = buf.Unlock();
        img
    }
}

pub(super) type Card = (ID3D11Device, ID3D11DeviceContext);

pub(super) struct GpuTex {
    pub id: egui::TextureId,
    pub handle: isize,
    pub epoch: u32,
    pub nv12: bool,
    pub bt709: bool,
    pub full_range: bool,
}

impl GpuTex {
    pub(super) fn reopen(&mut self) {
        if self.epoch == eframe::device_resets() {
            return;
        }
        if self.nv12 {
            if let Some((y, uv, epoch)) = unsafe { eframe::open_shared_d3d11_nv12(self.handle) } {
                eframe::update_d3d11_nv12(self.id, y, uv, self.bt709, self.full_range, epoch);
                self.epoch = epoch;
            }
        } else if let Some((srv, epoch)) = unsafe { eframe::open_shared_d3d11_texture(self.handle) } {
            eframe::update_d3d11_texture(self.id, srv, epoch);
            self.epoch = epoch;
        }
    }
}

impl Drop for GpuTex {
    fn drop(&mut self) {
        eframe::unregister_d3d11_texture(self.id);
    }
}

pub(super) struct SharedOut {
    tex: ID3D11Texture2D,
    size: (u32, u32),
    format: DXGI_FORMAT,
    bt709: bool,
    full_range: bool,
}

pub(super) unsafe fn manager() -> windows::core::Result<(IMFDXGIDeviceManager, Card)> {
    unsafe {
        if std::env::var_os("RUSTCORD_NO_CARD_DECODE").is_some() {
            return Err(windows::core::Error::empty());
        }
        let (mut dev, mut ctx) = (None, None);
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut dev), None, Some(&mut ctx))?;
        let (dev, ctx) = (dev.ok_or_else(windows::core::Error::empty)?, ctx.ok_or_else(windows::core::Error::empty)?);
        let _ = dev.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
        let (mut token, mut mgr) = (0u32, None);
        MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
        let mgr = mgr.ok_or_else(windows::core::Error::empty)?;
        mgr.ResetDevice(&dev, token)?;
        Ok((mgr, (dev, ctx)))
    }
}

pub(super) unsafe fn present(sample: &IMFSample, out: &mut Option<SharedOut>, gpu: &Mutex<Option<GpuTex>>, (dev, ctx): &Card, pic: &Pic) -> bool {
    unsafe {
        let Ok(dxgi) = sample.GetBufferByIndex(0).and_then(|b| b.cast::<IMFDXGIBuffer>()) else { return false };
        let mut tex: Option<ID3D11Texture2D> = None;
        if dxgi.GetResource(&ID3D11Texture2D::IID, &raw mut tex as *mut *mut _).is_err() {
            return false;
        }
        let Some(tex) = tex else { return false };
        let Ok(slice) = dxgi.GetSubresourceIndex() else { return false };
        let mut d = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut d);
        let is_nv12 = d.Format == DXGI_FORMAT_NV12;
        let size = if is_nv12 {
            (((pic.w as u32) & !1).min(d.Width & !1), ((pic.h as u32) & !1).min(d.Height & !1))
        } else {
            (pic.w as u32, pic.h as u32)
        };
        if size.0 < 2 || size.1 < 2 {
            return false;
        }
        if out.as_ref().is_none_or(|o| o.size != size || o.format != d.Format || o.bt709 != pic.bt709 || o.full_range != pic.full_range) {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: size.0,
                Height: size.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: d.Format,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_SHARED.0 as u32,
            };
            let mut st = None;
            if dev.CreateTexture2D(&desc, None, Some(&mut st)).is_err() {
                return false;
            }
            let Some(st) = st else { return false };
            let Ok(h) = st.cast::<IDXGIResource>().and_then(|r| r.GetSharedHandle()) else { return false };
            let handle = h.0 as isize;
            let mut g = gpu.lock().unwrap();
            if is_nv12 {
                let Some((y, uv, epoch)) = eframe::open_shared_d3d11_nv12(handle) else { return false };
                match g.as_mut() {
                    Some(gt) => {
                        eframe::update_d3d11_nv12(gt.id, y, uv, pic.bt709, pic.full_range, epoch);
                        gt.handle = handle;
                        gt.epoch = epoch;
                        gt.nv12 = true;
                        gt.bt709 = pic.bt709;
                        gt.full_range = pic.full_range;
                    }
                    None => {
                        let id = eframe::register_d3d11_nv12(y, uv, pic.bt709, pic.full_range, epoch);
                        *g = Some(GpuTex { id, handle, epoch, nv12: true, bt709: pic.bt709, full_range: pic.full_range });
                    }
                }
            } else {
                let Some((srv, epoch)) = eframe::open_shared_d3d11_texture(handle) else { return false };
                match g.as_mut() {
                    Some(gt) => {
                        eframe::update_d3d11_texture(gt.id, srv, epoch);
                        gt.handle = handle;
                        gt.epoch = epoch;
                        gt.nv12 = false;
                    }
                    None => {
                        let id = eframe::register_d3d11_texture(srv, epoch);
                        *g = Some(GpuTex { id, handle, epoch, nv12: false, bt709: false, full_range: false });
                    }
                }
            }
            *out = Some(SharedOut { tex: st, size, format: d.Format, bt709: pic.bt709, full_range: pic.full_range });
        }
        let Some(o) = out.as_ref() else { return false };
        let whole = D3D11_BOX { left: 0, top: 0, front: 0, right: size.0, bottom: size.1, back: 1 };
        ctx.CopySubresourceRegion(&o.tex, 0, 0, 0, 0, &tex, slice, Some(&whole));
        ctx.Flush();
        UPLOADS.fetch_add(1, Relaxed);
        true
    }
}

pub(super) unsafe fn image(sample: &IMFSample, mips: &mut Option<Mips>, (dev, ctx): &Card, pic: &Pic, want: (usize, usize)) -> Option<ColorImage> {
    unsafe {
        let dxgi = sample.GetBufferByIndex(0).ok()?.cast::<IMFDXGIBuffer>().ok()?;
        let mut tex: Option<ID3D11Texture2D> = None;
        dxgi.GetResource(&ID3D11Texture2D::IID, &raw mut tex as *mut *mut _).ok()?;
        let tex = tex?;
        let slice = dxgi.GetSubresourceIndex().ok()?;
        let mut d = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut d);
        let full = (pic.w as u32, pic.h as u32);
        let steps = halvings((pic.w, pic.h), want);
        if !mips.as_ref().is_some_and(|m| m.fits(full, steps, d.Format)) {
            *mips = Some(Mips::new(dev, ctx, full, steps, d.Format).ok()?);
        }
        let m = mips.as_ref()?;
        let whole = D3D11_BOX { left: 0, top: 0, front: 0, right: full.0, bottom: full.1, back: 1 };
        ctx.CopySubresourceRegion(&m.tex, 0, 0, 0, 0, &tex, slice, Some(&whole));
        m.read().ok()?
    }
}

//! Windows' own colour emoji (Segoe UI Emoji), drawn through DirectWrite into images. Discord
//! leaves emoji in names to the system font, so channel, server and member names look like this
//! there; only message text goes through twemoji.

use egui::load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use egui::mutex::Mutex;
use egui::ColorImage;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::task::Poll;

const SCHEME: &str = "segoe-emoji://";
// drawn once at this size and scaled down; a 16pt name at 200% wants 32
const PX: u32 = 64;

pub fn uri(seq: &str) -> String {
    let hex: Vec<String> = seq.chars().map(|c| format!("{:x}", c as u32)).collect();
    format!("{SCHEME}{}", hex.join("-"))
}

type Cache = Arc<Mutex<HashMap<String, Poll<Option<Arc<ColorImage>>>>>>;

#[derive(Default)]
pub struct Loader {
    cache: Cache,
    // drawing happens on one thread that keeps its Direct2D and DirectWrite objects: made per
    // emoji on the ui thread, each one cost a fresh D3D device, and a server list full of
    // emoji names froze startup for seconds
    jobs: std::sync::OnceLock<Sender<(String, String, egui::Context)>>,
}

impl ImageLoader for Loader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(Loader)
    }

    fn load(&self, ctx: &egui::Context, uri: &str, _: SizeHint) -> ImageLoadResult {
        let Some(code) = uri.strip_prefix(SCHEME) else { return Err(LoadError::NotSupported) };
        let mut cache = self.cache.lock();
        match cache.get(uri) {
            Some(Poll::Ready(Some(i))) => return Ok(ImagePoll::Ready { image: i.clone() }),
            Some(Poll::Ready(None)) => return Err(LoadError::Loading("segoe ui emoji couldn't draw it".into())),
            Some(Poll::Pending) => return Ok(ImagePoll::Pending { size: Some(egui::vec2(PX as f32, PX as f32)) }),
            None => {}
        }
        cache.insert(uri.to_string(), Poll::Pending);
        drop(cache);
        let text: String = code.split('-').filter_map(|h| u32::from_str_radix(h, 16).ok()).filter_map(char::from_u32).collect();
        let jobs = self.jobs.get_or_init(|| {
            let (tx, rx) = channel::<(String, String, egui::Context)>();
            let cache = self.cache.clone();
            let _ = std::thread::Builder::new().name("emoji".into()).spawn(move || {
                let mut pen = Pen::new();
                for (uri, text, ctx) in rx {
                    let img = pen.as_mut().and_then(|p| p.draw(&text)).map(Arc::new);
                    cache.lock().insert(uri, Poll::Ready(img));
                    ctx.request_repaint();
                }
            });
            tx
        });
        let _ = jobs.send((uri.to_string(), text, ctx.clone()));
        Ok(ImagePoll::Pending { size: Some(egui::vec2(PX as f32, PX as f32)) })
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache.lock().len() * (PX * PX * 4) as usize
    }
}

/// A 64px bitmap with Segoe UI Emoji set up to draw into it, in software: no D3D device.
struct Pen {
    bmp: windows::Win32::Graphics::Imaging::IWICBitmap,
    rt: windows::Win32::Graphics::Direct2D::ID2D1RenderTarget,
    fmt: windows::Win32::Graphics::DirectWrite::IDWriteTextFormat,
    brush: windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
}

impl Pen {
    fn new() -> Option<Pen> {
        use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
        use windows::Win32::Graphics::Direct2D::{
            D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            D2D1_RENDER_TARGET_USAGE_NONE, D2D1CreateFactory, ID2D1Factory,
        };
        use windows::Win32::Graphics::DirectWrite::{
            DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            DWRITE_TEXT_ALIGNMENT_CENTER, DWriteCreateFactory, IDWriteFactory,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnLoad};
        use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
        use windows::core::w;
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let bmp = wic.CreateBitmap(PX, PX, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad).ok()?;
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = d2d.CreateWicBitmapRenderTarget(&bmp, &props).ok()?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let size = PX as f32 * 0.75;
            let fmt = dw
                .CreateTextFormat(w!("Segoe UI Emoji"), None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))
                .ok()?;
            fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER).ok()?;
            fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER).ok()?;
            let brush = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }, None).ok()?;
            Some(Pen { bmp, rt: rt.into(), fmt, brush })
        }
    }

    fn draw(&mut self, text: &str) -> Option<ColorImage> {
        use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
        use windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT;
        use windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL;
        unsafe {
            let utf16: Vec<u16> = text.encode_utf16().collect();
            let rect = D2D_RECT_F { left: 0.0, top: 0.0, right: PX as f32, bottom: PX as f32 };
            self.rt.BeginDraw();
            self.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            self.rt.DrawText(&utf16, &self.fmt, &rect, &self.brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, DWRITE_MEASURING_MODE_NATURAL);
            self.rt.EndDraw(None, None).ok()?;
            let mut buf = vec![0u8; (PX * PX * 4) as usize];
            self.bmp.CopyPixels(std::ptr::null(), PX * 4, &mut buf).ok()?;
            for px in buf.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            Some(ColorImage::from_rgba_premultiplied([PX as usize, PX as usize], &buf))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn draws_emoji_in_colour_without_a_gpu() {
        let mut pen = super::Pen::new().expect("pen");
        let img = pen.draw("😀").expect("drawn");
        let coloured = img.pixels.iter().filter(|c| c.a() > 200 && (c.r() as i32 - c.b() as i32).abs() > 60).count();
        assert!(coloured > 200, "only {coloured} coloured pixels");
        assert!(pen.draw("🎲").is_some(), "the pen is reusable");
    }
}

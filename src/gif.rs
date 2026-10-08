//! Animated GIFs, decoded off the UI thread. egui_extras' loader decodes every
//! frame inline the first time an image is drawn, which froze the window for
//! ~0.4s when the GIF picker filled up. Registered after egui_extras' loaders,
//! so it's asked first.

use egui::load::{BytesPoll, ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use egui::mutex::Mutex;
use egui::{ColorImage, FrameDurations, Id, decode_animated_image_uri, has_gif_magic_header};
use image::AnimationDecoder as _;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

// long gifs get cut short rather than eating hundreds of MB of frames
const MAX_FRAMES: usize = 240;

struct Anim {
    frames: Vec<Arc<ColorImage>>,
}

type Entry = Poll<Result<Arc<Anim>, String>>;

#[derive(Default)]
pub struct GifLoader {
    cache: Arc<Mutex<HashMap<String, Entry>>>,
}

fn decode(data: &[u8]) -> Result<(Anim, Vec<Duration>), String> {
    let dec = image::codecs::gif::GifDecoder::new(Cursor::new(data)).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let mut durations = Vec::new();
    for f in dec.into_frames().take(MAX_FRAMES) {
        let f = f.map_err(|e| e.to_string())?;
        let delay: Duration = f.delay().into();
        let buf = f.buffer();
        frames.push(Arc::new(ColorImage::from_rgba_unmultiplied([buf.width() as usize, buf.height() as usize], buf.as_raw())));
        // browsers stretch 0-10ms frame delays to 100ms and gifs are made to look right there
        durations.push(if delay < Duration::from_millis(11) { Duration::from_millis(100) } else { delay });
    }
    if frames.is_empty() {
        return Err("gif has no frames".into());
    }
    Ok((Anim { frames }, durations))
}

impl ImageLoader for GifLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(GifLoader)
    }

    fn load(&self, ctx: &egui::Context, frame_uri: &str, _: SizeHint) -> ImageLoadResult {
        let (uri, index) = decode_animated_image_uri(frame_uri).map_err(|_| LoadError::NotSupported)?;
        if let Some(e) = self.cache.lock().get(uri) {
            return match e {
                Poll::Ready(Ok(a)) => Ok(ImagePoll::Ready { image: a.frames[index % a.frames.len()].clone() }),
                Poll::Ready(Err(e)) => Err(LoadError::Loading(e.clone())),
                Poll::Pending => Ok(ImagePoll::Pending { size: None }),
            };
        }
        match ctx.try_load_bytes(uri) {
            Ok(BytesPoll::Ready { bytes, .. }) => {
                if !has_gif_magic_header(&bytes) {
                    return Err(LoadError::NotSupported);
                }
                self.cache.lock().insert(uri.to_string(), Poll::Pending);
                let (cache, ctx, uri) = (self.cache.clone(), ctx.clone(), uri.to_string());
                let bytes = bytes.to_vec();
                std::thread::spawn(move || {
                    let entry = match decode(&bytes) {
                        Ok((anim, durations)) => {
                            ctx.data_mut(|d| *d.get_temp_mut_or_default(Id::new(uri.as_str())) = FrameDurations::new(durations));
                            Ok(Arc::new(anim))
                        }
                        Err(e) => Err(e),
                    };
                    if let Some(slot) = cache.lock().get_mut(&uri) {
                        *slot = Poll::Ready(entry);
                    }
                    ctx.request_repaint();
                });
                Ok(ImagePoll::Pending { size: None })
            }
            Ok(BytesPoll::Pending { size }) => Ok(ImagePoll::Pending { size }),
            Err(e) => Err(e),
        }
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .values()
            .map(|e| match e {
                Poll::Ready(Ok(a)) => a.frames.iter().map(|f| f.pixels.len() * 4).sum(),
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache.lock().values().any(|e| e.is_pending())
    }
}

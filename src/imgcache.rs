use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};
use egui::mutex::Mutex;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::task::Poll;
use tokio::runtime::Runtime;

type Entry = Poll<Result<(Arc<[u8]>, String), String>>;

pub struct DiskLoader {
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    dir: PathBuf,
    rt: Arc<Runtime>,
    http: reqwest::Client,
    limit: Arc<tokio::sync::Semaphore>,
}

impl DiskLoader {
    pub fn new(dir: PathBuf, rt: Arc<Runtime>) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        DiskLoader {
            cache: Default::default(),
            dir,
            rt,
            http: crate::api::http(),
            limit: Arc::new(tokio::sync::Semaphore::new(12)),
        }
    }
}

fn sniff(b: &[u8]) -> &'static str {
    match b {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xff, 0xd8, ..] => "image/jpeg",
        [b'G', b'I', b'F', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}

fn key(uri: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    uri.hash(&mut h);
    format!("{:016x}", h.finish())
}

impl BytesLoader for DiskLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(DiskLoader)
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return Err(LoadError::NotSupported);
        }
        let mut cache = self.cache.lock();
        if let Some(e) = cache.get(uri) {
            return match e {
                Poll::Ready(Ok((b, mime))) => Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(b.clone()),
                    mime: Some(mime.clone()),
                }),
                Poll::Ready(Err(e)) => Err(LoadError::Loading(e.clone())),
                Poll::Pending => Ok(BytesPoll::Pending { size: None }),
            };
        }
        cache.insert(uri.to_string(), Poll::Pending);
        drop(cache);

        let uri = uri.to_string();
        let path = self.dir.join(key(&uri));
        let cache = self.cache.clone();
        let http = self.http.clone();
        let limit = self.limit.clone();
        let ctx = ctx.clone();
        self.rt.spawn(async move {
            let res = match tokio::fs::read(&path).await {
                Ok(b) if !b.is_empty() => Ok(b),
                _ => {
                    let _permit = limit.acquire().await;
                    fetch(&http, &uri).await.inspect(|b| {
                        let tmp = path.with_extension("part");
                        if std::fs::write(&tmp, b).is_ok() {
                            let _ = std::fs::rename(&tmp, &path);
                        }
                    })
                }
            };
            let entry = res.map(|b| {
                let mime = sniff(&b).to_string();
                (Arc::<[u8]>::from(b), mime)
            });
            let keep = {
                let mut c = cache.lock();
                match c.get_mut(&uri) {
                    Some(e) => {
                        *e = Poll::Ready(entry);
                        true
                    }
                    None => false,
                }
            };
            if keep {
                ctx.request_repaint();
            }
        });
        Ok(BytesPoll::Pending { size: None })
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
                Poll::Ready(Ok((b, _))) => b.len(),
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache.lock().values().any(|e| e.is_pending())
    }
}

async fn fetch(http: &reqwest::Client, uri: &str) -> Result<Vec<u8>, String> {
    let r = http.get(uri).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("{} {}", r.status(), uri));
    }
    r.bytes().await.map(|b| b.to_vec()).map_err(|e| e.to_string())
}

//! A file read over http a piece at a time, for Media Foundation to play while it arrives
//! instead of after it has all come down. The reader sees an IStream; behind it are a few ranged
//! requests kept as windows, with the next one fetched before the player runs into it.

use std::sync::{Arc, Condvar, Mutex};

use windows::Win32::Foundation::{E_NOTIMPL, E_OUTOFMEMORY};
use windows::Win32::System::Com::{ISequentialStream_Impl, IStream, IStream_Impl, LOCKTYPE, STATFLAG, STATSTG, STGC, STGTY_STREAM, STREAM_SEEK, STREAM_SEEK_CUR, STREAM_SEEK_END, STREAM_SEEK_SET};
use windows::Win32::Media::MediaFoundation::{IMFAttributes, IMFByteStream, MF_BYTESTREAM_ORIGIN_NAME, MFCreateMFByteStreamOnStream};
use windows::core::{HRESULT, HSTRING, Interface, Ref};
use windows_core::implement;

const FIRST: u64 = 1 << 20;
const LAST: u64 = 8 << 20;
const KEEP: usize = 4;

pub struct Piece {
    pub start: u64,
    pub bytes: Vec<u8>,
    pub total: u64,
}

pub trait Fetch: Send + Sync + 'static {
    fn get(&self, from: u64, len: u64) -> Option<Piece>;
}

struct St {
    pos: u64,
    total: Option<u64>,
    wins: Vec<(u64, Vec<u8>)>,
    fetching: Option<(u64, u64)>,
    chunk: u64,
}

pub struct Ranged<F> {
    fetch: F,
    st: Mutex<St>,
    cv: Condvar,
}

impl<F: Fetch> Ranged<F> {
    pub fn new(fetch: F) -> Arc<Self> {
        Arc::new(Ranged { fetch, st: Mutex::new(St { pos: 0, total: None, wins: Vec::new(), fetching: None, chunk: FIRST }), cv: Condvar::new() })
    }

    fn load(&self, from: u64, len: u64) {
        let got = self.fetch.get(from, len);
        let mut st = self.st.lock().unwrap();
        st.fetching = None;
        if let Some(p) = got {
            st.total = Some(p.total);
            st.wins.retain(|(s, _)| *s != p.start);
            st.wins.push((p.start, p.bytes));
            if st.wins.len() > KEEP {
                let pos = st.pos;
                let far = st.wins.iter().enumerate().max_by_key(|(_, (s, b))| if pos >= *s + b.len() as u64 { pos - (*s + b.len() as u64) } else { s.saturating_sub(pos) }).map(|(i, _)| i);
                if let Some(i) = far {
                    st.wins.swap_remove(i);
                }
            }
        }
        self.cv.notify_all();
    }

    pub fn total(self: &Arc<Self>) -> Option<u64> {
        if let Some(t) = self.st.lock().unwrap().total {
            return Some(t);
        }
        self.load(0, FIRST);
        self.st.lock().unwrap().total
    }

    pub fn read(self: &Arc<Self>, out: &mut [u8]) -> usize {
        loop {
            let mut st = self.st.lock().unwrap();
            let pos = st.pos;
            if st.total.is_some_and(|t| pos >= t) {
                return 0;
            }
            let hit = st.wins.iter().find(|(s, b)| pos >= *s && pos < *s + b.len() as u64).map(|(s, b)| {
                let at = (pos - s) as usize;
                let n = out.len().min(b.len() - at);
                out[..n].copy_from_slice(&b[at..at + n]);
                (n, *s, *s + b.len() as u64)
            });
            if let Some((n, start, end)) = hit {
                st.pos += n as u64;
                let next = st.wins.iter().any(|(s, _)| *s == end);
                if !next && st.fetching.is_none() && st.total.is_some_and(|t| end < t) && st.pos - start > (end - start) / 2 {
                    let len = st.chunk;
                    st.chunk = (st.chunk * 2).min(LAST);
                    st.fetching = Some((end, len));
                    drop(st);
                    let me = self.clone();
                    std::thread::spawn(move || me.load(end, len));
                }
                return n;
            }
            if let Some((s, l)) = st.fetching
                && pos >= s
                && pos < s + l
            {
                drop(self.cv.wait(st).unwrap());
                continue;
            }
            let len = if st.wins.iter().any(|(s, b)| *s + b.len() as u64 == pos) { st.chunk } else { FIRST };
            st.fetching = Some((pos, len));
            drop(st);
            self.load(pos, len);
            let st = self.st.lock().unwrap();
            if !st.wins.iter().any(|(s, b)| pos >= *s && pos < *s + b.len() as u64) {
                return 0;
            }
        }
    }

    fn seek(&self, to: i64, from: STREAM_SEEK, total: u64) -> Option<u64> {
        let mut st = self.st.lock().unwrap();
        let base = match from {
            STREAM_SEEK_SET => 0,
            STREAM_SEEK_CUR => st.pos as i64,
            STREAM_SEEK_END => total as i64,
            _ => return None,
        };
        let at = base.checked_add(to).filter(|p| *p >= 0)? as u64;
        st.pos = at;
        Some(at)
    }
}

#[implement(IStream)]
struct Reader(Arc<Ranged<Http>>);

impl ISequentialStream_Impl for Reader_Impl {
    fn Read(&self, pv: *mut core::ffi::c_void, cb: u32, pcbread: *mut u32) -> HRESULT {
        let out = unsafe { std::slice::from_raw_parts_mut(pv as *mut u8, cb as usize) };
        let n = self.0.read(out);
        if !pcbread.is_null() {
            unsafe { *pcbread = n as u32 };
        }
        HRESULT(0)
    }

    fn Write(&self, _: *const core::ffi::c_void, _: u32, _: *mut u32) -> HRESULT {
        E_NOTIMPL
    }
}

impl IStream_Impl for Reader_Impl {
    fn Seek(&self, dlibmove: i64, dworigin: STREAM_SEEK, plibnewposition: *mut u64) -> windows::core::Result<()> {
        let total = self.0.total().ok_or_else(|| windows::core::Error::from(E_OUTOFMEMORY))?;
        let at = self.0.seek(dlibmove, dworigin, total).ok_or_else(|| windows::core::Error::from(E_NOTIMPL))?;
        if !plibnewposition.is_null() {
            unsafe { *plibnewposition = at };
        }
        Ok(())
    }

    fn SetSize(&self, _: u64) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn CopyTo(&self, _: Ref<'_, IStream>, _: u64, _: *mut u64, _: *mut u64) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn Commit(&self, _: &STGC) -> windows::core::Result<()> {
        Ok(())
    }

    fn Revert(&self) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn LockRegion(&self, _: u64, _: u64, _: &LOCKTYPE) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn UnlockRegion(&self, _: u64, _: u64, _: u32) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn Stat(&self, pstatstg: *mut STATSTG, _: &STATFLAG) -> windows::core::Result<()> {
        let total = self.0.total().ok_or_else(|| windows::core::Error::from(E_OUTOFMEMORY))?;
        unsafe { *pstatstg = STATSTG { r#type: STGTY_STREAM.0 as u32, cbSize: total, ..Default::default() } };
        Ok(())
    }

    fn Clone(&self) -> windows::core::Result<IStream> {
        Err(E_NOTIMPL.into())
    }
}

/// A byte stream for the source reader, named like the file so it picks the right parser.
pub fn byte_stream(fetch: Http, name: &str) -> windows::core::Result<IMFByteStream> {
    unsafe {
        let stream: IStream = Reader(Ranged::new(fetch)).into();
        let bs = MFCreateMFByteStreamOnStream(&stream)?;
        bs.cast::<IMFAttributes>()?.SetString(&MF_BYTESTREAM_ORIGIN_NAME, &HSTRING::from(name))?;
        Ok(bs)
    }
}

#[derive(Clone)]
pub struct Http {
    pub http: reqwest::Client,
    pub url: String,
    pub rt: tokio::runtime::Handle,
}

impl Fetch for Http {
    fn get(&self, from: u64, len: u64) -> Option<Piece> {
        self.rt.block_on(async {
            let r = self.http.get(&self.url).header(reqwest::header::RANGE, format!("bytes={from}-{}", from + len - 1)).send().await.ok()?;
            match r.status().as_u16() {
                206 => {
                    let range = r.headers().get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
                    let total = range.rsplit('/').next()?.parse().ok()?;
                    Some(Piece { start: from, bytes: r.bytes().await.ok()?.to_vec(), total })
                }
                200 => {
                    let bytes = r.bytes().await.ok()?.to_vec();
                    Some(Piece { start: 0, total: bytes.len() as u64, bytes })
                }
                _ => None,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

    struct Memory {
        data: Vec<u8>,
        asked: Arc<AtomicUsize>,
        ranged: bool,
    }

    impl Fetch for Memory {
        fn get(&self, from: u64, len: u64) -> Option<Piece> {
            self.asked.fetch_add(1, Relaxed);
            let total = self.data.len() as u64;
            if !self.ranged {
                return Some(Piece { start: 0, bytes: self.data.clone(), total });
            }
            let end = (from + len).min(total);
            (from < total).then(|| Piece { start: from, bytes: self.data[from as usize..end as usize].to_vec(), total })
        }
    }

    fn file(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn all(r: &Arc<Ranged<Memory>>, buf: usize) -> Vec<u8> {
        let (mut out, mut b) = (Vec::new(), vec![0u8; buf]);
        loop {
            let n = r.read(&mut b);
            if n == 0 {
                return out;
            }
            out.extend_from_slice(&b[..n]);
        }
    }

    #[test]
    fn reads_the_whole_file_in_pieces() {
        let data = file(5_000_000);
        let asked = Arc::new(AtomicUsize::new(0));
        let r = Ranged::new(Memory { data: data.clone(), asked: asked.clone(), ranged: true });
        assert_eq!(all(&r, 65_536), data);
        assert!(asked.load(Relaxed) >= 3, "{}", asked.load(Relaxed));
    }

    #[test]
    fn a_far_seek_fetches_only_what_is_there() {
        let data = file(20_000_000);
        let asked = Arc::new(AtomicUsize::new(0));
        let r = Ranged::new(Memory { data: data.clone(), asked: asked.clone(), ranged: true });
        let total = r.total().unwrap();
        assert_eq!(total, data.len() as u64);
        assert_eq!(r.seek(-100, STREAM_SEEK_END, total), Some(total - 100));
        let mut tail = vec![0u8; 200];
        assert_eq!(r.read(&mut tail), 100);
        assert_eq!(&tail[..100], &data[data.len() - 100..]);
        assert_eq!(r.seek(7_000_000, STREAM_SEEK_SET, total), Some(7_000_000));
        let mut mid = vec![0u8; 1000];
        assert_eq!(r.read(&mut mid), 1000);
        assert_eq!(mid, &data[7_000_000..7_001_000]);
        assert!(asked.load(Relaxed) <= 4, "{}", asked.load(Relaxed));
    }

    #[test]
    fn a_server_without_ranges_still_plays() {
        let data = file(3_000_000);
        let asked = Arc::new(AtomicUsize::new(0));
        let r = Ranged::new(Memory { data: data.clone(), asked: asked.clone(), ranged: false });
        assert_eq!(all(&r, 4096), data);
        assert_eq!(asked.load(Relaxed), 1);
    }
}

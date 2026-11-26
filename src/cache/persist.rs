use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{OnceLock, mpsc};

use super::{Settings, UiState};
use crate::state::{Id, Message, Msgs, ReadState, Set, Store};

const COMPACT_ENTRIES: usize = 64;
const COMPACT_BYTES: usize = 256 << 10;

static EPOCH: AtomicU64 = AtomicU64::new(0);
static SEQ: AtomicU64 = AtomicU64::new(0);
static COMPACT_DUE: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static WORKER: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

#[derive(Serialize)]
struct SnapshotOut<'a> {
    seq: u64,
    store: &'a Store,
    msgs: Vec<(Id, &'a [Message])>,
    ui: &'a UiState,
}

#[derive(Deserialize)]
struct SnapshotIn {
    #[serde(default)]
    seq: u64,
    store: Store,
    msgs: Vec<(Id, Vec<Message>)>,
    ui: UiState,
}

#[derive(Serialize)]
struct UiOut<'a> {
    seq: u64,
    ui: &'a UiState,
}

#[derive(Deserialize)]
struct UiIn {
    seq: u64,
    ui: UiState,
}

#[derive(Serialize)]
struct JournalOut<'a> {
    seq: u64,
    op: JournalOpOut<'a>,
}

#[derive(Serialize)]
enum JournalOpOut<'a> {
    Read { ch: Id, state: &'a ReadState },
    LastMsg { ch: Id, id: Id },
    Msgs { ch: Id, list: &'a [Message] },
    Ui(&'a UiState),
}

#[derive(Deserialize)]
struct JournalIn {
    seq: u64,
    op: JournalOpIn,
}

#[derive(Deserialize)]
enum JournalOpIn {
    Read { ch: Id, state: ReadState },
    LastMsg { ch: Id, id: Id },
    Msgs { ch: Id, list: Vec<Message> },
    Ui(UiState),
}

enum Job {
    Settings { dir: PathBuf, seq: u64, bytes: Vec<u8> },
    Ui { dir: PathBuf, epoch: u64, seq: u64, bytes: Vec<u8> },
    Journal { dir: PathBuf, epoch: u64, lines: Vec<Vec<u8>> },
    Snapshot { dir: PathBuf, epoch: u64, seq: u64, rx: mpsc::Receiver<Vec<u8>> },
    Clear { dir: PathBuf },
    Flush(mpsc::Sender<()>),
}

fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed) + 1
}

fn atomic_write(dir: &Path, name: &str, seq: u64, bytes: &[u8]) -> bool {
    let _ = std::fs::create_dir_all(dir);
    let tmp = dir.join(format!("{name}.{seq}.part"));
    if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, dir.join(name)).is_ok() {
        true
    } else {
        let _ = std::fs::remove_file(&tmp);
        false
    }
}

fn worker() -> &'static mpsc::Sender<Job> {
    WORKER.get_or_init(|| {
        STARTED.store(true, Ordering::Release);
        let (tx, rx) = mpsc::channel::<Job>();
        let _ = std::thread::Builder::new()
            .name("cache-io".into())
            .stack_size(512 << 10)
            .spawn(move || run_worker(rx));
        tx
    })
}

fn run_worker(rx: mpsc::Receiver<Job>) {
    let mut journal_entries = 0usize;
    let mut journal_bytes = 0usize;
    for job in rx {
        match job {
            Job::Settings { dir, seq, bytes } => {
                atomic_write(&dir, "settings.json", seq, &bytes);
            }
            Job::Ui { dir, epoch, seq, bytes } => {
                if epoch == EPOCH.load(Ordering::Acquire) {
                    atomic_write(&dir, "ui.json", seq, &bytes);
                }
            }
            Job::Journal { dir, epoch, lines } => {
                if epoch != EPOCH.load(Ordering::Acquire) || lines.is_empty() {
                    continue;
                }
                let _ = std::fs::create_dir_all(&dir);
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("journal.jsonl")) {
                    let mut ok = true;
                    let mut added = 0usize;
                    for line in &lines {
                        if f.write_all(line).is_err() || f.write_all(b"\n").is_err() {
                            ok = false;
                            break;
                        }
                        added += line.len() + 1;
                    }
                    if ok {
                        journal_entries += lines.len();
                        journal_bytes += added;
                        if journal_entries >= COMPACT_ENTRIES || journal_bytes >= COMPACT_BYTES {
                            COMPACT_DUE.store(true, Ordering::Relaxed);
                        }
                    }
                }
            }
            Job::Snapshot { dir, epoch, seq, rx: pieces } => {
                let _ = std::fs::create_dir_all(&dir);
                let tmp = dir.join(format!("state.json.{seq}.part"));
                let mut file = (epoch == EPOCH.load(Ordering::Acquire)).then(|| std::fs::File::create(&tmp).ok()).flatten();
                let mut whole = false;
                for piece in pieces {
                    whole = piece.is_empty();
                    if file.as_mut().is_some_and(|f| f.write_all(&piece).is_err()) {
                        file = None;
                    }
                }
                if whole && file.take().is_some() && epoch == EPOCH.load(Ordering::Acquire) && std::fs::rename(&tmp, dir.join("state.json")).is_ok() {
                    let _ = std::fs::remove_file(dir.join("journal.jsonl"));
                    let _ = std::fs::remove_file(dir.join("ui.json"));
                    journal_entries = 0;
                    journal_bytes = 0;
                    COMPACT_DUE.store(false, Ordering::Relaxed);
                } else {
                    let _ = std::fs::remove_file(&tmp);
                }
            }
            Job::Clear { dir } => {
                let _ = std::fs::remove_file(dir.join("token"));
                let _ = std::fs::remove_file(dir.join("state.json"));
                let _ = std::fs::remove_file(dir.join("ui.json"));
                let _ = std::fs::remove_file(dir.join("journal.jsonl"));
                journal_entries = 0;
                journal_bytes = 0;
                COMPACT_DUE.store(false, Ordering::Relaxed);
            }
            Job::Flush(done) => {
                let _ = done.send(());
            }
        }
    }
}

pub(super) fn flush() {
    if !STARTED.load(Ordering::Acquire) {
        return;
    }
    let (tx, rx) = mpsc::channel();
    if worker().send(Job::Flush(tx)).is_ok() {
        let _ = rx.recv();
    }
}

pub(super) fn compact_due() -> bool {
    COMPACT_DUE.load(Ordering::Relaxed)
}

pub(super) fn write_settings(dir: PathBuf, s: &Settings) {
    if let Ok(bytes) = serde_json::to_vec_pretty(s) {
        let seq = next_seq();
        let _ = worker().send(Job::Settings { dir, seq, bytes });
    }
}

pub(super) fn save_ui(dir: PathBuf, ui: &UiState) {
    let epoch = EPOCH.load(Ordering::Acquire);
    let seq = next_seq();
    if let Ok(bytes) = serde_json::to_vec(&UiOut { seq, ui }) {
        let _ = worker().send(Job::Ui { dir, epoch, seq, bytes });
    }
}

fn sent_tail(list: &[Message]) -> &[Message] {
    let sent = list.iter().take_while(|x| !x.pending && !x.failed).count();
    &list[sent.saturating_sub(50)..sent]
}

pub(super) fn append_journal(
    dir: PathBuf,
    store: &Store,
    ui: &UiState,
    reads: &Set<Id>,
    last_msgs: &Set<Id>,
    msgs: &Set<Id>,
    include_ui: bool,
) {
    let epoch = EPOCH.load(Ordering::Acquire);
    let mut lines = Vec::new();
    for &ch in reads {
        if let Some(state) = store.reads.get(&ch)
            && let Ok(b) = serde_json::to_vec(&JournalOut { seq: next_seq(), op: JournalOpOut::Read { ch, state } })
        {
            lines.push(b);
        }
    }
    for &ch in last_msgs {
        if let Some(c) = store.channels.get(&ch)
            && let Ok(b) = serde_json::to_vec(&JournalOut { seq: next_seq(), op: JournalOpOut::LastMsg { ch, id: c.last_msg } })
        {
            lines.push(b);
        }
    }
    for &ch in msgs {
        if ui.recent.contains(&ch)
            && let Some(m) = store.msgs.get(&ch)
            && let Ok(b) = serde_json::to_vec(&JournalOut { seq: next_seq(), op: JournalOpOut::Msgs { ch, list: sent_tail(&m.list) } })
        {
            lines.push(b);
        }
    }
    if include_ui && let Ok(b) = serde_json::to_vec(&JournalOut { seq: next_seq(), op: JournalOpOut::Ui(ui) }) {
        lines.push(b);
    }
    if !lines.is_empty() {
        let _ = worker().send(Job::Journal { dir, epoch, lines });
    }
}

pub(super) fn save_snapshot(dir: PathBuf, store: &Store, ui: &UiState, wait: bool) {
    let msgs = ui
        .recent
        .iter()
        .filter_map(|c| store.msgs.get(c).map(|m| (*c, sent_tail(&m.list))))
        .collect();
    let epoch = EPOCH.load(Ordering::Acquire);
    let seq = next_seq();
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);
    let _ = worker().send(Job::Snapshot { dir, epoch, seq, rx });
    let mut pieces = Pieces { buf: Vec::new(), tx };
    if serde_json::to_writer(&mut pieces, &SnapshotOut { seq, store, msgs, ui }).is_ok() {
        let _ = pieces.flush();
        let _ = pieces.tx.send(Vec::new());
    }
    drop(pieces);
    if wait {
        flush();
    }
}

pub(super) fn clear_session(dir: PathBuf) {
    EPOCH.fetch_add(1, Ordering::AcqRel);
    if STARTED.load(Ordering::Acquire) {
        let _ = worker().send(Job::Clear { dir: dir.clone() });
        flush();
    }
    let _ = std::fs::remove_file(dir.join("token"));
    let _ = std::fs::remove_file(dir.join("state.json"));
    let _ = std::fs::remove_file(dir.join("ui.json"));
    let _ = std::fs::remove_file(dir.join("journal.jsonl"));
}

pub(super) fn load_dir(dir: &Path) -> Option<(Store, UiState)> {
    flush();
    let bytes = std::fs::read(dir.join("state.json")).ok()?;
    let mut i: SnapshotIn = serde_json::from_slice(&bytes).ok()?;
    for (ch, list) in i.msgs {
        i.store.msgs.insert(ch, Msgs { list, ..Default::default() });
    }
    let mut max_seq = i.seq;
    let mut ui_seq = i.seq;
    if let Ok(raw) = std::fs::read(dir.join("journal.jsonl")) {
        for line in raw.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let Ok(entry) = serde_json::from_slice::<JournalIn>(line) else { continue };
            if entry.seq <= i.seq {
                continue;
            }
            max_seq = max_seq.max(entry.seq);
            match entry.op {
                JournalOpIn::Read { ch, state } => {
                    i.store.reads.insert(ch, state);
                }
                JournalOpIn::LastMsg { ch, id } => {
                    if let Some(c) = i.store.channels.get_mut(&ch) {
                        c.last_msg = c.last_msg.max(id);
                    }
                }
                JournalOpIn::Msgs { ch, list } => {
                    i.store.msgs.insert(ch, Msgs { list, ..Default::default() });
                }
                JournalOpIn::Ui(u) => {
                    if entry.seq >= ui_seq {
                        i.ui = u;
                        ui_seq = entry.seq;
                    }
                }
            }
        }
    }
    if let Ok(raw) = std::fs::read(dir.join("ui.json"))
        && let Ok(u) = serde_json::from_slice::<UiIn>(&raw)
    {
        max_seq = max_seq.max(u.seq);
        if u.seq > ui_seq {
            i.ui = u.ui;
        }
    }
    SEQ.fetch_max(max_seq, Ordering::Relaxed);
    Some((i.store, i.ui))
}

struct Pieces {
    buf: Vec<u8>,
    tx: mpsc::SyncSender<Vec<u8>>,
}

impl std::io::Write for Pieces {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        if self.buf.len() >= 1 << 16 {
            self.flush()?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            let _ = self.tx.send(std::mem::replace(&mut self.buf, Vec::with_capacity(1 << 16)));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Channel;

    fn temp_dir(name: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("rustcord-cache-test-{}-{name}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn incremental_ui_and_journal_replay_over_base_snapshot() {
        let dir = temp_dir("replay");
        let mut store = Store { me: 42, ..Default::default() };
        store.channels.insert(100, Channel { id: 100, last_msg: 10, ..Default::default() });
        let mut ui = UiState { guild: Some(7), recent: vec![100], side_w: 240.0, ..Default::default() };
        save_snapshot(dir.clone(), &store, &ui, true);

        store.reads.insert(100, ReadState { last_acked: 25, mentions: 2, ..Default::default() });
        store.channels.get_mut(&100).unwrap().last_msg = 25;
        store.msgs.insert(100, Msgs { list: vec![Message { id: 25, channel: 100, content: "hi".into(), ..Default::default() }], ..Default::default() });
        let mut chs = Set::default();
        chs.insert(100);
        append_journal(dir.clone(), &store, &ui, &chs, &chs, &chs, false);

        ui.side_w = 310.0;
        save_ui(dir.clone(), &ui);
        flush();

        let mut f = std::fs::OpenOptions::new().append(true).open(dir.join("journal.jsonl")).unwrap();
        let _ = f.write_all(b"{\"seq\":999999,\"op\":{truncated");
        drop(f);

        let (loaded_store, loaded_ui) = load_dir(&dir).expect("load");
        assert_eq!(loaded_store.me, 42);
        assert_eq!(loaded_store.reads.get(&100).unwrap().last_acked, 25);
        assert_eq!(loaded_store.reads.get(&100).unwrap().mentions, 2);
        assert_eq!(loaded_store.channels.get(&100).unwrap().last_msg, 25);
        assert_eq!(loaded_store.msgs.get(&100).unwrap().list.len(), 1);
        assert_eq!(loaded_store.msgs.get(&100).unwrap().list[0].content, "hi");
        assert_eq!(loaded_ui.side_w, 310.0);

        save_snapshot(dir.clone(), &loaded_store, &loaded_ui, true);
        assert!(!dir.join("journal.jsonl").exists());
        assert!(!dir.join("ui.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_state_json_without_seq_loads_cleanly() {
        let dir = temp_dir("legacy");
        let legacy = serde_json::json!({
            "store": Store { me: 99, ..Default::default() },
            "msgs": [],
            "ui": UiState { side_w: 275.0, ..Default::default() }
        });
        std::fs::write(dir.join("state.json"), serde_json::to_vec(&legacy).unwrap()).unwrap();
        let (store, ui) = load_dir(&dir).expect("legacy load");
        assert_eq!(store.me, 99);
        assert_eq!(ui.side_w, 275.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_and_clear_session_work_off_ui_thread() {
        let dir = temp_dir("settings");
        let mut s = Settings::default();
        s.font_size = 18.0;
        write_settings(dir.clone(), &s);
        flush();
        let read: Settings = serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(read.font_size, 18.0);

        let store = Store { me: 1, ..Default::default() };
        let ui = UiState::default();
        save_snapshot(dir.clone(), &store, &ui, true);
        save_ui(dir.clone(), &ui);
        clear_session(dir.clone());
        assert!(!dir.join("state.json").exists());
        assert!(!dir.join("ui.json").exists());
        assert!(!dir.join("journal.jsonl").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

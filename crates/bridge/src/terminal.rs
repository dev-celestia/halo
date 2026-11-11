//! Runtime store for terminal sessions — independent of work tabs.
//!
//! PTY output used to live inside the flat `BridgeState.coder` projection, which
//! meant switching work tabs restored a stale copy and dropped everything that
//! arrived meanwhile. Sessions now live here, keyed by terminal session id, so
//! output survives tab switches, splits and multiple windows.
//!
//! Reads are cursor-based instead of destructive: every consumer keeps its own
//! byte offset and sees each chunk exactly once. When a consumer falls behind
//! the retained window, [`TerminalStore::read_from`] reports
//! [`TerminalRead::truncated`] so the caller can drop its VT grid and restart
//! from the tail.
//!
//! The store is an `Arc<Mutex<..>>` so [`crate::BridgeState`] stays cheap to
//! clone (see the hand-written `PartialEq` below) and so
//! [`crate::BridgeState::apply_events`] can append without an `App` context.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

/// Retained tail per session. Beyond this the oldest bytes are dropped and the
/// session epoch is bumped so every surface resets its grid.
pub const TERMINAL_BUFFER_CAP: usize = 512_000;

struct Session {
    /// From `pty::start`; exit events for older generations are ignored.
    generation: u64,
    /// Bumped whenever the retained buffer is no longer contiguous with what a
    /// reader already consumed (truncation, reset). Surfaces reset their grid.
    epoch: u64,
    alive: bool,
    cwd: String,
    /// Raw PTY stream as UTF-8 — CSI / CR preserved for the VT parser.
    buf: String,
    /// Absolute offset of `buf[0]`.
    base: u64,
    /// Absolute offset just past `buf`.
    end: u64,
}

impl Session {
    fn new(generation: u64, cwd: String) -> Self {
        Self {
            generation,
            epoch: 0,
            alive: true,
            cwd,
            buf: String::new(),
            base: 0,
            end: 0,
        }
    }

    fn clear(&mut self) {
        self.buf.clear();
        self.base = 0;
        self.end = 0;
        self.epoch = self.epoch.wrapping_add(1);
    }
}

#[derive(Default)]
struct Inner {
    sessions: HashMap<String, Session>,
}

/// What one reader has not seen yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalRead {
    /// Session epoch — a change means the reader must reset its grid.
    pub epoch: u64,
    /// Absolute offset to pass as `cursor` on the next read.
    pub seq: u64,
    pub alive: bool,
    /// The requested cursor fell out of the retained window; `data` starts at
    /// the oldest retained byte. Reset the grid before feeding it.
    pub truncated: bool,
    pub data: String,
}

/// Header-only view of a session, for status chrome.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalMeta {
    pub id: String,
    pub epoch: u64,
    pub alive: bool,
    /// Bytes held for readers that have not caught up yet.
    pub pending_len: usize,
    pub cwd: String,
}

/// Shared PTY session buffers. Cloning is an `Arc` bump.
#[derive(Clone, Default)]
pub struct TerminalStore {
    inner: Arc<Mutex<Inner>>,
}

impl TerminalStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or adopt) a session after its PTY spawned.
    ///
    /// Adoption matters: the reader thread can emit the shell's first banner
    /// before `pty_start` returns, and [`Self::push`] creates an entry for that
    /// unknown id. Adopting keeps those bytes instead of wiping them.
    pub fn create(&self, id: &str, cwd: &str, generation: u64) {
        let mut inner = self.inner.lock();
        match inner.sessions.get_mut(id) {
            Some(session) => {
                session.generation = generation;
                session.alive = true;
                session.cwd = cwd.to_string();
            }
            None => {
                inner
                    .sessions
                    .insert(id.to_string(), Session::new(generation, cwd.to_string()));
            }
        }
    }

    /// Append raw PTY output. Unknown ids are created with an unclaimed
    /// generation so nothing is lost to the start/adopt race.
    pub fn push(&self, id: &str, data: &str) {
        if data.is_empty() {
            return;
        }
        let mut inner = self.inner.lock();
        let session = inner
            .sessions
            .entry(id.to_string())
            .or_insert_with(|| Session::new(0, String::new()));
        session.alive = true;
        session.buf.push_str(data);
        if session.buf.len() > TERMINAL_BUFFER_CAP {
            // The cut is a byte offset, so it must land on a char boundary or
            // `drain` panics on any multi-byte codepoint straddling it (CJK /
            // emoji / box drawing in build output).
            let mut cut = session.buf.len() - TERMINAL_BUFFER_CAP;
            while cut < session.buf.len() && !session.buf.is_char_boundary(cut) {
                cut += 1;
            }
            session.buf.drain(0..cut);
            session.base += cut as u64;
            session.epoch = session.epoch.wrapping_add(1);
        }
        session.end = session.base + session.buf.len() as u64;
    }

    /// Mark a session dead. Exits from a previous generation are ignored so a
    /// restarted session is not killed by its predecessor's reader.
    pub fn mark_exit(&self, id: &str, generation: u64) {
        let mut inner = self.inner.lock();
        let Some(session) = inner.sessions.get_mut(id) else {
            return;
        };
        if session.generation != 0 && generation != 0 && session.generation != generation {
            return;
        }
        if session.generation == 0 {
            session.generation = generation;
        }
        session.alive = false;
    }

    /// Mark dead regardless of generation — a local kill, not a process exit.
    pub fn mark_dead(&self, id: &str) {
        if let Some(session) = self.inner.lock().sessions.get_mut(id) {
            session.alive = false;
        }
    }

    /// Drop retained output and bump the epoch. Used before a restart.
    pub fn reset(&self, id: &str) {
        let mut inner = self.inner.lock();
        if let Some(session) = inner.sessions.get_mut(id) {
            session.clear();
            session.alive = false;
        }
    }

    /// Read everything after `cursor` without consuming it.
    pub fn read_from(&self, id: &str, cursor: u64) -> Option<TerminalRead> {
        let inner = self.inner.lock();
        let session = inner.sessions.get(id)?;
        let truncated = cursor < session.base;
        let from = if truncated { session.base } else { cursor };
        let mut start = from.saturating_sub(session.base) as usize;
        if start > session.buf.len() {
            start = session.buf.len();
        }
        // A cursor is normally the `seq` we handed out (a char boundary), but a
        // surface may pass an arbitrary offset after a font-driven rebuild.
        while start > 0 && !session.buf.is_char_boundary(start) {
            start -= 1;
        }
        Some(TerminalRead {
            epoch: session.epoch,
            seq: session.end,
            alive: session.alive,
            truncated,
            data: session.buf[start..].to_string(),
        })
    }

    pub fn meta(&self, id: &str) -> Option<TerminalMeta> {
        let inner = self.inner.lock();
        inner.sessions.get(id).map(|session| TerminalMeta {
            id: id.to_string(),
            epoch: session.epoch,
            alive: session.alive,
            pending_len: session.buf.len(),
            cwd: session.cwd.clone(),
        })
    }

    pub fn metas(&self) -> Vec<TerminalMeta> {
        let inner = self.inner.lock();
        let mut metas: Vec<TerminalMeta> = inner
            .sessions
            .iter()
            .map(|(id, session)| TerminalMeta {
                id: id.clone(),
                epoch: session.epoch,
                alive: session.alive,
                pending_len: session.buf.len(),
                cwd: session.cwd.clone(),
            })
            .collect();
        metas.sort_by(|a, b| a.id.cmp(&b.id));
        metas
    }

    pub fn alive(&self, id: &str) -> bool {
        self.inner
            .lock()
            .sessions
            .get(id)
            .map(|session| session.alive)
            .unwrap_or(false)
    }

    /// Whether the store knows this session at all (dead or alive).
    pub fn has(&self, id: &str) -> bool {
        self.inner.lock().sessions.contains_key(id)
    }

    pub fn any_alive(&self) -> bool {
        self.inner
            .lock()
            .sessions
            .values()
            .any(|session| session.alive)
    }

    /// Forget a session entirely (its tab/pane closed). Dropping the buffer is
    /// safe: no surface can still be showing it.
    pub fn remove(&self, id: &str) {
        self.inner.lock().sessions.remove(id);
    }

    pub fn ids(&self) -> Vec<String> {
        let inner = self.inner.lock();
        let mut ids: Vec<String> = inner.sessions.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn len(&self) -> usize {
        self.inner.lock().sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Two states sharing the same store are equal: the buffers are runtime data
/// driven by `shell_epoch`, not by snapshot diffing.
impl PartialEq for TerminalStore {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for TerminalStore {}

impl std::fmt::Debug for TerminalStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.inner.try_lock() {
            Some(inner) => f
                .debug_struct("TerminalStore")
                .field("sessions", &inner.sessions.len())
                .finish(),
            None => f.write_str("TerminalStore { <locked> }"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn two_readers_each_see_every_byte_once() {
        let store = TerminalStore::new();
        store.create("t", "/tmp", 1);
        store.push("t", "hello ");
        store.push("t", "world");

        let first = store.read_from("t", 0).expect("session exists");
        assert_eq!(first.data, "hello world");
        assert_eq!(first.seq, 11);
        assert!(!first.truncated);

        let a = store.read_from("t", 0).unwrap();
        let b = store.read_from("t", 0).unwrap();
        assert_eq!(a.data, b.data);

        store.push("t", "!");
        assert_eq!(store.read_from("t", a.seq).unwrap().data, "!");
        // The other reader also picks up only the new byte at its own cursor.
        assert_eq!(store.read_from("t", 11).unwrap().data, "!");
        assert!(store.read_from("t", 12).unwrap().data.is_empty());
    }

    #[test]
    fn truncation_keeps_the_tail_and_flags_the_reader() {
        let store = TerminalStore::new();
        store.create("t", "/tmp", 1);
        let chunk = "é".repeat(TERMINAL_BUFFER_CAP / 2 + 10);
        store.push("t", &chunk);

        let meta = store.meta("t").unwrap();
        assert!(meta.pending_len <= TERMINAL_BUFFER_CAP);
        assert_eq!(meta.epoch, 1, "truncation bumps the epoch");

        let read = store.read_from("t", 0).unwrap();
        assert!(read.truncated);
        assert_eq!(read.epoch, 1);
        // Never panics on a multibyte boundary, and the tail is intact.
        assert!(read.data.ends_with("é"));
        assert_eq!(read.data.len(), store.meta("t").unwrap().pending_len);
        assert_eq!(read.seq as usize, chunk.len(), "absolute end offset");
    }

    #[test]
    fn stale_generation_exit_is_ignored() {
        let store = TerminalStore::new();
        store.create("t", "/tmp", 1);
        store.mark_exit("t", 2);
        assert!(store.alive("t"), "older generation must not kill it");

        store.mark_exit("t", 1);
        assert!(!store.alive("t"));
        assert!(!store.any_alive());
    }

    #[test]
    fn reset_bumps_epoch_and_clears() {
        let store = TerminalStore::new();
        store.create("t", "/tmp", 1);
        store.push("t", "before");
        assert_eq!(store.meta("t").unwrap().epoch, 0);

        store.reset("t");
        let meta = store.meta("t").unwrap();
        assert_eq!(meta.epoch, 1);
        assert_eq!(meta.pending_len, 0);
        assert!(!meta.alive);

        store.push("t", "after");
        let read = store.read_from("t", 0).unwrap();
        assert_eq!(read.data, "after");
        assert_eq!(read.seq, 5);
    }

    #[test]
    fn adoption_keeps_bytes_that_arrived_before_start_returned() {
        let store = TerminalStore::new();
        store.push("t", "early prompt");
        assert_eq!(store.meta("t").unwrap().pending_len, 12);

        store.create("t", "/work", 7);
        assert_eq!(store.read_from("t", 0).unwrap().data, "early prompt");
        assert_eq!(store.meta("t").unwrap().cwd, "/work");

        store.mark_exit("t", 3);
        assert!(store.alive("t"), "stale exit rejected after adoption");
        store.mark_exit("t", 7);
        assert!(!store.alive("t"));
    }

    #[test]
    fn cursor_past_the_end_reads_ahead_only() {
        let store = TerminalStore::new();
        store.create("t", "/tmp", 1);
        store.push("t", "abc");
        let read = store.read_from("t", 99).unwrap();
        assert!(read.data.is_empty());
        assert_eq!(read.seq, 3);
    }

    #[test]
    fn remove_and_ids() {
        let store = TerminalStore::new();
        store.create("b", "/tmp", 1);
        store.create("a", "/tmp", 1);
        assert_eq!(store.ids(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(store.len(), 2);
        store.remove("a");
        assert_eq!(store.ids(), vec!["b".to_string()]);
        assert!(store.read_from("a", 0).is_none());
    }

    #[test]
    fn clones_share_buffers_and_compare_equal() {
        let store = TerminalStore::new();
        let clone = store.clone();
        clone.push("t", "shared");
        assert_eq!(store.read_from("t", 0).unwrap().data, "shared");
        assert_eq!(store, clone);
        assert_ne!(store, TerminalStore::new());
        assert!(Arc::ptr_eq(&store.inner, &clone.inner));
    }
}

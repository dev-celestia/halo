use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use halo_core::fs_secure;

pub const MAX_BODY_SIZE: usize = 2 * 1024 * 1024; // 2 MB truncation cap
pub const DEFAULT_EPHEMERAL_MAX_BYTES: u64 = 64 * 1024 * 1024; // 64 MB RAM cap
pub const MAX_SEGMENT_SIZE: u64 = 32 * 1024 * 1024; // 32 MB per segment file
pub const MAX_SESSION_SEGMENTS: usize = 8; // Max 8 segments (256 MB) per session
pub const DEFAULT_PERSISTENT_MAX_BYTES: u64 = 512 * 1024 * 1024; // 512 MB global disk cap

/// Opaque reference to a stored payload.
/// Format:
///   - Ephemeral:  "slab:<id>"
///   - Persistent: "seg:<session_id>:<segment_id>:<offset>:<length>"
///   - None/Empty: ""
pub type PayloadRef = String;

pub trait PayloadBackend: Send + Sync {
    fn store(&self, session_id: &str, data: &[u8]) -> Result<PayloadRef, String>;
    fn load(&self, reference: &str) -> Result<Option<Vec<u8>>, String>;
    fn remove(&self, reference: &str) -> Result<(), String>;
    fn total_bytes(&self) -> u64;
    fn evict_to(&self, target_bytes: u64) -> Result<usize, String>;
    fn close(&self) -> Result<(), String>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Ephemeral Slab Store (RAM)
// ─────────────────────────────────────────────────────────────────────────────

pub struct EphemeralSlab {
    slabs: RwLock<HashMap<u64, Vec<u8>>>,
    insertion_order: Mutex<VecDeque<u64>>,
    next_id: AtomicU64,
    total_bytes: AtomicU64,
    max_bytes: u64,
}

impl EphemeralSlab {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            slabs: RwLock::new(HashMap::new()),
            insertion_order: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            total_bytes: AtomicU64::new(0),
            max_bytes,
        }
    }

    pub fn clear(&self) {
        let mut slabs = self.slabs.write();
        slabs.clear();
        let mut order = self.insertion_order.lock();
        order.clear();
        self.total_bytes.store(0, Ordering::Relaxed);
    }
}

impl Default for EphemeralSlab {
    fn default() -> Self {
        Self::new(DEFAULT_EPHEMERAL_MAX_BYTES)
    }
}

impl PayloadBackend for EphemeralSlab {
    fn store(&self, _session_id: &str, data: &[u8]) -> Result<PayloadRef, String> {
        let size = data.len() as u64;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Evict older slabs if we exceed memory cap
        let current = self.total_bytes.load(Ordering::Relaxed);
        if current + size > self.max_bytes {
            let target = self.max_bytes.saturating_sub(size);
            let _ = self.evict_to(target);
        }

        self.slabs.write().insert(id, data.to_vec());
        self.insertion_order.lock().push_back(id);
        self.total_bytes.fetch_add(size, Ordering::Relaxed);

        Ok(format!("slab:{id}"))
    }

    fn load(&self, reference: &str) -> Result<Option<Vec<u8>>, String> {
        let id = parse_slab_id(reference)?;
        let slabs = self.slabs.read();
        Ok(slabs.get(&id).cloned())
    }

    fn remove(&self, reference: &str) -> Result<(), String> {
        let id = parse_slab_id(reference)?;
        let mut slabs = self.slabs.write();
        if let Some(data) = slabs.remove(&id) {
            self.total_bytes
                .fetch_sub(data.len() as u64, Ordering::Relaxed);
            self.insertion_order
                .lock()
                .retain(|&existing| existing != id);
        }
        Ok(())
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }

    fn evict_to(&self, target_bytes: u64) -> Result<usize, String> {
        let mut evicted_count = 0;
        let mut order = self.insertion_order.lock();
        let mut slabs = self.slabs.write();

        while self.total_bytes.load(Ordering::Relaxed) > target_bytes && !order.is_empty() {
            let oldest_id = order.pop_front().expect("order is non-empty");
            if let Some(data) = slabs.remove(&oldest_id) {
                self.total_bytes
                    .fetch_sub(data.len() as u64, Ordering::Relaxed);
                evicted_count += 1;
            }
        }

        Ok(evicted_count)
    }

    fn close(&self) -> Result<(), String> {
        self.clear();
        Ok(())
    }
}

fn parse_slab_id(reference: &str) -> Result<u64, String> {
    if let Some(id_str) = reference.strip_prefix("slab:") {
        id_str
            .parse::<u64>()
            .map_err(|e| format!("Invalid slab id '{id_str}': {e}"))
    } else {
        Err(format!("Reference '{reference}' is not a slab reference"))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Persistent Segment Store (Disk)
// ─────────────────────────────────────────────────────────────────────────────

struct SegmentFile {
    id: u64,
    file: File,
    offset: u64,
}

struct SessionSegmentWriter {
    dir: PathBuf,
    current_segment: Mutex<SegmentFile>,
    segment_counter: AtomicU64,
}

impl SessionSegmentWriter {
    fn open_or_create(dir: PathBuf) -> Result<Self, String> {
        fs_secure::create_private_dir(&dir)
            .map_err(|e| format!("Failed to create session dir {dir:?}: {e}"))?;

        // Find existing segments to determine next ID & offset
        let mut max_id = 0u64;
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("seg_") && name.ends_with(".bin") {
                    let num_part = &name[4..name.len() - 4];
                    if let Ok(id) = num_part.parse::<u64>() {
                        if id > max_id {
                            max_id = id;
                        }
                    }
                }
            }
        }

        let seg_id = if max_id == 0 { 1 } else { max_id };
        let seg_path = dir.join(format!("seg_{seg_id:04}.bin"));
        let file = fs_secure::open_private_append(&seg_path)
            .map_err(|e| format!("Failed to open segment file {seg_path:?}: {e}"))?;

        let offset = file.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            dir,
            current_segment: Mutex::new(SegmentFile {
                id: seg_id,
                file,
                offset,
            }),
            segment_counter: AtomicU64::new(seg_id),
        })
    }

    fn write_payload(&self, data: &[u8]) -> Result<(u64, u64, usize), String> {
        let mut seg = self.current_segment.lock();

        // Check for segment size rollover
        if seg.offset + (data.len() as u64) > MAX_SEGMENT_SIZE && seg.offset > 0 {
            let next_id = self.segment_counter.fetch_add(1, Ordering::Relaxed) + 1;
            let next_path = self.dir.join(format!("seg_{next_id:04}.bin"));
            let new_file = fs_secure::open_private_append(&next_path)
                .map_err(|e| format!("Failed to open next segment file {next_path:?}: {e}"))?;

            *seg = SegmentFile {
                id: next_id,
                file: new_file,
                offset: 0,
            };

            // Enforce per-session segment cap by removing oldest segments beyond MAX_SESSION_SEGMENTS
            self.prune_excess_segments(next_id);
        }

        let offset = seg.offset;
        seg.file
            .write_all(data)
            .map_err(|e| format!("Failed to write segment data: {e}"))?;
        seg.offset += data.len() as u64;

        Ok((seg.id, offset, data.len()))
    }

    fn prune_excess_segments(&self, active_id: u64) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut segs: Vec<(u64, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("seg_") && name.ends_with(".bin") {
                    let id = name[4..name.len() - 4].parse::<u64>().ok()?;
                    if id < active_id {
                        return Some((id, entry.path()));
                    }
                }
                None
            })
            .collect();
        // Keep at most MAX_SESSION_SEGMENTS - 1 closed segments plus the active one.
        let max_closed = MAX_SESSION_SEGMENTS.saturating_sub(1);
        if segs.len() > max_closed {
            segs.sort_by_key(|(id, _)| *id);
            let remove_count = segs.len() - max_closed;
            for (_, path) in segs.into_iter().take(remove_count) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

pub struct DiskSegmentStore {
    sessions_dir: PathBuf,
    writers: RwLock<HashMap<String, Arc<SessionSegmentWriter>>>,
}

impl DiskSegmentStore {
    pub fn new(sessions_dir: PathBuf) -> Self {
        let _ = fs_secure::create_private_dir(&sessions_dir);
        Self {
            sessions_dir,
            writers: RwLock::new(HashMap::new()),
        }
    }

    fn get_or_create_writer(&self, session_id: &str) -> Result<Arc<SessionSegmentWriter>, String> {
        {
            let readers = self.writers.read();
            if let Some(writer) = readers.get(session_id) {
                return Ok(Arc::clone(writer));
            }
        }

        let mut writers = self.writers.write();
        if let Some(writer) = writers.get(session_id) {
            return Ok(Arc::clone(writer));
        }

        let sid = safe_session_component(session_id)?;
        let session_path = self.sessions_dir.join(sid);
        let writer = Arc::new(SessionSegmentWriter::open_or_create(session_path)?);
        writers.insert(session_id.to_string(), Arc::clone(&writer));
        Ok(writer)
    }

    pub fn remove_session(&self, session_id: &str) -> Result<(), String> {
        let mut writers = self.writers.write();
        writers.remove(session_id);
        let sid = safe_session_component(session_id)?;
        let path = self.sessions_dir.join(sid);
        if path.exists() {
            let _ = fs::remove_dir_all(path);
        }
        Ok(())
    }

    pub fn clear_all(&self) -> Result<u64, String> {
        let mut writers = self.writers.write();
        writers.clear();
        let bytes_freed = compute_dir_size(&self.sessions_dir).unwrap_or(0);
        if self.sessions_dir.exists() {
            let _ = fs::remove_dir_all(&self.sessions_dir);
            let _ = fs_secure::create_private_dir(&self.sessions_dir);
        }
        Ok(bytes_freed)
    }

    pub fn total_disk_size_bytes(&self) -> u64 {
        compute_dir_size(&self.sessions_dir).unwrap_or(0)
    }

    /// Remove segment files for `session_id` whose segment ID is strictly less than `min_keep_seg_id`.
    pub fn prune_session_segments_before(&self, session_id: &str, min_keep_seg_id: u64) -> usize {
        let Ok(sid) = safe_session_component(session_id) else {
            return 0;
        };
        let session_dir = self.sessions_dir.join(sid);
        let active_id = self
            .writers
            .read()
            .get(session_id)
            .map(|w| w.current_segment.lock().id);

        let Ok(entries) = fs::read_dir(&session_dir) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("seg_") && name.ends_with(".bin") {
                if let Ok(seg_id) = name[4..name.len() - 4].parse::<u64>() {
                    if seg_id < min_keep_seg_id && Some(seg_id) != active_id {
                        if fs::remove_file(entry.path()).is_ok() {
                            removed += 1;
                        }
                    }
                }
            }
        }
        removed
    }
}

impl PayloadBackend for DiskSegmentStore {
    fn store(&self, session_id: &str, data: &[u8]) -> Result<PayloadRef, String> {
        let writer = self.get_or_create_writer(session_id)?;
        let (seg_id, offset, len) = writer.write_payload(data)?;
        let sid = safe_session_component(session_id)?;
        Ok(format!("seg:{sid}:{seg_id}:{offset}:{len}"))
    }

    fn load(&self, reference: &str) -> Result<Option<Vec<u8>>, String> {
        let (session_id, seg_id, offset, length) = parse_seg_ref(reference)?;
        let path = self
            .sessions_dir
            .join(safe_session_component(session_id)?)
            .join(format!("seg_{seg_id:04}.bin"));

        if !path.exists() {
            return Ok(None);
        }

        let mut file =
            File::open(&path).map_err(|e| format!("Failed to open segment file {path:?}: {e}"))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Failed to seek in segment file {path:?}: {e}"))?;

        let mut buf = vec![0u8; length];
        file.read_exact(&mut buf)
            .map_err(|e| format!("Failed to read segment payload: {e}"))?;

        Ok(Some(buf))
    }

    fn remove(&self, _reference: &str) -> Result<(), String> {
        // Disk segment store is append-only; per-item deletion is handled by segment rollover/pruning
        Ok(())
    }

    fn total_bytes(&self) -> u64 {
        // Compute total size of all session dirs
        compute_dir_size(&self.sessions_dir).unwrap_or(0)
    }

    fn evict_to(&self, target_bytes: u64) -> Result<usize, String> {
        let mut current_bytes = self.total_bytes();
        if current_bytes <= target_bytes {
            return Ok(0);
        }

        // Collect all non-active segment files across sessions and sort oldest first by mtime/seg_id.
        let mut candidates: Vec<(std::time::SystemTime, u64, PathBuf, u64)> = Vec::new();
        let active_segments: HashMap<String, u64> = self
            .writers
            .read()
            .iter()
            .map(|(sid, w)| (sid.clone(), w.current_segment.lock().id))
            .collect();

        if let Ok(sessions) = fs::read_dir(&self.sessions_dir) {
            for session_entry in sessions.flatten() {
                let session_path = session_entry.path();
                if !session_path.is_dir() {
                    continue;
                }
                let sid = session_entry.file_name().to_string_lossy().to_string();
                let active_id = active_segments.get(&sid).copied();
                if let Ok(files) = fs::read_dir(&session_path) {
                    for file_entry in files.flatten() {
                        let name = file_entry.file_name().to_string_lossy().to_string();
                        if name.starts_with("seg_") && name.ends_with(".bin") {
                            let Ok(seg_id) = name[4..name.len() - 4].parse::<u64>() else {
                                continue;
                            };
                            if Some(seg_id) == active_id {
                                continue;
                            }
                            if let Ok(meta) = file_entry.metadata() {
                                let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                                candidates.push((mtime, seg_id, file_entry.path(), meta.len()));
                            }
                        }
                    }
                }
            }
        }

        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut evicted = 0;
        for (_, _, path, len) in candidates {
            if current_bytes <= target_bytes {
                break;
            }
            if fs::remove_file(path).is_ok() {
                current_bytes = current_bytes.saturating_sub(len);
                evicted += 1;
            }
        }
        Ok(evicted)
    }

    fn close(&self) -> Result<(), String> {
        let mut writers = self.writers.write();
        writers.clear();
        Ok(())
    }
}

/// Validate a session id before it is used as a path component.
///
/// Session ids are UUIDs (or the literal `default`), but `remove_session` and
/// `load` accept them from stored references. A crafted or imported row with
/// `sid = "../../.."` would otherwise escape the sessions directory and let a
/// delete or read reach arbitrary paths.
fn safe_session_component(session_id: &str) -> Result<&str, String> {
    let sid = if session_id.is_empty() {
        "default"
    } else {
        session_id
    };

    let valid = sid.len() <= 128
        && sid != "."
        && sid != ".."
        && sid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');

    if valid {
        Ok(sid)
    } else {
        Err(format!("Invalid session id for payload storage: {sid:?}"))
    }
}

fn parse_seg_ref(reference: &str) -> Result<(&str, u64, u64, usize), String> {
    if let Some(rest) = reference.strip_prefix("seg:") {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() != 4 {
            return Err(format!(
                "Invalid segment reference '{reference}': expected 4 parts"
            ));
        }
        let session_id = parts[0];
        let seg_id = parts[1]
            .parse::<u64>()
            .map_err(|e| format!("Invalid seg_id in '{reference}': {e}"))?;
        let offset = parts[2]
            .parse::<u64>()
            .map_err(|e| format!("Invalid offset in '{reference}': {e}"))?;
        let length = parts[3]
            .parse::<usize>()
            .map_err(|e| format!("Invalid length in '{reference}': {e}"))?;

        Ok((session_id, seg_id, offset, length))
    } else {
        Err(format!(
            "Reference '{reference}' is not a segment reference"
        ))
    }
}

fn compute_dir_size(path: &PathBuf) -> std::io::Result<u64> {
    let mut total = 0;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                total += compute_dir_size(&p)?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unified PayloadStore Facade
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PayloadStore {
    ephemeral: Arc<EphemeralSlab>,
    persistent: Arc<DiskSegmentStore>,
}

impl PayloadStore {
    pub fn new(sessions_dir: PathBuf) -> Self {
        Self {
            ephemeral: Arc::new(EphemeralSlab::default()),
            persistent: Arc::new(DiskSegmentStore::new(sessions_dir)),
        }
    }

    pub fn with_ephemeral_limit(sessions_dir: PathBuf, ephemeral_max_bytes: u64) -> Self {
        Self {
            ephemeral: Arc::new(EphemeralSlab::new(ephemeral_max_bytes)),
            persistent: Arc::new(DiskSegmentStore::new(sessions_dir)),
        }
    }

    pub fn ephemeral(&self) -> &EphemeralSlab {
        &self.ephemeral
    }

    pub fn persistent(&self) -> &DiskSegmentStore {
        &self.persistent
    }

    /// Store a body buffer. Truncates if > MAX_BODY_SIZE.
    /// Returns (payload_reference, original_body_size, was_truncated).
    pub fn store_body(
        &self,
        session_id: &str,
        data: &[u8],
        storage_mode: &str,
    ) -> (PayloadRef, usize, bool) {
        if data.is_empty() {
            return (String::new(), 0, false);
        }

        let original_size = data.len();
        let is_truncated = original_size > MAX_BODY_SIZE;
        let slice = if is_truncated {
            &data[..MAX_BODY_SIZE]
        } else {
            data
        };

        let res = if storage_mode == "ephemeral" {
            self.ephemeral.store(session_id, slice)
        } else {
            self.persistent.store(session_id, slice)
        };

        match res {
            Ok(reference) => (reference, original_size, is_truncated),
            Err(err) => {
                eprintln!("[PayloadStore] Store failed: {err}");
                (String::new(), original_size, is_truncated)
            }
        }
    }

    /// Load payload data by reference string ("slab:..." or "seg:...").
    pub fn load_body(&self, reference: &str) -> Result<Option<Vec<u8>>, String> {
        if reference.is_empty() {
            return Ok(None);
        }

        if reference.starts_with("slab:") {
            self.ephemeral.load(reference)
        } else if reference.starts_with("seg:") {
            self.persistent.load(reference)
        } else {
            Err(format!("Unknown payload reference scheme in '{reference}'"))
        }
    }

    /// Remove a single payload reference if applicable.
    pub fn remove_body(&self, reference: &str) -> Result<(), String> {
        if reference.starts_with("slab:") {
            self.ephemeral.remove(reference)
        } else if reference.starts_with("seg:") {
            self.persistent.remove(reference)
        } else {
            Ok(())
        }
    }

    /// Clear all ephemeral payloads (called on session reset / app restart).
    pub fn clear_ephemeral(&self) {
        self.ephemeral.clear();
    }

    /// Remove disk files for a deleted session.
    pub fn remove_session(&self, session_id: &str) -> Result<(), String> {
        self.persistent.remove_session(session_id)
    }

    /// Clear all persistent session segments on disk. Returns bytes freed.
    pub fn clear_all_persistent(&self) -> Result<u64, String> {
        self.persistent.clear_all()
    }

    /// Total bytes used by disk segments.
    pub fn total_disk_size_bytes(&self) -> u64 {
        self.persistent.total_disk_size_bytes()
    }

    /// Prune segment files for `session_id` with segment IDs `< min_keep_seg_id`,
    /// and enforce the global persistent disk budget.
    pub fn prune_session_segments_before(&self, session_id: &str, min_keep_seg_id: u64) -> usize {
        let removed = self
            .persistent
            .prune_session_segments_before(session_id, min_keep_seg_id);
        let _ = self.persistent.evict_to(DEFAULT_PERSISTENT_MAX_BYTES);
        removed
    }

    /// Enforce global persistent disk budget across all sessions.
    pub fn enforce_persistent_budget(&self) -> usize {
        self.persistent
            .evict_to(DEFAULT_PERSISTENT_MAX_BYTES)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ephemeral_slab_basic_store_and_load() {
        let slab = EphemeralSlab::new(1024 * 1024);
        let data = b"Hello, Ephemeral World!";
        let r = slab.store("test_session", data).unwrap();
        assert!(r.starts_with("slab:"));

        let loaded = slab.load(&r).unwrap();
        assert_eq!(loaded.as_deref(), Some(&data[..]));

        slab.remove(&r).unwrap();
        let after_remove = slab.load(&r).unwrap();
        assert!(after_remove.is_none());
    }

    #[test]
    fn test_ephemeral_slab_fifo_eviction() {
        // Max 100 bytes
        let slab = EphemeralSlab::new(100);

        let data1 = vec![1u8; 40];
        let data2 = vec![2u8; 40];
        let data3 = vec![3u8; 40];

        let r1 = slab.store("s", &data1).unwrap();
        let r2 = slab.store("s", &data2).unwrap();
        assert_eq!(slab.total_bytes(), 80);

        // Storing 40 more bytes exceeds 100 bytes -> r1 should be evicted
        let r3 = slab.store("s", &data3).unwrap();
        assert!(slab.total_bytes() <= 100);

        assert!(slab.load(&r1).unwrap().is_none());
        assert_eq!(slab.load(&r2).unwrap().as_deref(), Some(&data2[..]));
        assert_eq!(slab.load(&r3).unwrap().as_deref(), Some(&data3[..]));
    }

    #[test]
    fn test_disk_segment_store_and_load() {
        let temp = tempdir().unwrap();
        let store = DiskSegmentStore::new(temp.path().to_path_buf());

        let data1 = b"Segment Payload Alpha";
        let data2 = b"Segment Payload Beta 123456789";

        let r1 = store.store("sess_1", data1).unwrap();
        let r2 = store.store("sess_1", data2).unwrap();

        assert!(r1.starts_with("seg:sess_1:1:0:"));
        assert!(r2.starts_with("seg:sess_1:1:"));

        assert_eq!(store.load(&r1).unwrap().as_deref(), Some(&data1[..]));
        assert_eq!(store.load(&r2).unwrap().as_deref(), Some(&data2[..]));

        // Session removal
        store.remove_session("sess_1").unwrap();
        assert!(store.load(&r1).unwrap().is_none());
    }

    #[test]
    fn test_payload_store_truncation() {
        let temp = tempdir().unwrap();
        let store = PayloadStore::new(temp.path().to_path_buf());

        // Body smaller than cap
        let small = vec![42u8; 1024];
        let (ref1, size1, trunc1) = store.store_body("s", &small, "ephemeral");
        assert_eq!(size1, 1024);
        assert!(!trunc1);
        assert_eq!(store.load_body(&ref1).unwrap().unwrap().len(), 1024);

        // Body larger than MAX_BODY_SIZE (4 MB)
        let large = vec![99u8; MAX_BODY_SIZE + 500];
        let (ref2, size2, trunc2) = store.store_body("s", &large, "ephemeral");
        assert_eq!(size2, MAX_BODY_SIZE + 500);
        assert!(trunc2);
        assert_eq!(
            store.load_body(&ref2).unwrap().unwrap().len(),
            MAX_BODY_SIZE
        );
    }

    #[test]
    fn test_parse_slab_id_valid_and_invalid() {
        assert_eq!(parse_slab_id("slab:42").unwrap(), 42);
        assert_eq!(parse_slab_id("slab:0").unwrap(), 0);

        assert!(
            parse_slab_id("42")
                .unwrap_err()
                .contains("is not a slab reference")
        );
        assert!(
            parse_slab_id("seg:1:2:3:4")
                .unwrap_err()
                .contains("is not a slab reference")
        );
        assert!(
            parse_slab_id("slab:abc")
                .unwrap_err()
                .contains("Invalid slab id 'abc'")
        );
        assert!(parse_slab_id("slab:").is_err());
    }

    #[test]
    fn test_parse_seg_ref_valid_and_invalid() {
        let (session, seg_id, offset, length) = parse_seg_ref("seg:sess_9:3:1024:64").unwrap();
        assert_eq!(session, "sess_9");
        assert_eq!(seg_id, 3);
        assert_eq!(offset, 1024);
        assert_eq!(length, 64);

        assert!(
            parse_seg_ref("slab:1")
                .unwrap_err()
                .contains("is not a segment reference")
        );
        assert!(
            parse_seg_ref("seg:only:two")
                .unwrap_err()
                .contains("expected 4 parts")
        );
        assert!(
            parse_seg_ref("seg:s:1:1:x")
                .unwrap_err()
                .contains("Invalid length")
        );
        assert!(
            parse_seg_ref("seg:s:x:1:1")
                .unwrap_err()
                .contains("Invalid seg_id")
        );
        assert!(
            parse_seg_ref("seg:s:1:x:1")
                .unwrap_err()
                .contains("Invalid offset")
        );
    }
}

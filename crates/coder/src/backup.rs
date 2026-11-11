//! Hot-exit backups and local history for the Coder IDE.
//!
//! Dirty editor buffers are written under the shared app data dir (never into
//! the project tree), so a restart can restore unsaved work the way VS Code
//! does. Explicit saves also push the previous on-disk text into local history.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const BACKUPS_DIR: &str = "coder-backups";
const HISTORY_DIR: &str = "coder-history";
const MAX_HISTORY: usize = 50;
/// Skip storing local-history snapshots for files larger than 512 KiB.
const MAX_HISTORY_ENTRY_BYTES: usize = 512 * 1024;
/// Maximum total bytes retained across snapshots for a single file (4 MiB).
const MAX_FILE_HISTORY_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupMeta {
    pub path: String,
    /// Unix seconds of the on-disk file when the backup was written.
    pub disk_mtime_secs: Option<u64>,
    pub disk_len: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupRestore {
    pub content: String,
    pub meta: BackupMeta,
    /// True when the on-disk file's mtime/len no longer match `meta`.
    pub disk_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: String,
    /// Unix seconds of the snapshot.
    pub saved_at_secs: u64,
}

fn hash16(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    hex::encode(&digest[..8])
}

#[cfg(test)]
thread_local! {
    static TEST_APP_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

fn app_dir() -> PathBuf {
    #[cfg(test)]
    {
        if let Some(dir) = TEST_APP_DIR.with(|c| c.borrow().clone()) {
            return dir;
        }
    }
    halo_core::paths::get_shared_app_dir()
}

fn backups_root() -> PathBuf {
    app_dir().join(BACKUPS_DIR)
}

fn history_root() -> PathBuf {
    app_dir().join(HISTORY_DIR)
}

fn workspace_dir(workspace: &str) -> PathBuf {
    backups_root().join(hash16(workspace))
}

fn backup_stem(workspace: &str, path: &str) -> PathBuf {
    workspace_dir(workspace).join(hash16(path))
}

fn backup_txt_path(workspace: &str, path: &str) -> PathBuf {
    backup_stem(workspace, path).with_extension("txt")
}

fn backup_meta_path(workspace: &str, path: &str) -> PathBuf {
    backup_stem(workspace, path).with_extension("meta.json")
}

fn history_dir_for(path: &str) -> PathBuf {
    history_root().join(hash16(path))
}

fn disk_stat(path: &Path) -> (Option<u64>, Option<u64>) {
    let Ok(meta) = fs::metadata(path) else {
        return (None, None);
    };
    let len = Some(meta.len());
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    (mtime, len)
}

/// Write `bytes` via a sibling temp file, then rename into place.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Persist a dirty buffer for hot exit.
pub fn write_backup(workspace: &str, path: &str, content: &str) -> Result<(), String> {
    let (mtime, len) = disk_stat(Path::new(path));
    let meta = BackupMeta {
        path: path.to_string(),
        disk_mtime_secs: mtime,
        disk_len: len,
    };
    let meta_json = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;
    atomic_write(&backup_txt_path(workspace, path), content.as_bytes())?;
    atomic_write(&backup_meta_path(workspace, path), &meta_json)?;
    Ok(())
}

/// Remove the hot-exit backup for a path (after save or discard).
pub fn delete_backup(workspace: &str, path: &str) -> Result<(), String> {
    let txt = backup_txt_path(workspace, path);
    let meta = backup_meta_path(workspace, path);
    if txt.exists() {
        fs::remove_file(&txt).map_err(|e| e.to_string())?;
    }
    if meta.exists() {
        fs::remove_file(&meta).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Load a hot-exit backup if one exists and differs from `disk_content`.
pub fn load_backup(
    workspace: &str,
    path: &str,
    disk_content: &str,
) -> Result<Option<BackupRestore>, String> {
    let txt_path = backup_txt_path(workspace, path);
    if !txt_path.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(&txt_path).map_err(|e| e.to_string())?;
    if content == disk_content {
        // Stale backup matching disk — drop it.
        let _ = delete_backup(workspace, path);
        return Ok(None);
    }
    let meta = match fs::read(backup_meta_path(workspace, path)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or(BackupMeta {
            path: path.to_string(),
            disk_mtime_secs: None,
            disk_len: None,
        }),
        Err(_) => BackupMeta {
            path: path.to_string(),
            disk_mtime_secs: None,
            disk_len: None,
        },
    };
    let (now_mtime, now_len) = disk_stat(Path::new(path));
    let disk_changed = match (meta.disk_mtime_secs, meta.disk_len, now_mtime, now_len) {
        (Some(m0), Some(l0), Some(m1), Some(l1)) => m0 != m1 || l0 != l1,
        // Missing stats: treat as unchanged so we do not spam toasts.
        _ => false,
    };
    Ok(Some(BackupRestore {
        content,
        meta,
        disk_changed,
    }))
}

/// Push the previous on-disk text into local history (cap [`MAX_HISTORY`]).
pub fn push_history(path: &str, previous_content: &str) -> Result<(), String> {
    if previous_content.len() > MAX_HISTORY_ENTRY_BYTES {
        return Ok(());
    }
    let dir = history_dir_for(path);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Deduplicate against the most recent snapshot so repeated saves/autosaves
    // of identical baseline content never multiply files on disk.
    if let Ok(entries) = list_history_ids(&dir) {
        if let Some(newest) = entries.first() {
            let newest_path = dir.join(format!("{}.txt", newest.id));
            if fs::read_to_string(newest_path).ok().as_deref() == Some(previous_content) {
                return Ok(());
            }
        }
    }

    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // Zero-padded seq keeps lexicographic order when many saves share a ms.
    let mut seq = 0u32;
    let mut id = format!("{now_ms}-{seq:04}");
    while dir.join(format!("{id}.txt")).exists() {
        seq += 1;
        id = format!("{now_ms}-{seq:04}");
    }
    atomic_write(&dir.join(format!("{id}.txt")), previous_content.as_bytes())?;
    prune_history(&dir)?;
    Ok(())
}

fn history_sort_key(entry: &HistoryEntry) -> (u64, u64, u32) {
    // id is `{millis}-{seq:04}` (legacy: `{secs}` or `{secs}-{n}`).
    let mut parts = entry.id.split('-');
    let stamp = parts
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let seq = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    // Normalize legacy second-precision stamps so they sort before ms stamps
    // from the same era only by their numeric value as stored.
    let millis = if stamp < 10_000_000_000 {
        stamp.saturating_mul(1000)
    } else {
        stamp
    };
    (millis, entry.saved_at_secs, seq)
}

fn prune_history(dir: &Path) -> Result<(), String> {
    let mut entries = list_history_ids(dir)?;
    // Newest first so we can keep a running byte budget and drop the tail.
    entries.sort_by(|a, b| history_sort_key(b).cmp(&history_sort_key(a)));
    let mut total_bytes = 0u64;
    for (idx, entry) in entries.into_iter().enumerate() {
        let path = dir.join(format!("{}.txt", entry.id));
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        total_bytes = total_bytes.saturating_add(len);
        if idx >= MAX_HISTORY || (idx > 0 && total_bytes > MAX_FILE_HISTORY_BYTES) {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

fn list_history_ids(dir: &Path) -> Result<Vec<HistoryEntry>, String> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(id) = name.strip_suffix(".txt") else {
            continue;
        };
        let stamp = id
            .split('-')
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let saved_at_secs = if stamp < 10_000_000_000 {
            stamp
        } else {
            stamp / 1000
        };
        out.push(HistoryEntry {
            id: id.to_string(),
            saved_at_secs,
        });
    }
    out.sort_by(|a, b| history_sort_key(b).cmp(&history_sort_key(a)));
    Ok(out)
}

/// Newest-first history entries for a file path.
pub fn list_history(path: &str) -> Result<Vec<HistoryEntry>, String> {
    list_history_ids(&history_dir_for(path))
}

/// Read one history snapshot by id.
pub fn read_history(path: &str, id: &str) -> Result<String, String> {
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err("Invalid history id".into());
    }
    let file = history_dir_for(path).join(format!("{id}.txt"));
    if !file.is_file() {
        return Err(format!("History entry not found: {id}"));
    }
    fs::read_to_string(file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialise tests that touch the shared app dir.
    static LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_app_dir<R>(f: impl FnOnce(&Path) -> R) -> R {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        TEST_APP_DIR.with(|c| *c.borrow_mut() = Some(dir.path().to_path_buf()));
        let result = f(dir.path());
        TEST_APP_DIR.with(|c| *c.borrow_mut() = None);
        result
    }

    #[test]
    fn round_trip_dirty_backup() {
        with_temp_app_dir(|_| {
            let workspace = "/tmp/proj-a";
            let path = "/tmp/proj-a/src/main.rs";
            write_backup(workspace, path, "fn main() {}\n").unwrap();
            let restored = load_backup(workspace, path, "fn main() { }\n")
                .unwrap()
                .expect("backup present");
            assert_eq!(restored.content, "fn main() {}\n");
            assert_eq!(restored.meta.path, path);
            delete_backup(workspace, path).unwrap();
            assert!(load_backup(workspace, path, "x").unwrap().is_none());
        });
    }

    #[test]
    fn matching_backup_is_dropped() {
        with_temp_app_dir(|_| {
            let workspace = "/tmp/proj-b";
            let path = "/tmp/proj-b/a.txt";
            write_backup(workspace, path, "same").unwrap();
            assert!(load_backup(workspace, path, "same").unwrap().is_none());
            assert!(!backup_txt_path(workspace, path).exists());
        });
    }

    #[test]
    fn atomic_replace_keeps_previous_on_abandoned_tmp() {
        with_temp_app_dir(|_| {
            let workspace = "/tmp/proj-c";
            let path = "/tmp/proj-c/b.txt";
            write_backup(workspace, path, "first").unwrap();
            let txt = backup_txt_path(workspace, path);
            // Simulate a crash that left a sibling .tmp-* file.
            let orphan = txt.with_extension("tmp-99999");
            fs::write(&orphan, b"partial").unwrap();
            // Previous content must still load.
            let restored = load_backup(workspace, path, "disk")
                .unwrap()
                .expect("backup");
            assert_eq!(restored.content, "first");
            // Orphan tmp is ignored by load.
            let _ = fs::remove_file(orphan);
        });
    }

    #[test]
    fn history_caps_at_fifty() {
        with_temp_app_dir(|_| {
            let path = "/tmp/proj-d/c.txt";
            for i in 0..55 {
                // Distinct content so we can tell which survived.
                push_history(path, &format!("snap-{i}")).unwrap();
                // Tiny delay via unique ids is handled by -n suffix in same second.
            }
            let entries = list_history(path).unwrap();
            assert_eq!(entries.len(), MAX_HISTORY);
            // Newest kept.
            let newest = read_history(path, &entries[0].id).unwrap();
            assert_eq!(newest, "snap-54");
            // Oldest of the retained window.
            let oldest = read_history(path, &entries[entries.len() - 1].id).unwrap();
            assert_eq!(oldest, "snap-5");
        });
    }

    #[test]
    fn restore_prefers_backup_over_disk() {
        with_temp_app_dir(|_| {
            let workspace = "/tmp/proj-e";
            let path = "/tmp/proj-e/d.txt";
            write_backup(workspace, path, "unsaved edits").unwrap();
            let restored = load_backup(workspace, path, "saved on disk")
                .unwrap()
                .expect("backup");
            assert_eq!(restored.content, "unsaved edits");
            assert_ne!(restored.content, "saved on disk");
        });
    }
}

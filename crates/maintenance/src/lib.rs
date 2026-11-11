//! Disk usage observability and local data wipe protocols.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use halo_core::fs_secure;
use halo_core::paths;
use halo_storage::HistoryBridge;

/// Log file location.
///
/// This used to be `/tmp/halo.log`: a predictable path in a shared,
/// world-writable directory. Any local user could pre-create it as a symlink
/// to a file the victim owns, and the "clear log" action would then truncate
/// that file; anything written there was readable by other users anyway. It
/// now lives under the private app dir with `0600`.
fn log_file_path() -> PathBuf {
    let dir = paths::get_shared_app_dir().join("logs");
    let _ = fs_secure::create_private_dir(&dir);
    dir.join("halo.log")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub app_data_dir: String,
    pub database_path: String,
    pub browser_artifacts_path: String,
    pub crawler_artifacts_path: String,
    pub database_size_bytes: u64,
    pub browser_artifacts_size_bytes: u64,
    pub crawler_artifacts_size_bytes: u64,
    pub regression_artifacts_size_bytes: u64,
    pub log_file_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataResult {
    pub artifact_dir: String,
    pub files_deleted: u64,
    pub bytes_deleted: u64,
    pub pages_updated: usize,
    pub intercept_browser_profile_removed: bool,
    pub ca_file_removed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteArtifactResult {
    pub bytes_deleted: u64,
    pub label: String,
}

pub fn get_storage_info(history: &HistoryBridge) -> Result<StorageInfo, String> {
    let app_data_dir = paths::get_shared_app_dir();
    let database_path = paths::get_shared_db_path();
    let crawler_artifacts_path = {
        let p = app_data_dir.join("crawler-artifacts");
        if p.exists() {
            p
        } else {
            app_data_dir.join("ai-browser-artifacts")
        }
    };
    let intercept_profile_path = app_data_dir.join("intercept-browser-profile");
    let regression_artifacts_path = app_data_dir.join("regression-artifacts");

    let sqlite_size: u64 = [
        database_path.clone(),
        database_path.with_extension("db-wal"),
        database_path.with_extension("db-shm"),
    ]
    .iter()
    .filter_map(|p| fs::metadata(p).ok())
    .map(|m| m.len())
    .sum();

    let segments_size = history
        .payload_store()
        .map(|ps| ps.total_disk_size_bytes())
        .unwrap_or(0);
    let db_size = sqlite_size + segments_size;

    let (_, crawler_artifacts_size) = count_files(&crawler_artifacts_path).unwrap_or((0, 0));
    let (_, intercept_profile_size) = count_files(&intercept_profile_path).unwrap_or((0, 0));
    let crawler_total_size = crawler_artifacts_size.saturating_add(intercept_profile_size);
    let (_, regression_size) = count_files(&regression_artifacts_path).unwrap_or((0, 0));

    let log_size = fs::metadata(log_file_path()).map(|m| m.len()).unwrap_or(0);

    Ok(StorageInfo {
        app_data_dir: app_data_dir.display().to_string(),
        database_path: database_path.display().to_string(),
        browser_artifacts_path: crawler_artifacts_path.display().to_string(),
        crawler_artifacts_path: crawler_artifacts_path.display().to_string(),
        database_size_bytes: db_size,
        browser_artifacts_size_bytes: crawler_total_size,
        crawler_artifacts_size_bytes: crawler_total_size,
        regression_artifacts_size_bytes: regression_size,
        log_file_size_bytes: log_size,
    })
}

/// Startup housekeeping to keep production disk usage bounded across restarts.
///
/// Purges volatile browser caches, leftover `.npm-cache`, orphaned soft-delete
/// trash, legacy `sessions/` directory, and rotates `halo.log` if > 5 MiB.
pub fn prune_runtime_storage() {
    let app_data_dir = paths::get_shared_app_dir();

    // 1. Purge soft-delete trash (in-memory undo stack is empty on fresh boot).
    let trash_dir = app_data_dir.join("coder-trash");
    if trash_dir.exists() {
        let _ = fs::remove_dir_all(&trash_dir);
    }

    // 2. Purge leftover npm cache from Cursor SDK adapter installation.
    let npm_cache_dir = app_data_dir.join("adapters").join(".npm-cache");
    if npm_cache_dir.exists() {
        let _ = fs::remove_dir_all(&npm_cache_dir);
    }

    // 3. Remove legacy orphaned `sessions/` directory if present.
    let legacy_sessions = app_data_dir.join("sessions");
    if legacy_sessions.exists() {
        let _ = fs::remove_dir_all(&legacy_sessions);
    }

    // 4. Prune volatile Chrome caches in `intercept-browser-profile`.
    let intercept_profile = app_data_dir.join("intercept-browser-profile");
    if intercept_profile.exists() {
        intercept::prune_chrome_profile_caches(&intercept_profile);
    }

    // 5. Remove any leftover temporary `cdp-browser-profile-*` directories in temp_dir.
    if let Ok(entries) = fs::read_dir(std::env::temp_dir()) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("cdp-browser-profile-") {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
    }

    // 6. Cap `logs/halo.log` at 5 MiB so long-running production installs never bloat.
    const MAX_LOG_FILE_BYTES: u64 = 5 * 1024 * 1024;
    let log_path = log_file_path();
    if let Ok(meta) = fs::metadata(&log_path) {
        if meta.len() > MAX_LOG_FILE_BYTES {
            let _ = fs_secure::truncate_private(&log_path);
        }
    }
}

pub fn delete_storage_artifact(
    history: &HistoryBridge,
    artifact: &str,
) -> Result<DeleteArtifactResult, String> {
    let app_data_dir = paths::get_shared_app_dir();

    match artifact {
        "database" => {
            history.close_connection()?;

            let db_path = paths::get_shared_db_path();
            let mut bytes_deleted: u64 = 0;
            for p in [
                db_path.clone(),
                db_path.with_extension("db-wal"),
                db_path.with_extension("db-shm"),
            ] {
                if let Ok(meta) = fs::metadata(&p) {
                    bytes_deleted += meta.len();
                    let _ = fs::remove_file(&p);
                }
            }

            if let Some(ps) = history.payload_store() {
                if let Ok(seg_bytes) = ps.clear_all_persistent() {
                    bytes_deleted += seg_bytes;
                }
                ps.clear_ephemeral();
            }

            history.reopen_and_init()?;

            // Recreate a default session so the UI isn't empty after a wipe.
            let _ = history.create_http_session(
                "Default",
                Some("Auto-created session"),
                Some("all"),
                None,
                None,
                Some("persistent"),
            );

            Ok(DeleteArtifactResult {
                bytes_deleted,
                label: "SQL Database & Payloads".to_string(),
            })
        }
        "crawler_artifacts" | "browser_artifacts" => {
            let dir = app_data_dir.join("crawler-artifacts");
            let legacy_dir = app_data_dir.join("ai-browser-artifacts");
            let intercept_dir = app_data_dir.join("intercept-browser-profile");
            let (_, crawler_bytes) = count_files(&dir).unwrap_or((0, 0));
            let (_, legacy_bytes) = count_files(&legacy_dir).unwrap_or((0, 0));
            let (_, intercept_bytes) = count_files(&intercept_dir).unwrap_or((0, 0));
            let bytes_deleted = crawler_bytes
                .saturating_add(legacy_bytes)
                .saturating_add(intercept_bytes);
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
            }
            if legacy_dir.exists() {
                let _ = fs::remove_dir_all(&legacy_dir);
            }
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            if intercept_dir.exists() {
                intercept::prune_chrome_profile_caches(&intercept_dir);
            }
            let _ = history.clear_crawler_artifact_paths();
            Ok(DeleteArtifactResult {
                bytes_deleted,
                label: "Crawler Artifacts".to_string(),
            })
        }
        "regression_artifacts" => {
            let dir = app_data_dir.join("regression-artifacts");
            let (_, bytes_deleted) = count_files(&dir).unwrap_or((0, 0));
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
            }
            Ok(DeleteArtifactResult {
                bytes_deleted,
                label: "Regression Artifacts".to_string(),
            })
        }
        "log_file" => {
            let path = log_file_path();
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            // Truncate through O_NOFOLLOW + 0600 rather than `fs::write`, which
            // would follow a symlink planted at the path.
            let _ = fs_secure::truncate_private(&path);
            Ok(DeleteArtifactResult {
                bytes_deleted: size,
                label: "Log File".to_string(),
            })
        }
        _ => Err(format!("Unknown artifact: {artifact}")),
    }
}

pub fn reset_all_app_data(history: &HistoryBridge) -> Result<ResetLocalDataResult, String> {
    let _ = halo_traffic::stop();

    history.close_connection()?;

    let app_data_dir = paths::get_shared_app_dir();

    let db_path = paths::get_shared_db_path();
    if db_path.exists() {
        fs::remove_file(&db_path).map_err(|error| error.to_string())?;
    }
    let wal_path = db_path.with_extension("db-wal");
    if wal_path.exists() {
        let _ = fs::remove_file(&wal_path);
    }
    let shm_path = db_path.with_extension("db-shm");
    if shm_path.exists() {
        let _ = fs::remove_file(&shm_path);
    }

    if let Some(ps) = history.payload_store() {
        let _ = ps.clear_all_persistent();
        ps.clear_ephemeral();
    }
    let sessions_dir = app_data_dir.join("sessions");
    if sessions_dir.exists() {
        let _ = fs::remove_dir_all(&sessions_dir);
    }
    let payloads_dir = app_data_dir.join("payloads");
    if payloads_dir.exists() {
        let _ = fs::remove_dir_all(&payloads_dir);
        let _ = fs::create_dir_all(&payloads_dir);
    }
    for coder_dir in ["coder-trash", "coder-backups", "coder-history"] {
        let dir = app_data_dir.join(coder_dir);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    let artifact_dir = app_data_dir.join("crawler-artifacts");
    let legacy_artifact_dir = app_data_dir.join("ai-browser-artifacts");
    let intercept_browser_profile_dir = app_data_dir.join("intercept-browser-profile");
    let ca_dir = app_data_dir.join(".halo");
    let ca_path = app_data_dir.join("halo-ca.pem");
    let regression_artifact_dir = app_data_dir.join("regression-artifacts");
    let settings_files = [
        app_data_dir.join("ai-settings.json"),
        app_data_dir.join("r2-settings.json"),
    ];

    let (files_deleted, bytes_deleted) = count_files(&artifact_dir)?;
    if artifact_dir.exists() {
        fs::remove_dir_all(&artifact_dir).map_err(|error| error.to_string())?;
    }
    if legacy_artifact_dir.exists() {
        let _ = fs::remove_dir_all(&legacy_artifact_dir);
    }
    fs::create_dir_all(&artifact_dir).map_err(|error| error.to_string())?;

    let intercept_browser_profile_removed = intercept_browser_profile_dir.exists();
    if intercept_browser_profile_removed {
        fs::remove_dir_all(&intercept_browser_profile_dir).map_err(|error| error.to_string())?;
    }

    if ca_dir.exists() {
        fs::remove_dir_all(&ca_dir).map_err(|error| error.to_string())?;
    }

    let ca_file_removed = ca_path.exists();
    if ca_file_removed {
        fs::remove_file(&ca_path).map_err(|error| error.to_string())?;
    }

    if regression_artifact_dir.exists() {
        fs::remove_dir_all(&regression_artifact_dir).map_err(|error| error.to_string())?;
    }
    for settings_path in settings_files {
        if settings_path.exists() {
            let _ = fs::remove_file(&settings_path);
        }
    }

    history.reopen_and_init()?;
    let pages_updated = history.clear_crawler_artifact_paths().unwrap_or(0);

    let _ = history.create_http_session(
        "Default",
        Some("Auto-created session"),
        Some("all"),
        None,
        None,
        Some("persistent"),
    );

    // Ensure a fresh CA exists after wipe.
    halo_traffic::ca::ensure_ca_exists();

    Ok(ResetLocalDataResult {
        artifact_dir: artifact_dir.display().to_string(),
        files_deleted,
        bytes_deleted,
        pages_updated,
        intercept_browser_profile_removed,
        ca_file_removed,
    })
}

fn count_files(path: &Path) -> Result<(u64, u64), String> {
    if !path.exists() {
        return Ok((0, 0));
    }

    let mut files = 0u64;
    let mut bytes = 0u64;
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if metadata.is_dir() {
            let (child_files, child_bytes) = count_files(&entry.path())?;
            files += child_files;
            bytes += child_bytes;
        } else if metadata.is_file() {
            files += 1;
            bytes += metadata.len();
        }
    }

    Ok((files, bytes))
}

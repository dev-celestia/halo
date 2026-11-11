use std::path::{Path, PathBuf};
use std::sync::Once;

use crate::fs_secure;

/// Legacy migration runs once per process, before any writer touches the dir.
static MIGRATE_LEGACY_NAMES: Once = Once::new();

/// Returns the centralized, shared Halo data directory.
///
/// macOS: ~/Library/Application Support/com.halo
/// Windows: %APPDATA%/Halo
/// Linux: ~/.local/share/halo (or $XDG_DATA_HOME/halo)
///
/// The directory is created `0700`: it holds the CA private key and every
/// captured request/response, so it must not be traversable by other users.
///
/// Installs that ran as Hexbuffer are migrated in place on first access: the
/// old top-level directory is renamed, then the legacy file names inside it.
/// Renames keep the traffic database, payloads, and CA key material together,
/// so history and installed trust survive the rebrand.
pub fn get_shared_app_dir() -> PathBuf {
    let (path, legacy) = resolve_app_dir();

    MIGRATE_LEGACY_NAMES.call_once(|| {
        if let Some(legacy) = legacy.as_deref() {
            migrate_legacy_app_dir(legacy, &path);
        }
        migrate_legacy_file_names(&path);
    });

    let _ = fs_secure::create_private_dir(&path);
    path
}

fn resolve_app_dir() -> (PathBuf, Option<PathBuf>) {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let support = PathBuf::from(home).join("Library/Application Support");
            return (
                support.join("com.halo"),
                Some(support.join("com.hexbuffer")),
            );
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let root = PathBuf::from(appdata);
            return (root.join("Halo"), Some(root.join("Hexbuffer")));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            let root = PathBuf::from(xdg);
            return (root.join("halo"), Some(root.join("hexbuffer")));
        }
        if let Ok(home) = std::env::var("HOME") {
            let root = PathBuf::from(home).join(".local/share");
            return (root.join("halo"), Some(root.join("hexbuffer")));
        }
    }

    (PathBuf::from(".halo"), Some(PathBuf::from(".hexbuffer")))
}

/// Rename the pre-rebrand data directory, unless Halo already has one.
fn migrate_legacy_app_dir(legacy: &Path, app_dir: &Path) {
    if app_dir.exists() || !legacy.exists() {
        return;
    }
    let _ = std::fs::rename(legacy, app_dir);
}

/// Rename pre-rebrand files inside the app directory.
///
/// Idempotent: every rename is skipped when the Halo-named target exists, so
/// a half-finished migration or a user who already relaunched keeps working.
fn migrate_legacy_file_names(app_dir: &Path) {
    rename_if_absent(&app_dir.join("hexbuffer.db"), &app_dir.join("halo.db"));
    rename_if_absent(
        &app_dir.join("hexbuffer.db-wal"),
        &app_dir.join("halo.db-wal"),
    );
    rename_if_absent(
        &app_dir.join("hexbuffer.db-shm"),
        &app_dir.join("halo.db-shm"),
    );
    rename_if_absent(
        &app_dir.join("hexbuffer-ca.pem"),
        &app_dir.join("halo-ca.pem"),
    );
    rename_if_absent(&app_dir.join(".hexbuffer"), &app_dir.join(".halo"));
    rename_if_absent(
        &app_dir.join("logs").join("hexbuffer.log"),
        &app_dir.join("logs").join("halo.log"),
    );
}

fn rename_if_absent(from: &Path, to: &Path) {
    if !to.exists() && from.exists() {
        let _ = std::fs::rename(from, to);
    }
}

/// Returns the centralized database file path `halo.db`.
pub fn get_shared_db_path() -> PathBuf {
    let db_path = get_shared_app_dir().join("halo.db");

    // Seamless migration: If centralized DB doesn't exist yet, check if there is an
    // existing database from the pre-rebrand per-feature dirs
    // (com.hexbuffer.security / com.hexbuffer.http-history).
    if !db_path.exists() {
        #[cfg(target_os = "macos")]
        if let Ok(home) = std::env::var("HOME") {
            let support = PathBuf::from(&home).join("Library/Application Support");
            let candidates = [
                support.join("com.hexbuffer.security/hexbuffer.db"),
                support.join("com.hexbuffer.http-history/hexbuffer.db"),
            ];
            for candidate in candidates {
                if candidate.exists() {
                    let _ = std::fs::copy(&candidate, &db_path);
                    let _ = std::fs::copy(
                        candidate.with_extension("db-wal"),
                        db_path.with_extension("db-wal"),
                    );
                    let _ = std::fs::copy(
                        candidate.with_extension("db-shm"),
                        db_path.with_extension("db-shm"),
                    );
                    break;
                }
            }
        }
    }

    db_path
}

/// Every file under the app dir that holds captured traffic or key material.
fn sensitive_files(app_dir: &Path) -> Vec<PathBuf> {
    let db = app_dir.join("halo.db");
    vec![
        db.clone(),
        db.with_extension("db-wal"),
        db.with_extension("db-shm"),
        app_dir.join("halo-ca.pem"),
        app_dir.join(".halo/ca-key.pem"),
        app_dir.join("ai-credentials.json"),
        app_dir.join("cursor-state/auth.json"),
    ]
}

/// Re-assert `0700` on the app dir and `0600` on every sensitive file.
///
/// Called once at boot. This is what heals installs created before the
/// permissions were tightened: earlier releases wrote the CA private key and
/// the traffic database with the process umask (typically `0644`), so those
/// files stay world-readable until something chmods them.
pub fn harden_app_data_permissions() {
    let app_dir = get_shared_app_dir();
    fs_secure::set_mode(&app_dir, fs_secure::DIR_MODE);

    for dir in [
        app_dir.join(".halo"),
        app_dir.join("payloads"),
        app_dir.join("sessions"),
        app_dir.join("logs"),
        app_dir.join("cursor-state"),
    ] {
        if dir.exists() {
            fs_secure::set_mode(&dir, fs_secure::DIR_MODE);
        }
    }

    for file in sensitive_files(&app_dir) {
        fs_secure::harden_file_if_exists(&file);
    }

    // Clean up legacy `sessions` directory if present (payload segments now live in `payloads/`).
    let legacy_sessions = app_dir.join("sessions");
    if legacy_sessions.exists() {
        let _ = std::fs::remove_dir_all(&legacy_sessions);
    }

    // Payload segments: `payloads/<uuid>/seg_XXXX.bin`.
    if let Ok(sessions) = std::fs::read_dir(app_dir.join("payloads")) {
        for entry in sessions.flatten() {
            if entry.path().is_dir() {
                fs_secure::set_mode(&entry.path(), fs_secure::DIR_MODE);
                if let Ok(segments) = std::fs::read_dir(entry.path()) {
                    for segment in segments.flatten() {
                        fs_secure::harden_file(&segment.path());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_app_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("halo-paths-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        dir
    }

    #[test]
    fn legacy_names_migrate_in_place() {
        let dir = temp_app_dir();
        std::fs::write(dir.join("hexbuffer.db"), b"db").unwrap();
        std::fs::write(dir.join("hexbuffer.db-wal"), b"wal").unwrap();
        std::fs::write(dir.join("hexbuffer.db-shm"), b"shm").unwrap();
        std::fs::write(dir.join("hexbuffer-ca.pem"), b"cert").unwrap();
        std::fs::write(dir.join("logs/hexbuffer.log"), b"log").unwrap();
        std::fs::create_dir_all(dir.join(".hexbuffer")).unwrap();
        std::fs::write(dir.join(".hexbuffer/ca-key.pem"), b"key").unwrap();

        migrate_legacy_file_names(&dir);

        assert_eq!(std::fs::read(dir.join("halo.db")).unwrap(), b"db");
        assert_eq!(std::fs::read(dir.join("halo.db-wal")).unwrap(), b"wal");
        assert_eq!(std::fs::read(dir.join("halo.db-shm")).unwrap(), b"shm");
        assert_eq!(std::fs::read(dir.join("halo-ca.pem")).unwrap(), b"cert");
        assert_eq!(std::fs::read(dir.join("logs/halo.log")).unwrap(), b"log");
        assert_eq!(std::fs::read(dir.join(".halo/ca-key.pem")).unwrap(), b"key");
        assert!(!dir.join("hexbuffer.db").exists());
        assert!(!dir.join(".hexbuffer").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_never_overwrites_halo_data() {
        let dir = temp_app_dir();
        std::fs::write(dir.join("hexbuffer.db"), b"old").unwrap();
        std::fs::write(dir.join("halo.db"), b"new").unwrap();
        std::fs::create_dir_all(dir.join(".hexbuffer")).unwrap();
        std::fs::create_dir_all(dir.join(".halo")).unwrap();
        std::fs::write(dir.join(".halo/ca.pem"), b"halo").unwrap();

        migrate_legacy_file_names(&dir);

        assert_eq!(std::fs::read(dir.join("halo.db")).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join(".halo/ca.pem")).unwrap(), b"halo");
        // Losers of the name collision stay put rather than being destroyed.
        assert!(dir.join("hexbuffer.db").exists());
        assert!(dir.join(".hexbuffer").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

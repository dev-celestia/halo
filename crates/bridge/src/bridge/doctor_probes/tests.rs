//! Doctor probe tests.

use super::*;

#[test]
fn a_managed_service_is_an_orphan_but_an_interactive_run_is_not() {
    // `comm` is the executable path, so the argument is what tells these apart.
    assert!(is_orphanable(
        "/Users/me/.opencode/bin/opencode serve --port 0 --hostname 127.0.0.1"
    ));
    assert!(is_orphanable(
        "/Users/me/.opencode/bin/opencode serve --service"
    ));
    assert!(!is_orphanable("/Users/me/.opencode/bin/opencode"));
    assert!(!is_orphanable(
        "/Users/me/.opencode/bin/opencode auth login"
    ));
}

#[test]
fn language_servers_are_still_orphans() {
    assert!(is_orphanable("/x/rust-analyzer"));
    assert!(is_orphanable("/x/typescript-language-server --stdio"));
    assert!(is_orphanable("/x/bash-language-server start"));
    assert!(!is_orphanable("/tmp/halo-check/debug/Halo"));
}

fn permissions(accessibility: Option<bool>, screen: Option<bool>) -> DesktopPermissions {
    DesktopPermissions {
        accessibility,
        screen_recording: screen,
    }
}

#[test]
fn a_denied_accessibility_grant_is_its_own_failing_row() {
    let path = Path::new("/usr/local/bin/agent-desktop");
    let row = agent_desktop_permissions_item(&path, Some(&permissions(Some(false), Some(false))));
    assert_eq!(row.category, DoctorCategory::BrowserProxy);
    assert_eq!(row.status, DoctorStatus::Failed);
    assert_eq!(row.action_label, "Open permission guide");
    assert!(row.can_auto_install, "the guide is the only fix");
    assert!(row.hint.contains("Accessibility"), "{}", row.hint);
    assert!(row.description.contains("denied"), "{}", row.description);
    // Installed and usable are different states: the engine row is separate.
    assert_ne!(row.id, "browser:agent-desktop");
}

#[test]
fn screen_recording_alone_is_a_note_not_a_failure() {
    let path = Path::new("/usr/local/bin/agent-desktop");
    let row = agent_desktop_permissions_item(&path, Some(&permissions(Some(true), Some(false))));
    assert_eq!(row.status, DoctorStatus::Ready, "reads and acts still work");
    assert!(row.can_auto_install, "the guide is still offered");
    assert!(
        row.description.contains("Screen Recording"),
        "{}",
        row.description
    );
    assert!(
        !row.description.contains("Accessibility denied"),
        "{}",
        row.description
    );
}

#[test]
fn nothing_is_offered_once_both_grants_are_in_place() {
    let path = Path::new("/usr/local/bin/agent-desktop");
    let row = agent_desktop_permissions_item(&path, Some(&permissions(Some(true), Some(true))));
    assert_eq!(row.status, DoctorStatus::Ready);
    assert!(
        !row.can_auto_install,
        "no button when there is nothing to grant"
    );
}

#[test]
fn an_unreadable_state_is_reported_as_unknown_not_denied() {
    let path = Path::new("/usr/local/bin/agent-desktop");
    let row = agent_desktop_permissions_item(&path, None);
    assert_eq!(row.status, DoctorStatus::Failed);
    assert!(
        row.description.contains("could not be read"),
        "{}",
        row.description
    );
    assert!(row.can_auto_install);
}

/// Env overrides for Cursor probe tests must not race.
static CURSOR_PROBE_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvGuard {
    keys: Vec<&'static str>,
    previous: Vec<Option<std::ffi::OsString>>,
}

impl EnvGuard {
    fn set(pairs: &[(&'static str, Option<&str>)]) -> Self {
        let mut keys = Vec::with_capacity(pairs.len());
        let mut previous = Vec::with_capacity(pairs.len());
        for &(key, value) in pairs {
            previous.push(std::env::var_os(key));
            keys.push(key);
            match value {
                Some(v) => unsafe { std::env::set_var(key, v) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
        Self { keys, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, prev) in self.keys.iter().zip(self.previous.drain(..)) {
            match prev {
                Some(v) => unsafe { std::env::set_var(key, v) },
                None => unsafe { std::env::remove_var(key) },
            }
        }
    }
}

struct TempScratch {
    root: PathBuf,
}

impl TempScratch {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "halo-cursor-doctor-{}-{}",
            label,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create scratch dir");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn cursor_probe_missing_without_node() {
    let _lock = CURSOR_PROBE_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvGuard::set(&[
        ("NODE_EXECUTABLE", Some("/tmp/halo-doctor-missing-node-bin")),
        ("PATH", Some("")),
        ("CURSOR_SDK_SHIM_EXECUTABLE", None),
        ("CURSOR_API_KEY", None),
    ]);
    let row = probe_cursor();
    assert_eq!(row.id, "harness:cursor");
    assert_eq!(row.status, DoctorStatus::Missing);
    assert!(row.can_auto_install);
    assert!(row.hint.contains("Node"), "{}", row.hint);
}

#[test]
fn cursor_probe_missing_without_sdk() {
    let _lock = CURSOR_PROBE_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempScratch::new("no-sdk");
    // A real file so resolve_node succeeds without needing system Node.
    let fake_node = tmp.path().join("node");
    std::fs::write(&fake_node, b"#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&fake_node, std::fs::Permissions::from_mode(0o755));
    }
    let adapters = tmp.path().join("adapters");
    std::fs::create_dir_all(&adapters).unwrap();
    let state = tmp.path().join("cursor-state");
    std::fs::create_dir_all(&state).unwrap();
    let fake_node_s = fake_node.to_string_lossy().into_owned();
    let adapters_s = adapters.to_string_lossy().into_owned();
    let state_s = state.to_string_lossy().into_owned();
    let _env = EnvGuard::set(&[
        ("NODE_EXECUTABLE", Some(&fake_node_s)),
        ("HALO_ADAPTERS_DIR", Some(&adapters_s)),
        ("CURSOR_SDK_SHIM_EXECUTABLE", None),
        ("CURSOR_API_KEY", None),
        ("HALO_CURSOR_STATE_DIR", Some(&state_s)),
    ]);
    let row = probe_cursor();
    assert_eq!(row.status, DoctorStatus::Missing);
    assert!(row.can_auto_install);
    assert!(
        row.hint.contains("@cursor/sdk") || row.hint.contains("shim"),
        "{}",
        row.hint
    );
}

#[test]
fn cursor_probe_failed_without_auth() {
    let _lock = CURSOR_PROBE_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempScratch::new("no-auth");
    let shim = tmp.path().join("fake-shim");
    std::fs::write(&shim, b"ok").unwrap();
    let state = tmp.path().join("cursor-state");
    std::fs::create_dir_all(&state).unwrap();
    let shim_s = shim.to_string_lossy().into_owned();
    let state_s = state.to_string_lossy().into_owned();
    let _env = EnvGuard::set(&[
        ("CURSOR_SDK_SHIM_EXECUTABLE", Some(&shim_s)),
        ("CURSOR_API_KEY", None),
        ("HALO_CURSOR_STATE_DIR", Some(&state_s)),
        // Node still required by the first branch; provide a stub.
        ("NODE_EXECUTABLE", Some(&shim_s)),
    ]);
    let row = probe_cursor();
    assert_eq!(row.status, DoctorStatus::Failed);
    assert!(!row.can_auto_install);
    assert!(row.hint.contains("API key"), "{}", row.hint);
}

#[test]
fn cursor_probe_ready_with_sdk_and_auth() {
    let _lock = CURSOR_PROBE_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempScratch::new("ready");
    let shim = tmp.path().join("fake-shim");
    std::fs::write(&shim, b"ok").unwrap();
    let state = tmp.path().join("cursor-state");
    std::fs::create_dir_all(&state).unwrap();
    let shim_s = shim.to_string_lossy().into_owned();
    let state_s = state.to_string_lossy().into_owned();
    let _env = EnvGuard::set(&[
        ("CURSOR_SDK_SHIM_EXECUTABLE", Some(&shim_s)),
        ("CURSOR_API_KEY", Some("test-key-not-real")),
        ("HALO_CURSOR_STATE_DIR", Some(&state_s)),
        ("NODE_EXECUTABLE", Some(&shim_s)),
    ]);
    let row = probe_cursor();
    assert_eq!(row.status, DoctorStatus::Ready);
    assert!(row.can_auto_install);
    assert!(row.hint.contains("ready"), "{}", row.hint);
}

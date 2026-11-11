//! Editor module tests.

use super::*;

use halo_agent::ModelInfo;

/// Relaunch hydrates from these prefs fields; a serde miss would force
/// every cold start back onto a CLI spawn.
#[test]
fn agent_model_catalogs_round_trip_in_prefs() {
    let mut prefs = CoderPrefs::default();
    prefs.agent_model_catalogs.insert(
        "pi".into(),
        vec![ModelInfo {
            id: "antigravity/claude-opus-5-5".into(),
            display_name: "claude-opus-5-5".into(),
            supports_images: Some(true),
        }],
    );
    prefs
        .agent_model_catalogs_fetched_at
        .insert("pi".into(), 1_700_000_000);
    let json = serde_json::to_vec(&prefs).unwrap();
    let back: CoderPrefs = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        back.agent_model_catalogs
            .get("pi")
            .map(|m| m[0].id.as_str()),
        Some("antigravity/claude-opus-5-5")
    );
    assert_eq!(
        back.agent_model_catalogs_fetched_at.get("pi").copied(),
        Some(1_700_000_000)
    );
}

/// A corrupt file must never be silently reset: defaults load, the user
/// hears about it, and the unreadable bytes survive for support.
#[test]
fn a_corrupt_prefs_file_is_quarantined_and_defaults_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("coder.json");
    std::fs::write(&path, "{ not json").unwrap();

    let (prefs, error) = load_prefs_from(&path);
    assert!(error.is_some(), "a parse failure must be reported");
    assert_eq!(prefs.root_path, CoderPrefs::default().root_path);

    let quarantined = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("coder.json.corrupt-")
        })
        .expect("quarantine copy kept");
    let kept = std::fs::read(quarantined.path()).unwrap();
    assert_eq!(kept, b"{ not json");
    assert!(
        !path.exists(),
        "the corrupt file no longer blocks the next save"
    );
    assert!(
        error.unwrap().contains("coder.json.corrupt-"),
        "the message names the quarantine path"
    );
}

#[test]
fn a_clean_prefs_file_loads_without_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("coder.json");
    std::fs::write(&path, r#"{"root_path": "/tmp/demo"}"#).unwrap();
    let (prefs, error) = load_prefs_from(&path);
    assert!(error.is_none());
    assert_eq!(prefs.root_path.as_deref(), Some("/tmp/demo"));
}

/// An absent file is the first-launch shape, not an error.
#[test]
fn a_missing_prefs_file_is_a_first_launch_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (prefs, error) = load_prefs_from(&dir.path().join("coder.json"));
    assert!(error.is_none());
    assert_eq!(prefs.root_path, CoderPrefs::default().root_path);
}

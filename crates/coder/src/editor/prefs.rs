//! Coder preferences: load/save, recent projects, open tabs, and remembered
//! agent harness/model selection (stored here for shared `coder.json`).

use super::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CoderPrefs {
    /// Last folder opened in the Coder IDE.
    pub root_path: Option<String>,
    /// Recently opened project folders (most recent first).
    #[serde(default)]
    pub recent_projects: Vec<String>,
    /// Open editor tabs keyed by absolute project root path.
    #[serde(default)]
    pub open_tabs_by_root: std::collections::HashMap<String, CoderPersistedTabs>,
    /// Shared IDE settings (font, shell, agent, …).
    #[serde(default)]
    pub settings: Option<CoderPersistedSettings>,
    /// Last coding-agent harness id. The panel only drives Pi (`"pi"`).
    #[serde(default)]
    pub agent_harness: Option<String>,
    /// Last coding-agent model id for [`Self::agent_harness`].
    #[serde(default)]
    pub agent_model: Option<String>,
    /// Last model per harness. `agent_model` only names the active harness's
    /// choice; this keeps every other harness's choice intact so switching
    /// back restores it instead of reusing a model from another CLI.
    #[serde(default)]
    pub agent_models: std::collections::HashMap<String, String>,
    /// Last probed model list per harness, so the picker opens instantly on
    /// relaunch instead of waiting on `pi --list-models` / `opencode models`.
    #[serde(default)]
    pub agent_model_catalogs: std::collections::HashMap<String, Vec<halo_agent::ModelInfo>>,
    /// Unix seconds when each [`Self::agent_model_catalogs`] entry was probed.
    #[serde(default)]
    pub agent_model_catalogs_fetched_at: std::collections::HashMap<String, u64>,
}

pub(crate) fn prefs_path() -> PathBuf {
    halo_core::paths::get_shared_app_dir().join("coder.json")
}

/// The first settings-read failure since boot, for the bridge to surface as a
/// toast. A reset must never be silent; consumed once.
pub(crate) static SETTINGS_LOAD_ERROR: std::sync::Mutex<Option<String>> =
    std::sync::Mutex::new(None);

pub(crate) fn record_settings_load_error(message: String) {
    if let Ok(mut slot) = SETTINGS_LOAD_ERROR.lock() {
        if slot.is_none() {
            *slot = Some(message);
        }
    }
}

pub fn take_settings_load_error() -> Option<String> {
    SETTINGS_LOAD_ERROR
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
}

/// Read + parse one prefs file. A file that exists but does not parse is
/// quarantined — renamed with a timestamp so the next `save_prefs` (a
/// read-modify-write) cannot destroy whatever the user had. The bytes stay on
/// disk for support. Returns the prefs to use and, on a parse failure, the
/// message to surface.
pub(crate) fn load_prefs_from(path: &std::path::Path) -> (CoderPrefs, Option<String>) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        // Never written yet (or unreadable for the same reason a write would
        // fail) — an absent file is the first-launch shape, not an error.
        Err(_) => return (CoderPrefs::default(), None),
    };
    match serde_json::from_slice::<CoderPrefs>(&bytes) {
        Ok(prefs) => (prefs, None),
        Err(error) => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = path.with_extension(format!("json.corrupt-{stamp}"));
            let message = match fs::rename(path, &backup) {
                Ok(()) => format!(
                    "Coder settings could not be read ({error}); the original was kept as {} and defaults were restored.",
                    backup.display()
                ),
                Err(rename_error) => format!(
                    "Coder settings could not be read ({error}) and the unreadable file could not be moved aside ({rename_error}); defaults were restored."
                ),
            };
            (CoderPrefs::default(), Some(message))
        }
    }
}

pub fn load_prefs() -> CoderPrefs {
    let (prefs, error) = load_prefs_from(&prefs_path());
    if let Some(message) = error {
        record_settings_load_error(message);
    }
    prefs
}

pub fn save_prefs(prefs: &CoderPrefs) -> Result<(), String> {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(prefs).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    {
        use std::io::Write;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp).map_err(|e| e.to_string())?;
        file.write_all(&json)
            .and_then(|_| file.flush())
            .map_err(|e| e.to_string())?;
    }
    // The file holds provider API keys; keep it owner-only even though the
    // enclosing app directory is already private.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(())
}

pub(crate) fn ensure_recent_project(prefs: &mut CoderPrefs, path: &str) {
    // Keep existing order — selecting/switching a project must not reshuffle the list.
    if prefs.recent_projects.iter().any(|p| p == path) {
        return;
    }
    prefs.recent_projects.insert(0, path.to_string());
    prefs.recent_projects.truncate(MAX_RECENT_PROJECTS);
}

pub fn save_root_path(root_path: Option<&str>) -> Result<(), String> {
    let mut prefs = load_prefs();
    prefs.root_path = root_path.map(str::to_string);
    if let Some(path) = root_path {
        ensure_recent_project(&mut prefs, path);
    }
    save_prefs(&prefs)
}

/// Soft cap so prefs stay small if a user opens many files.
pub(crate) const MAX_PERSISTED_TABS: usize = 40;

pub fn save_open_tabs(root: &str, tabs: &CoderPersistedTabs) -> Result<(), String> {
    let mut prefs = load_prefs();
    let empty = if !tabs.panes.is_empty() {
        tabs.panes.iter().all(|p| p.tabs.is_empty())
    } else {
        tabs.primary_tabs.is_empty() && tabs.secondary_tabs.is_empty()
    };
    if empty {
        prefs.open_tabs_by_root.remove(root);
    } else {
        let mut trimmed = tabs.clone();
        for pane in &mut trimmed.panes {
            if pane.tabs.len() > MAX_PERSISTED_TABS {
                let drop_n = pane.tabs.len() - MAX_PERSISTED_TABS;
                pane.tabs.drain(0..drop_n);
                if let Some(active) = pane.active.as_ref() {
                    if !pane.tabs.iter().any(|p| p == active) {
                        pane.active = pane.tabs.last().cloned();
                    }
                }
            }
        }
        if trimmed.primary_tabs.len() > MAX_PERSISTED_TABS {
            let drop_n = trimmed.primary_tabs.len() - MAX_PERSISTED_TABS;
            trimmed.primary_tabs.drain(0..drop_n);
            if let Some(active) = trimmed.active_path.as_ref() {
                if !trimmed.primary_tabs.iter().any(|p| p == active) {
                    trimmed.active_path = trimmed.primary_tabs.last().cloned();
                }
            }
        }
        if trimmed.secondary_tabs.len() > MAX_PERSISTED_TABS {
            let drop_n = trimmed.secondary_tabs.len() - MAX_PERSISTED_TABS;
            trimmed.secondary_tabs.drain(0..drop_n);
            if let Some(active) = trimmed.secondary_active.as_ref() {
                if !trimmed.secondary_tabs.iter().any(|p| p == active) {
                    trimmed.secondary_active = trimmed.secondary_tabs.last().cloned();
                }
            }
        }
        prefs.open_tabs_by_root.insert(root.to_string(), trimmed);
    }
    save_prefs(&prefs)
}

pub fn load_open_tabs(root: &str) -> CoderPersistedTabs {
    load_prefs()
        .open_tabs_by_root
        .get(root)
        .cloned()
        .unwrap_or_default()
}

/// Load remembered IDE settings, if the user has changed any.
pub fn load_settings() -> Option<CoderPersistedSettings> {
    load_prefs().settings
}

pub fn save_settings(settings: &CoderPersistedSettings) -> Result<(), String> {
    let mut prefs = load_prefs();
    prefs.settings = Some(settings.clone());
    save_prefs(&prefs)
}

/// Last harness + model chosen in the agent panel / settings.
pub fn load_agent_selection() -> (Option<String>, Option<String>) {
    let prefs = load_prefs();
    (prefs.agent_harness, prefs.agent_model)
}

/// The model each harness last ran, so a switch restores that harness's own
/// choice. `agent_model` alone cannot do this: it is one shared slot.
pub fn load_agent_models() -> std::collections::HashMap<String, String> {
    load_prefs().agent_models
}

/// Cached model catalogs + when each was last probed (unix seconds).
pub fn load_agent_model_catalogs() -> (
    std::collections::HashMap<String, Vec<halo_agent::ModelInfo>>,
    std::collections::HashMap<String, u64>,
) {
    let prefs = load_prefs();
    (
        prefs.agent_model_catalogs,
        prefs.agent_model_catalogs_fetched_at,
    )
}

/// Persist one harness's probed model list so the next launch skips the spawn.
pub fn save_agent_model_catalog(
    harness: &str,
    models: &[halo_agent::ModelInfo],
) -> Result<(), String> {
    let mut prefs = load_prefs();
    prefs
        .agent_model_catalogs
        .insert(harness.to_string(), models.to_vec());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    prefs
        .agent_model_catalogs_fetched_at
        .insert(harness.to_string(), now);
    save_prefs(&prefs)
}

pub fn save_agent_selection(harness: &str, model: Option<&str>) -> Result<(), String> {
    let mut prefs = load_prefs();
    prefs.agent_harness = Some(harness.to_string());
    prefs.agent_model = model.map(str::to_string);
    match model {
        Some(model) => {
            prefs
                .agent_models
                .insert(harness.to_string(), model.to_string());
        }
        None => {
            prefs.agent_models.remove(harness);
        }
    }
    save_prefs(&prefs)
}

/// Valid on-disk project folders, most recent first.
///
/// Missing folders are dropped from prefs (and their remembered tabs) so
/// deleted projects do not linger in the Agent sessions rail.
pub fn recent_projects() -> Vec<String> {
    let mut prefs = load_prefs();
    let before = prefs.recent_projects.len();
    let root_before = prefs.root_path.clone();
    prefs.recent_projects.retain(|p| Path::new(p).is_dir());
    prefs
        .open_tabs_by_root
        .retain(|root, _| Path::new(root).is_dir());
    if prefs
        .root_path
        .as_ref()
        .is_some_and(|root| !Path::new(root).is_dir())
    {
        prefs.root_path = None;
    }
    let pruned = prefs.recent_projects.len() != before || prefs.root_path != root_before;
    if pruned {
        let _ = save_prefs(&prefs);
    }
    let mut projects = prefs.recent_projects;
    if projects.is_empty() {
        if let Some(root) = prefs.root_path {
            if Path::new(&root).is_dir() {
                projects.push(root);
            }
        }
    }
    projects
}

/// Forget a remembered project folder (does not delete files on disk).
pub fn remove_recent_project(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    let mut prefs = load_prefs();
    prefs.recent_projects.retain(|p| p != path);
    prefs.open_tabs_by_root.remove(path);
    if prefs.root_path.as_deref() == Some(path) {
        prefs.root_path = None;
    }
    save_prefs(&prefs)
}

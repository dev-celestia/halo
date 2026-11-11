//! Persist the work-tab strip, activity sessions, and app appearance across
//! launches.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShellLayout {
    #[serde(default)]
    pub sessions: Vec<ShellSession>,
    pub active_session_id: Option<String>,
    #[serde(default)]
    pub tabs: Vec<ShellTab>,
    pub active_tab_id: Option<String>,
    /// Settings → Appearance → Typography.
    #[serde(default)]
    pub appearance: ShellAppearance,
    /// Stable work-kind strings pinned as icons on the shell desktop.
    /// `None` = never customized (the desktop seeds every app); `Some([])`
    /// means the user deliberately unpinned all of them.
    #[serde(default)]
    pub pinned_apps: Option<Vec<String>>,
    /// First-run bottom-right stepper finished or dismissed.
    #[serde(default)]
    pub onboarding_completed: bool,
    /// Current step index while onboarding is open (0 = welcome).
    #[serde(default)]
    pub onboarding_step: u8,
}

/// App-wide typography: UI face, mono face, and base size.
///
/// Every field is optional so a fresh profile means "whatever the theme
/// defaults to" (Geist + Geist Mono at 16px) rather than a pinned value that
/// would survive a theme change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellAppearance {
    /// GPUI family for the interface. `None` = Geist (theme default).
    #[serde(default)]
    pub font_family: Option<String>,
    /// GPUI monospace family for code, diffs, and logs. `None` = Geist Mono.
    #[serde(default)]
    pub mono_font_family: Option<String>,
    /// Base interface size in px — also the window rem size. `None` = 16.
    #[serde(default)]
    pub font_size: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShellSession {
    pub id: String,
    pub title: String,
    pub active_work_tab_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShellTab {
    pub id: String,
    pub title: String,
    /// Stable string for [`halo_bridge::WorkKind`].
    pub kind: String,
    pub session_id: String,
}

fn prefs_path() -> PathBuf {
    halo_core::paths::get_shared_app_dir().join("shell.json")
}

pub fn load_layout() -> ShellLayout {
    let path = prefs_path();
    let Ok(bytes) = fs::read(&path) else {
        return ShellLayout::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_layout(layout: &ShellLayout) -> Result<(), String> {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(layout).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

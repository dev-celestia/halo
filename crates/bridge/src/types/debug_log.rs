//! Debug-log and Git-output entries.

use serde::{Deserialize, Serialize};

/// Severity for the in-app debugger log ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DebugLogLevel {
    #[default]
    Info,
    Warn,
    Error,
    Debug,
}

impl DebugLogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Debug => "debug",
        }
    }
}

/// One entry in the debugger log ring on [`crate::BridgeState`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugLogEntry {
    pub timestamp: String,
    pub level: DebugLogLevel,
    pub message: String,
    pub source: String,
}

impl DebugLogEntry {
    pub fn new(
        level: DebugLogLevel,
        source: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: chrono_now(),
            level,
            message: message.into(),
            source: source.into(),
        }
    }
}

/// One failed git command in the Source Control Git Output dock.
///
/// Kept separately from [`DebugLogEntry`]: the dock is the user-facing console
/// for git, shows the command that failed above git's own stderr, and survives
/// the `git status` snapshot swaps that replace [`crate::GitState`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitOutputEntry {
    pub timestamp: String,
    /// What the command was doing — `"Switching to main"`, `"Pushing"`.
    pub label: String,
    /// git's stderr/stdout, verbatim.
    pub detail: String,
}

impl GitOutputEntry {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            timestamp: chrono_now(),
            label: label.into(),
            detail: detail.into(),
        }
    }
}

fn chrono_now() -> String {
    // Avoid pulling chrono into bridge — use a simple UTC-ish stamp via system time.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_millis())
        .unwrap_or(0);
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}.{millis:03}")
}

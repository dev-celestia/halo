//! Wire types shared by the engine, the facade and the CLI front.

use serde::Serialize;

/// Arguments for `halo-desktop snapshot`.
///
/// These map one-to-one onto `agent-desktop snapshot` flags. Nothing is
/// invented: a flag that does not exist in the engine does not exist here
/// either, because a wrapper that accepts it would fail later and worse.
#[derive(Debug, Clone, Default)]
pub struct SnapshotRequest {
    /// Application display name. Required unless a shell `surface` is used.
    pub app: Option<String>,
    /// Pick one window when the app has several. Without it the engine takes
    /// the only visible window and answers `AMBIGUOUS_TARGET` when it cannot.
    pub window_id: Option<String>,
    /// Overlay kind: `menu`, `sheet`, `alert`, `focused`, … Gated on
    /// [`EngineStatus::supported_surfaces`] before the call.
    pub surface: Option<String>,
    /// Start traversal from a ref instead of the window root.
    pub root: Option<String>,
    /// Shallow map with `children_count` on cut branches.
    pub skeleton: bool,
    /// Interactive elements only. The default at the call site; kept as a bool
    /// so the CLI can turn it off with `--all`.
    pub interactive: bool,
    pub compact: bool,
    /// Depth cap. The engine defaults to 10; `None` keeps that default.
    pub max_depth: Option<u32>,
    /// Observation deadline. The engine defaults to 3 000 ms, which is short
    /// for a cold Chromium app (10–25 s).
    pub timeout_ms: Option<u64>,
    /// Rendering budget for this call, clamped to [`crate::HARD_MAX_CHARS`].
    pub max_chars: Option<usize>,
}

/// Arguments for `halo-desktop find`.
#[derive(Debug, Clone, Default)]
pub struct FindRequest {
    pub app: Option<String>,
    pub role: Option<String>,
    pub name: Option<String>,
    /// Matches name, value, title or description.
    pub text: Option<String>,
    /// Search only inside this ref's subtree — far cheaper than a full dump.
    pub root: Option<String>,
    pub surface: Option<String>,
    /// Case-insensitive equality instead of a fuzzy match.
    pub exact: bool,
    /// Return the first match only.
    pub first: bool,
    pub limit: Option<u32>,
}

/// One headless action. Physical input (`--headed`, drag, hover) is not a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActAction {
    Click,
    Type,
    Press,
    Select,
    Scroll,
    Focus,
}

impl ActAction {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "click" => Some(Self::Click),
            "type" => Some(Self::Type),
            "press" => Some(Self::Press),
            "select" => Some(Self::Select),
            "scroll" => Some(Self::Scroll),
            "focus" => Some(Self::Focus),
            _ => None,
        }
    }

    pub fn engine_name(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::Type => "type",
            Self::Press => "press",
            Self::Select => "select",
            Self::Scroll => "scroll",
            Self::Focus => "focus",
        }
    }

    /// Press sends a combo, not a ref. Every other action addresses an element.
    pub fn needs_ref(self) -> bool {
        !matches!(self, Self::Press)
    }

    pub fn needs_value(self) -> bool {
        matches!(self, Self::Type | Self::Press | Self::Select)
    }
}

/// Arguments for `halo-desktop act`.
#[derive(Debug, Clone)]
pub struct ActRequest {
    pub reference: Option<String>,
    /// Generation printed by the last snapshot. A mismatch is refused before
    /// the engine is spawned.
    pub generation: u64,
    pub action: ActAction,
    pub value: Option<String>,
}

/// What the agent gets back from `halo-desktop act`.
#[derive(Debug, Clone, Serialize)]
pub struct ActOutcome {
    pub generation: u64,
    pub changed: bool,
    pub evidence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<String>,
}

/// What the engine said, before this crate decides how much of it to keep.
#[derive(Debug, Clone, Default)]
pub struct RawSnapshot {
    pub app: String,
    pub window_title: String,
    pub snapshot_id: String,
    pub ref_count: u64,
    /// `false` means the engine hit its own observation budget. That is not an
    /// error and must not be retried — drill with `--root` instead.
    pub complete: bool,
    pub truncated: bool,
    pub nodes_observed: Option<u64>,
    pub tree: serde_json::Value,
}

/// What the agent gets back from `halo-desktop snapshot`.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub generation: u64,
    pub text: String,
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<Truncation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Truncation {
    pub kept_chars: usize,
    pub dropped_chars: usize,
    pub hint: String,
}

/// macOS TCC state as the engine reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Granted,
    Denied,
    Unknown,
}

impl PermissionState {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "granted" | "allowed" | "authorized" => Self::Granted,
            "denied" => Self::Denied,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Unknown => "unknown",
        }
    }

    /// Only an explicit grant is good enough to advertise the tool.
    pub fn is_granted(self) -> bool {
        self == Self::Granted
    }
}

/// `agent-desktop status`, normalised.
#[derive(Debug, Clone, Default)]
pub struct EngineStatus {
    pub version: String,
    pub platform: String,
    pub state_root: String,
    pub accessibility: PermissionState,
    pub screen_recording: PermissionState,
    /// Advertised surfaces only. The `--surface` enum accepts more kinds than
    /// this list, and asking for an unlisted one is `PLATFORM_NOT_SUPPORTED`.
    pub supported_surfaces: Vec<String>,
    pub session_id: Option<String>,
}

impl Default for PermissionState {
    fn default() -> Self {
        Self::Unknown
    }
}

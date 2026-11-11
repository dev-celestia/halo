//! Wire types shared by the engine, the core, and the MCP front.

use serde::{Deserialize, Serialize};

/// Arguments for `browser_snapshot`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SnapshotRequest {
    /// CSS selector to scope the tree. The only reliable way to stay small.
    #[serde(default)]
    pub scope: Option<String>,
    /// Defaults to true: interactive-only is the smallest useful tree.
    #[serde(default)]
    pub interactive: Option<bool>,
    /// Per-call budget. Clamped to [`crate::HARD_MAX_CHARS`].
    #[serde(default)]
    pub max_chars: Option<usize>,
}

/// What the agent gets back from `browser_snapshot`.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub generation: u64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<Truncation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Truncation {
    pub kept_chars: usize,
    pub dropped_chars: usize,
    pub hint: String,
}

/// The only three verbs. Anything else is a new verb, not a new flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Click,
    Fill,
    Press,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::Fill => "fill",
            Self::Press => "press",
        }
    }
}

/// Arguments for `browser_act`.
#[derive(Debug, Clone, Deserialize)]
pub struct ActRequest {
    /// Element ref from the snapshot, with or without the leading `@`.
    #[serde(default)]
    pub reference: String,
    /// Generation the ref came from. A mismatch is rejected, never guessed.
    #[serde(default)]
    pub generation: u64,
    pub action: Action,
    /// Text for `fill`, key name for `press`.
    #[serde(default)]
    pub value: Option<String>,
}

/// Result of `browser_act` and `browser_open`.
#[derive(Debug, Clone, Serialize)]
pub struct ActOutcome {
    /// Generation to use for the next `act`. Advances only when the page moved.
    pub generation: u64,
    pub changed: bool,
    pub url: String,
    pub title: String,
    pub note: String,
    /// What was actually observed. `fill` reports the field's read-back value,
    /// because input values never appear in body text.
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavOutcome {
    pub generation: u64,
    pub url: String,
    pub title: String,
}

/// Cheap evidence that the page moved, without a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub url: String,
    pub title: String,
    pub text_hash: u64,
}

impl Fingerprint {
    pub fn hash_text(text: &str) -> u64 {
        // FNV-1a: stable across runs, no dependency, good enough to notice change.
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

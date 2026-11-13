//! Antigravity ACP harness wire types.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::PiImage;

#[derive(Debug, Clone)]
pub struct AntigravityModelInfo {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub enum AntigravityProbeResult {
    Ready { version: String, path: PathBuf },
    Missing,
    Failed { message: String },
}

impl AntigravityProbeResult {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn setup_hint(&self) -> String {
        match self {
            Self::Ready { version, path } => {
                format!("Antigravity ACP {version} ready ({})", path.display())
            }
            Self::Missing => {
                "Antigravity ACP server is not installed. Install from Halo Agent settings or set ANTIGRAVITY_ACP_EXECUTABLE."
                    .into()
            }
            Self::Failed { message } => message.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AntigravityRunOptions {
    pub run_id: String,
    pub prompt: String,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub resume: Option<String>,
    /// Prior conversation transcript injected when starting a fresh session or
    /// when resuming a stored session ID falls back to `session/new`.
    pub fallback_context: Option<String>,
    pub custom_executable: Option<String>,
    pub approval_mode: String,
    pub plan_mode: bool,
    pub extra_env: HashMap<String, String>,
    /// Images attached to the first prompt, already capped and base64-encoded.
    pub images: Vec<PiImage>,
}

#[derive(Debug, Clone)]
pub enum AntigravityDoneStatus {
    Completed,
    Interrupted,
    Errored,
}

/// Events from the ACP driver (mapped to Halo AgentEvent upstream).
#[derive(Debug, Clone)]
pub enum AntigravityEvent {
    SessionStarted {
        session_id: String,
        model: Option<String>,
        cwd: Option<String>,
    },
    TextDelta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    CommandAccepted {
        command_id: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        id: String,
        output: Option<String>,
        is_error: bool,
    },
    InputRequested {
        request_id: String,
        title: String,
        message: String,
        options: Vec<String>,
    },
    InputResolved {
        request_id: String,
    },
    Done {
        status: AntigravityDoneStatus,
        error: Option<String>,
        session_id: Option<String>,
    },
    /// Latest context fill. `window_tokens` is never zero.
    Usage {
        used_tokens: u64,
        window_tokens: u64,
        input_tokens: u64,
        output_tokens: u64,
    },
}

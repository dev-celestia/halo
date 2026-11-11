//! OpenCode harness wire types.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::PiImage;

#[derive(Debug, Clone)]
pub struct OpenCodeModelInfo {
    pub id: String,
    pub display_name: String,
    /// `None` when the models.dev cache cannot describe the model.
    pub supports_images: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum OpenCodeProbeResult {
    Ready {
        version: String,
        path: PathBuf,
    },
    /// Installed but older than the v2 API Halo drives (≥ 2.2.0).
    WrongVersion {
        found: String,
        path: PathBuf,
        required: String,
    },
    Missing,
    /// The binary runs, but OpenCode dies at startup because its state
    /// directory is not writable (usually left root-owned by an earlier
    /// privileged run). Reinstalling cannot fix this; ownership repair can.
    StateBlocked {
        message: String,
        path: PathBuf,
    },
    Failed {
        message: String,
    },
}

impl OpenCodeProbeResult {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn needs_upgrade(&self) -> bool {
        matches!(self, Self::WrongVersion { .. })
    }

    pub fn is_state_blocked(&self) -> bool {
        matches!(self, Self::StateBlocked { .. })
    }

    pub fn setup_hint(&self) -> String {
        match self {
            Self::Ready { version, path } => {
                format!("OpenCode {version} ready ({})", path.display())
            }
            Self::WrongVersion {
                found,
                path,
                required,
            } => format!(
                "Found OpenCode {found} at {}, but Halo needs OpenCode ≥ {required} (v2 server API). Use Upgrade to install OpenCode 2.",
                path.display()
            ),
            Self::Missing => {
                "OpenCode is not installed. Use Install, or install OpenCode ≥ 2.2.0 from https://opencode.ai / set a custom path in Coder Settings."
                    .into()
            }
            Self::StateBlocked { message, .. } => message.clone(),
            Self::Failed { message } => message.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenCodeRunOptions {
    pub run_id: String,
    pub prompt: String,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub resume: Option<String>,
    pub custom_executable: Option<String>,
    pub extra_env: HashMap<String, String>,
    /// Skill ids attached to the prompt; empty attaches none.
    pub skills: Vec<String>,
    /// Images attached to the prompt, already capped and base64-encoded.
    pub images: Vec<PiImage>,
    /// `"ask"` | `"approve_edits"` | `"approve_safe"` | `"full_access"`.
    pub approval_mode: String,
    /// When `true`, restrict tool execution to read-only operations.
    pub plan_mode: bool,
}

/// One row of `GET /api/skill` on a v2 server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCodeSkill {
    /// The id a prompt attaches.
    pub id: String,
    pub name: String,
    pub description: String,
    /// Canonical skill file; `/builtin/...` for built-ins.
    pub path: String,
    /// `"project" | "user" | "system"`.
    pub scope: String,
    pub builtin: bool,
}

/// `provider/model` or `provider/model#variant` → v2 `{ providerID, id, variant? }`.
pub fn model_ref(model: &str) -> Option<serde_json::Value> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    let (model, variant) = model
        .split_once('#')
        .map_or((model, None), |(m, v)| (m, Some(v)));
    let (provider, id) = model.split_once('/')?;
    if provider.is_empty() || id.is_empty() {
        return None;
    }
    let mut value = serde_json::json!({ "providerID": provider, "id": id });
    if let Some(variant) = variant.filter(|v| !v.is_empty()) {
        value["variant"] = serde_json::json!(variant);
    }
    Some(value)
}

#[derive(Debug, Clone)]
pub enum OpenCodeDoneStatus {
    Completed,
    Interrupted,
    Errored,
}

/// Events emitted by the OpenCode HTTP/SSE driver (mapped to Halo AgentEvent upstream).
#[derive(Debug, Clone)]
pub enum OpenCodeEvent {
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
    /// A steer POST the server refused (turn already ended, bad session) —
    /// correlated like Pi so the bridge can requeue the prompt.
    CommandRejected {
        command_id: String,
        error: String,
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
    },
    InputResolved {
        request_id: String,
    },
    Done {
        status: OpenCodeDoneStatus,
        error: Option<String>,
        session_id: Option<String>,
    },
    /// A `task` / `subagent` tool call opened a child session.
    SubagentStarted {
        /// Parent tool call id when known; `session.created` may arrive first.
        parent_tool_call_id: Option<String>,
        session_id: String,
        title: String,
        agent: Option<String>,
    },
    /// The child session id is now linked to the parent's spawn call.
    SubagentBound {
        session_id: String,
        parent_tool_call_id: String,
    },
    /// One event produced by a child session.
    SubagentEvent {
        session_id: String,
        event: Box<OpenCodeEvent>,
    },
    /// A child session finished; the root turn continues.
    SubagentDone {
        session_id: String,
        status: OpenCodeDoneStatus,
        error: Option<String>,
    },
    /// Latest context fill. `window_tokens` is never zero, so the composer
    /// meter can render it.
    Usage {
        used_tokens: u64,
        window_tokens: u64,
        input_tokens: u64,
        output_tokens: u64,
    },
}

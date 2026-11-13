//! Slim agent wire types (Halo-local).

use serde::{Deserialize, Serialize};

use super::catalog::SkillInfo;
use halo_harness::McpServerConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessId {
    Pi,
    OpenCode,
    Antigravity,
    Cursor,
}

impl HarnessId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Antigravity => "antigravity",
            Self::Cursor => "cursor",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Pi => "Pi",
            Self::OpenCode => "OpenCode",
            Self::Antigravity => "Antigravity",
            Self::Cursor => "Cursor",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pi" => Some(Self::Pi),
            "opencode" | "open-code" => Some(Self::OpenCode),
            "antigravity" | "agy" | "antigravity-acp" => Some(Self::Antigravity),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }

    pub fn install_hint(self) -> &'static str {
        match self {
            Self::Pi => {
                "Pi 1.4.0 is required. Install it from Halo Agent or set a custom binary path in Settings."
            }
            Self::OpenCode => {
                "OpenCode 2.2.0 or newer is required. Install it from Halo Agent or set a custom binary path in Settings."
            }
            Self::Antigravity => {
                "Antigravity ACP server is required. Install it from Halo Agent or set ANTIGRAVITY_ACP_EXECUTABLE."
            }
            Self::Cursor => {
                "Cursor requires Node.js and a Cursor API key (cursor.com/settings → API Keys)."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessInfo {
    pub id: String,
    pub display_name: String,
    pub installed: bool,
    pub available: bool,
}

/// One harness row for the provider sheet. Probed off the UI thread.
///
/// `installed` means the binary or SDK is present. A wrong version is still
/// installed, with [`Self::needs_upgrade`]. Pi and OpenCode keep
/// [`Self::auth_ready`] true — their credentials are not a harness sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessReadiness {
    pub id: String,
    pub installed: bool,
    pub needs_upgrade: bool,
    pub state_blocked: bool,
    pub auth_ready: bool,
    pub subtitle: String,
}

impl HarnessReadiness {
    pub fn display_name(id: &str) -> &'static str {
        HarnessId::parse(id)
            .map(HarnessId::display_name)
            .unwrap_or("Harness")
    }

    /// Static line once the harness can actually run.
    pub fn ready_blurb(id: &str) -> &'static str {
        match id {
            "opencode" => "Connected via local OpenCode server",
            "antigravity" => "Signed in with Google",
            "cursor" => "Connected with Cursor API key",
            _ => "Uses provider API keys configured in Halo",
        }
    }

    pub fn is_usable(&self) -> bool {
        if !self.installed || self.needs_upgrade || self.state_blocked {
            return false;
        }
        match self.id.as_str() {
            "pi" | "opencode" => true,
            _ => self.auth_ready,
        }
    }

    /// Derive the compact picker row without re-spawning `--version`.
    pub fn to_harness_info(&self) -> HarnessInfo {
        HarnessInfo {
            id: self.id.clone(),
            display_name: Self::display_name(&self.id).to_string(),
            installed: self.installed && !self.needs_upgrade && !self.state_blocked,
            available: true,
        }
    }
}

/// UI-facing Pi readiness (pinned to harness protocol 1.4.0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PiProbeStatus {
    Ready { version: String, path: String },
    WrongVersion { found: String, path: String },
    Missing { node_available: bool },
    NodeMissing,
    Failed { message: String },
}

impl PiProbeStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn needs_upgrade(&self) -> bool {
        matches!(self, Self::WrongVersion { .. })
    }

    pub fn setup_hint(&self) -> String {
        match self {
            Self::Ready { version, path } => format!("Pi {version} ready ({path})"),
            Self::WrongVersion { found, path } => format!(
                "Pi {found} is installed at {path}. Upgrade to Pi 1.4.0 to continue."
            ),
            Self::Missing {
                node_available: true,
            } => "Pi is not installed. Click Install below to set up Pi 1.4.0."
                .into(),
            Self::Missing {
                node_available: false,
            } => "Node.js and Pi are not installed. Install Node.js 22.19 or newer, then install Pi 1.4.0."
                .into(),
            Self::NodeMissing => {
                "Pi requires Node.js 22.19 or newer beside the executable or on PATH.".into()
            }
            Self::Failed { message } => message.clone(),
        }
    }
}

impl From<halo_harness::PiProbeResult> for PiProbeStatus {
    fn from(value: halo_harness::PiProbeResult) -> Self {
        match value {
            halo_harness::PiProbeResult::Installed { version, path } => Self::Ready {
                version,
                path: path.display().to_string(),
            },
            halo_harness::PiProbeResult::WrongVersion { found, path } => Self::WrongVersion {
                found,
                path: path.display().to_string(),
            },
            halo_harness::PiProbeResult::BinaryNotFound { node_available } => {
                Self::Missing { node_available }
            }
            halo_harness::PiProbeResult::NodeMissing => Self::NodeMissing,
            halo_harness::PiProbeResult::ProbeFailed(message) => Self::Failed { message },
        }
    }
}

/// UI-facing OpenCode readiness (v2 API — OpenCode ≥ 2.2.0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum OpenCodeProbeStatus {
    Ready {
        version: String,
        path: String,
    },
    WrongVersion {
        found: String,
        path: String,
        required: String,
    },
    Missing,
    /// The binary works, but OpenCode dies at startup because its state
    /// directory is not writable (usually root-owned). Reinstalling cannot fix
    /// it — the ownership repair in Doctor can.
    StateBlocked {
        message: String,
        path: String,
    },
    Failed {
        message: String,
    },
}

impl OpenCodeProbeStatus {
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
            Self::Ready { version, path } => format!("OpenCode {version} ready ({path})"),
            Self::WrongVersion {
                found,
                path,
                required,
            } => format!(
                "OpenCode {found} is installed at {path}. Upgrade to OpenCode {required} or newer to continue."
            ),
            Self::Missing => {
                "OpenCode is not installed. Click Install below, or set a custom binary path in Settings."
                    .into()
            }
            Self::StateBlocked { message, .. } => message.clone(),
            Self::Failed { message } => message.clone(),
        }
    }
}

impl From<halo_harness::OpenCodeProbeResult> for OpenCodeProbeStatus {
    fn from(value: halo_harness::OpenCodeProbeResult) -> Self {
        match value {
            halo_harness::OpenCodeProbeResult::Ready { version, path } => Self::Ready {
                version,
                path: path.display().to_string(),
            },
            halo_harness::OpenCodeProbeResult::WrongVersion {
                found,
                path,
                required,
            } => Self::WrongVersion {
                found,
                path: path.display().to_string(),
                required,
            },
            halo_harness::OpenCodeProbeResult::Missing => Self::Missing,
            halo_harness::OpenCodeProbeResult::StateBlocked { message, path } => {
                Self::StateBlocked {
                    message,
                    path: path.display().to_string(),
                }
            }
            halo_harness::OpenCodeProbeResult::Failed { message } => Self::Failed { message },
        }
    }
}

/// UI-facing Antigravity ACP readiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AntigravityProbeStatus {
    Ready { version: String, path: String },
    Missing,
    Failed { message: String },
}

impl AntigravityProbeStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn setup_hint(&self) -> String {
        match self {
            Self::Ready { version, path } => {
                format!("Antigravity ACP {version} ready ({path})")
            }
            Self::Missing => {
                "Antigravity ACP server is not installed. Click Install below, or set ANTIGRAVITY_ACP_EXECUTABLE."
                    .into()
            }
            Self::Failed { message } => message.clone(),
        }
    }
}

impl From<halo_harness::AntigravityProbeResult> for AntigravityProbeStatus {
    fn from(value: halo_harness::AntigravityProbeResult) -> Self {
        match value {
            halo_harness::AntigravityProbeResult::Ready { version, path } => Self::Ready {
                version,
                path: path.display().to_string(),
            },
            halo_harness::AntigravityProbeResult::Missing => Self::Missing,
            halo_harness::AntigravityProbeResult::Failed { message } => Self::Failed { message },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    /// `Some(false)` blocks image attachments; `None` is unknown (allow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_images: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub text: String,
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl TodoItem {
    pub fn new(text: String, status: &str) -> Self {
        let normalized = match status {
            "completed" | "complete" | "done" => "completed",
            "in_progress" | "inProgress" | "in-progress" => "inProgress",
            _ => "pending",
        };
        Self {
            text,
            done: normalized == "completed",
            status: Some(normalized.into()),
        }
    }

    pub fn effective_status(&self) -> &str {
        self.status
            .as_deref()
            .unwrap_or(if self.done { "completed" } else { "pending" })
    }
}

/// Write vs edit for the agent Artifacts rail (from WriteFile / EditFile tools).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileChangeKind {
    Write,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolCall {
    Exec {
        command: String,
    },
    ReadFile {
        path: String,
    },
    WriteFile {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<String>,
    },
    EditFile {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_string: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_string: Option<String>,
    },
    Search {
        pattern: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    Glob {
        pattern: String,
    },
    Todo {
        items: Vec<TodoItem>,
    },
    /// Delegation to a child agent (`task` / `subagent`).
    Task {
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
        #[serde(default)]
        prompt: String,
    },
    /// A `halo-browser` invocation, recognised inside the shell tool. Kept
    /// distinct from [`Self::Exec`] so the transcript can say "Open example.com"
    /// instead of showing a command line.
    Browser {
        /// `open`, `snapshot`, `act`, `read`, `close`, `status`.
        action: String,
        /// URL, element ref, or selector.
        target: String,
    },
    /// A harness-native web search (`websearch` on OpenCode). Results arrive as
    /// the tool output; the transcript turns them into a source list.
    WebSearch {
        query: String,
    },
    /// A harness-native URL fetch (`webfetch` on OpenCode).
    WebFetch {
        url: String,
    },
    /// An interactive question (`question` on OpenCode). Halo has no answer
    /// channel for it, so the transcript renders the prompt as text — never as
    /// the raw argument JSON the harness would otherwise show.
    Question {
        #[serde(default)]
        header: String,
        #[serde(default)]
        question: String,
        #[serde(default)]
        options: Vec<String>,
    },
    Unknown {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_json: Option<String>,
    },
}

impl ToolCall {
    pub fn label(&self) -> String {
        match self {
            Self::Exec { command } => {
                let short = command.chars().take(48).collect::<String>();
                if command.len() > 48 {
                    format!("Run {short}…")
                } else {
                    format!("Run {short}")
                }
            }
            Self::ReadFile { path } => format!("Read {path}"),
            Self::WriteFile { path, .. } => format!("Write {path}"),
            Self::EditFile { path, .. } => format!("Edit {path}"),
            Self::Search { pattern, .. } => format!("Search {pattern}"),
            Self::Glob { pattern } => format!("Glob {pattern}"),
            Self::Todo { items } => {
                let done = items.iter().filter(|t| t.done).count();
                format!("Todo · {done}/{}", items.len())
            }
            Self::Task { description, .. } => format!("Subagent · {description}"),
            Self::Browser { action, target } => {
                if target.is_empty() {
                    format!("Browser {action}")
                } else {
                    format!("Browser {action}: {target}")
                }
            }
            Self::WebSearch { query } => format!("Web search: {query}"),
            Self::WebFetch { url } => format!("Fetch: {url}"),
            Self::Question {
                header, question, ..
            } => {
                let title = if !header.trim().is_empty() {
                    header.trim()
                } else {
                    question.trim()
                };
                if title.is_empty() {
                    return "Question".into();
                }
                let short = title.chars().take(56).collect::<String>();
                if title.chars().count() > 56 {
                    format!("Question · {short}…")
                } else {
                    format!("Question · {short}")
                }
            }
            Self::Unknown { name, .. } => name.clone(),
        }
    }

    /// Path + kind when this tool writes or edits a file.
    pub fn file_change(&self) -> Option<(&str, FileChangeKind)> {
        match self {
            Self::WriteFile { path, .. } => Some((path.as_str(), FileChangeKind::Write)),
            Self::EditFile { path, .. } => Some((path.as_str(), FileChangeKind::Edit)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DoneStatus {
    Completed,
    Interrupted,
    Errored,
}

/// One question the agent needs answered before it can continue
/// (approvals and AskUserQuestion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<String>,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInputAnswer {
    pub question_id: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    SessionStarted {
        harness: String,
        model: String,
        #[serde(default)]
        tools: Vec<String>,
        cwd: String,
        session_id: String,
    },
    TextDelta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    /// A stable response/content-block update; final snapshots replace deltas.
    ContentUpdated {
        message_id: String,
        content_index: usize,
        text: String,
        reasoning: bool,
        append: bool,
        done: bool,
    },
    CommandAccepted {
        command_id: String,
    },
    /// The live harness handle is installed. Pending follow-ups can steer.
    HarnessReady,
    CommandRejected {
        command_id: String,
        error: String,
    },
    ToolCall {
        id: String,
        call: ToolCall,
    },
    ToolResult {
        id: String,
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
    },
    /// Human approver / question panel — park the harness until
    /// [`super::respond_input`] answers.
    InputRequested {
        request_id: String,
        questions: Vec<UserInputQuestion>,
    },
    InputResolved {
        request_id: String,
    },
    /// A delegation tool call opened a child agent session.
    SubagentStarted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_tool_call_id: Option<String>,
        session_id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },
    /// The child session is now linked to its parent spawn call.
    SubagentBound {
        session_id: String,
        parent_tool_call_id: String,
    },
    /// One event produced by a child session; never retires the root run.
    SubagentEvent {
        session_id: String,
        event: Box<AgentEvent>,
    },
    /// A child session finished; the root turn continues.
    SubagentDone {
        session_id: String,
        status: DoneStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Steered,
    /// Latest context fill for the composer meter.
    Usage {
        used_tokens: u64,
        window_tokens: u64,
        input_tokens: u64,
        output_tokens: u64,
    },
    Done {
        status: DoneStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventPayload {
    pub run_id: String,
    /// UI chat session this run belongs to (concurrent runs).
    #[serde(default)]
    pub chat_id: String,
    pub event: AgentEvent,
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    /// Allocated by the caller before asynchronous startup emits any events.
    pub run_id: String,
    pub chat_id: String,
    pub prompt: String,
    pub harness: HarnessId,
    pub model: Option<String>,
    pub cwd: String,
    /// `"ask"` | `"approve_edits"` | `"approve_safe"` | `"full_access"`.
    pub approval_mode: String,
    /// Explore and propose a plan without applying edits. Wins over approval.
    pub plan_mode: bool,
    pub resume: Option<String>,
    /// Prior conversation transcript to prepend if a session cannot be resumed
    /// or starts fresh after switching harnesses mid-chat.
    pub fallback_context: Option<String>,
    /// Absolute paths of staged image files (png/jpeg/gif/webp).
    pub attachments: Vec<String>,
    /// Skills this chat enabled (empty = none).
    pub skills: Vec<SkillInfo>,
    /// MCP servers this chat enabled; only OpenCode consumes them.
    pub mcp_servers: Vec<McpServerConfig>,
    /// Settings → Agent → Computer use. Off means the prompt never mentions it.
    pub computer_use: bool,
    /// Physical pointer and keyboard, only when [`Self::computer_use`] is also on.
    pub computer_use_headed: bool,
}

/// Per-launch overrides, never persisted or included in diagnostic output.
/// `None` preserves the launch environment; `Some("")` explicitly removes a key.
#[derive(Clone, Default)]
pub struct AgentLaunchContext {
    /// Pi executable override (`PI_EXECUTABLE` / settings).
    pub custom_executable: Option<String>,
    /// OpenCode executable override (`OPENCODE_EXECUTABLE` / settings).
    pub opencode_executable: Option<String>,
    /// Antigravity ACP server override (`ANTIGRAVITY_ACP_EXECUTABLE` / settings).
    pub antigravity_executable: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub deepseek_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
}

impl std::fmt::Debug for AgentLaunchContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLaunchContext").finish_non_exhaustive()
    }
}

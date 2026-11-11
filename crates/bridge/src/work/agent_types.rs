//! Coding-agent transcript types: messages, sessions, subagents, and completions.

use super::*;

/// Role of a coding-agent transcript message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HaloAgentRole {
    User,
    Assistant,
    System,
}

/// One part inside an agent message (text, tool chip, etc.).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum HaloAgentPart {
    Text(String),
    /// Model thought / reasoning. Collapsed by default; user expands the body.
    Reasoning {
        text: String,
        expanded: bool,
    },
    /// Staged image paths shown on the user bubble (basename chips).
    Attachments(Vec<String>),
    ToolCall {
        id: String,
        label: String,
        detail: Option<String>,
        expanded: bool,
        is_error: bool,
        done: bool,
        /// Links parsed out of a web-search result. Empty for every other tool,
        /// and for transcripts written before the field existed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sources: Vec<HaloAgentSource>,
    },
    Error(String),
    /// User cancelled the run — rendered as a retry card in the transcript.
    Interrupted,
    /// The selected model changed mid-chat. Rendered as a divider between the
    /// turns so it is obvious which model answered which half of the thread.
    ///
    /// `from`/`to` are display names, not model ids: the transcript shows what
    /// the user picked, not the provider-qualified key the harness runs on.
    ModelChanged {
        from: String,
        to: String,
    },
}

/// One link a web search returned, shown as a row under the search chip.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HaloAgentSource {
    pub title: String,
    pub url: String,
}

/// One bubble in the agent transcript.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HaloAgentMessage {
    pub id: String,
    pub role: HaloAgentRole,
    pub parts: Vec<HaloAgentPart>,
    /// Per-turn token usage for assistant replies (composer meter stays session-level).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<HaloAgentContextUsage>,
}

impl HaloAgentMessage {
    pub fn new(id: impl Into<String>, role: HaloAgentRole, parts: Vec<HaloAgentPart>) -> Self {
        Self {
            id: id.into(),
            role,
            parts,
            usage: None,
        }
    }
}

/// Todo row derived from the latest TodoWrite tool call.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HaloAgentTodo {
    pub text: String,
    pub done: bool,
    /// pending | inProgress | completed
    pub status: String,
}

/// Write vs edit for one path in the agent Artifacts rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HaloAgentFileChangeKind {
    Write,
    Edit,
}

/// One file touched by WriteFile / EditFile during the active session.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HaloAgentFileChange {
    pub path: String,
    pub kind: HaloAgentFileChangeKind,
}

/// One optioned question parked for the human approver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaloAgentInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<String>,
    pub multi_select: bool,
}

/// Context window fill from the last assistant turn (composer meter).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloAgentContextUsage {
    pub used_tokens: u64,
    pub window_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl HaloAgentContextUsage {
    pub fn percent(&self) -> u32 {
        if self.window_tokens == 0 {
            return 0;
        }
        ((self.used_tokens.saturating_mul(100)) / self.window_tokens).min(100) as u32
    }
}

/// Live approval / AskUserQuestion wizard state above the composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaloAgentPendingInput {
    pub chat_id: String,
    pub run_id: String,
    pub request_id: String,
    /// Child agent title when the request came from a subagent (`None` = root).
    pub subagent: Option<String>,
    pub questions: Vec<HaloAgentInputQuestion>,
    /// Current page in a multi-question wizard.
    pub page: usize,
    /// Picked option indexes per question.
    pub picked: Vec<Vec<usize>>,
}

/// Lifecycle of one delegated child agent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HaloAgentSubagentStatus {
    #[default]
    Running,
    Completed,
    Interrupted,
    Errored,
}

/// One child agent spawned by a `task` / `subagent` tool call.
///
/// Child transcript rows live here, never in the parent's message list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaloAgentSubagent {
    /// Harness child session id (the join key for every child event).
    pub session_id: String,
    /// Parent tool call that opened this child, when known.
    pub parent_tool_call_id: Option<String>,
    pub title: String,
    /// Requested subagent type (`general`, `explore`, …).
    pub agent: Option<String>,
    pub status: HaloAgentSubagentStatus,
    pub error: Option<String>,
    pub messages: crate::HaloAgentMessages,
    pub streaming_part: Option<(String, usize)>,
    pub pending_input: Option<HaloAgentPendingInput>,
    pub todos: Vec<HaloAgentTodo>,
    pub file_changes: Vec<HaloAgentFileChange>,
    pub(crate) live: HaloAgentRunState,
}

impl HaloAgentSubagent {
    pub fn new(session_id: String, title: String, agent: Option<String>) -> Self {
        Self {
            session_id,
            parent_tool_call_id: None,
            title,
            agent,
            status: HaloAgentSubagentStatus::Running,
            error: None,
            messages: Default::default(),
            streaming_part: None,
            pending_input: None,
            todos: Vec::new(),
            file_changes: Vec::new(),
            live: Default::default(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.status == HaloAgentSubagentStatus::Running
    }

    /// Stable drawer/tray label; falls back to the requested agent type.
    pub fn label(&self) -> &str {
        if !self.title.trim().is_empty() {
            return self.title.as_str();
        }
        self.agent.as_deref().unwrap_or("Subagent")
    }
}

/// Queued user message waiting for the current turn to finish.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HaloAgentQueuedMessage {
    pub id: String,
    pub text: String,
    pub attachments: Vec<String>,
    pub delivery: HaloAgentDelivery,
}

/// Sending is not delivery: only a correlated CommandAccepted removes an item.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HaloAgentDelivery {
    #[default]
    Pending,
    Sending {
        run_id: String,
        command_id: String,
    },
    Rejected {
        error: String,
    },
    /// Delivery may have happened. Never automatically replay this item.
    Uncertain {
        run_id: String,
        command_id: String,
        error: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HaloAgentRunState {
    pub content: std::collections::HashMap<(String, usize), (usize, usize)>,
    pub responses: std::collections::HashMap<String, usize>,
    pub tools: std::collections::HashMap<String, (usize, usize, Option<HaloAgentFileChange>)>,
}

/// Live run status for one chat. Drives the Agents rail indicator; one live
/// harness process per chat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HaloAgentSessionStatus {
    #[default]
    Idle,
    Working,
    Errored,
}

/// One saved agent session (chat row + local transcript snapshot).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaloAgentSession {
    pub id: String,
    /// First user prompt, truncated — empty until the first send.
    pub title: String,
    /// Last text snippet for the Sessions list.
    pub preview: String,
    /// Unix millis; used to order the Sessions list (recency).
    pub updated_at: u64,
    /// Soft-removed from the active Sessions list.
    pub archived: bool,
    /// Project root this chat belongs to (`None` → "No project" in the rail).
    pub root_path: Option<String>,
    /// Harness thread id for resume (Claude/Codex/Cursor). Cleared on new session.
    pub harness_session_id: Option<String>,
    /// Sidebar / header indicator.
    pub status: HaloAgentSessionStatus,
    pub run_id: Option<String>,
    /// Unix millis when the current Working run began (`None` when idle).
    pub run_started_at: Option<u64>,
    /// Last completed run duration in seconds (kept until the next run starts).
    pub last_worked_secs: Option<u64>,
    pub messages: crate::HaloAgentMessages,
    pub streaming_part: Option<(String, usize)>,
    pub pending_input: Option<HaloAgentPendingInput>,
    /// Child agents spawned by delegation tool calls, in spawn order.
    pub subagents: Vec<HaloAgentSubagent>,
    pub attachments: Vec<String>,
    pub draft: String,
    pub model: Option<String>,
    pub plan_mode: bool,
    /// How far this chat may go without asking — independent of global settings.
    pub approval_mode: AgentApprovalMode,
    pub completion: Option<HaloAgentCompletion>,
    pub(crate) live: HaloAgentRunState,
    pub todos: Vec<HaloAgentTodo>,
    pub todo_expanded: bool,
    pub todo_dismissed: bool,
    pub queue: Vec<HaloAgentQueuedMessage>,
    /// Write/Edit paths accumulated during this session (Artifacts rail).
    pub file_changes: Vec<HaloAgentFileChange>,
    /// Skill ids this chat may use (resolved against the catalog at send time).
    pub skills: Vec<String>,
    /// MCP server names this chat may use (only OpenCode consumes them).
    pub mcp_servers: Vec<String>,
    /// Last reported context fill for the composer meter.
    pub context_usage: Option<HaloAgentContextUsage>,
}

impl HaloAgentSession {
    pub fn new_empty() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: String::new(),
            preview: String::new(),
            updated_at: agent_now_ms(),
            archived: false,
            root_path: None,
            harness_session_id: None,
            status: HaloAgentSessionStatus::Idle,
            run_id: None,
            run_started_at: None,
            last_worked_secs: None,
            messages: Default::default(),
            streaming_part: None,
            pending_input: None,
            subagents: Vec::new(),
            attachments: Vec::new(),
            draft: String::new(),
            model: None,
            plan_mode: false,
            approval_mode: AgentApprovalMode::Ask,
            completion: None,
            live: Default::default(),
            todos: Vec::new(),
            todo_expanded: true,
            todo_dismissed: false,
            queue: Vec::new(),
            file_changes: Vec::new(),
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            context_usage: None,
        }
    }

    /// Blank session stamped with the open project (or `None` for no project).
    pub fn new_for_project(root_path: Option<String>) -> Self {
        let mut session = Self::new_empty();
        session.root_path = root_path;
        session
    }

    pub fn title_or_default(&self) -> &str {
        if self.title.trim().is_empty() {
            "New session"
        } else {
            self.title.as_str()
        }
    }

    pub fn is_working(&self) -> bool {
        self.status == HaloAgentSessionStatus::Working
    }

    pub fn subagent(&self, session_id: &str) -> Option<&HaloAgentSubagent> {
        self.subagents.iter().find(|s| s.session_id == session_id)
    }

    /// Approvals shown above the composer: the root turn wins, then the first
    /// waiting child (children never overwrite the root's own request).
    pub fn visible_pending_input(&self) -> Option<HaloAgentPendingInput> {
        self.pending_input
            .clone()
            .or_else(|| self.subagents.iter().find_map(|s| s.pending_input.clone()))
    }
}

pub fn agent_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn truncate_preview(text: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        if ch == '\n' {
            break;
        }
        out.push(ch);
    }
    out
}

/// Build a short session title from the first user text part.
pub fn agent_session_title_from_messages(messages: &[std::sync::Arc<HaloAgentMessage>]) -> String {
    let text = messages.iter().find_map(|m| {
        if m.role != HaloAgentRole::User {
            return None;
        }
        m.parts.iter().find_map(|p| match p {
            HaloAgentPart::Text(t) if !t.trim().is_empty() => Some(t.trim()),
            _ => None,
        })
    });
    text.map(|t| truncate_preview(t, 48)).unwrap_or_default()
}

/// Latest text part for the Sessions row subtitle.
pub fn agent_session_preview_from_messages(
    messages: &[std::sync::Arc<HaloAgentMessage>],
) -> String {
    let text = messages.iter().rev().find_map(|m| {
        m.parts.iter().rev().find_map(|p| match p {
            HaloAgentPart::Text(t) if !t.trim().is_empty() => Some(t.trim()),
            HaloAgentPart::Error(t) if !t.trim().is_empty() => Some(t.trim()),
            _ => None,
        })
    });
    text.map(|t| truncate_preview(t, 64)).unwrap_or_default()
}

/// Whether the composer popup is completing a file (`@`) or a skill (`/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HaloAgentCompletionKind {
    File,
    Skill,
}

/// One row in the `@` / `/` completion popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaloAgentCompletionItem {
    pub label: String,
    pub detail: String,
    /// Text that replaces the trigger token (`@path` or `/skill`).
    pub insert: String,
    pub is_dir: bool,
}

/// Live `@` file / `/` skill completion above the agent composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaloAgentCompletion {
    pub kind: HaloAgentCompletionKind,
    pub query: String,
    /// UTF-8 byte range of the trigger token in the draft.
    pub token_start: usize,
    pub token_end: usize,
    pub selected: usize,
    pub items: Vec<HaloAgentCompletionItem>,
}

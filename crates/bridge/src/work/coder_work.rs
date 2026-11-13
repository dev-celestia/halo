//! Coder work-tab state: editor panes, terminals, and agent sessions.

use super::*;

/// Language-server connection status for the Coder status bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoderLspStatus {
    /// No language server for the current workspace / language.
    Idle,
    Starting,
    /// Server is loading workspace metadata, indexing, or running background checks (`$/progress`).
    Indexing(String),
    Ready {
        errors: u32,
        warnings: u32,
    },
    /// Server missing or failed — editors fell back to plaintext (no providers).
    Unavailable(String),
    Failed(String),
}

impl Default for CoderLspStatus {
    fn default() -> Self {
        Self::Idle
    }
}

impl CoderLspStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Idle => "LSP".into(),
            Self::Starting => "LSP…".into(),
            Self::Indexing(detail) => {
                let detail = detail.trim();
                if detail.is_empty() {
                    "LSP…".into()
                } else {
                    format!("LSP: {detail}")
                }
            }
            Self::Ready { errors, warnings } => {
                if *errors == 0 && *warnings == 0 {
                    "LSP".into()
                } else {
                    format!("LSP E{errors} W{warnings}")
                }
            }
            // Soft fallback — do not sticky-show "LSP err".
            Self::Unavailable(_) => "LSP".into(),
            Self::Failed(_) => "LSP err".into(),
        }
    }

    /// True when no language server is available (syntax highlighting still applies).
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }

    #[deprecated(note = "use is_unavailable — highlighting is kept when LSP is down")]
    pub fn is_plaintext_fallback(&self) -> bool {
        self.is_unavailable()
    }
}

/// Detected project type for the open Coder root (drives Run).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoderProjectKind {
    Rust,
    Node,
    /// Both `Cargo.toml` and `package.json` — Run follows the focused file.
    Both,
}

impl CoderProjectKind {
    pub fn is_runnable(self) -> bool {
        matches!(self, Self::Rust | Self::Node | Self::Both)
    }

    pub fn has_rust(self) -> bool {
        matches!(self, Self::Rust | Self::Both)
    }

    pub fn has_node(self) -> bool {
        matches!(self, Self::Node | Self::Both)
    }
}

/// Engine install, macOS grants, and the lease held by the active chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopStatus {
    pub installed: bool,
    pub version: Option<String>,
    /// Install hint or version line for the settings row.
    pub detail: String,
    /// `None` is unknown, not denied.
    pub accessibility: Option<bool>,
    pub screen_recording: Option<bool>,
    pub lease_held: bool,
    pub lease_app: Option<String>,
}

impl Default for DesktopStatus {
    fn default() -> Self {
        Self {
            installed: false,
            version: None,
            detail: "Not installed".into(),
            accessibility: None,
            screen_recording: None,
            lease_held: false,
            lease_app: None,
        }
    }
}

/// Kind of in-tree name edit in the Coder explorer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoderExplorerInlineKind {
    CreateFile,
    CreateFolder,
    Rename { from: String },
}

/// Active inline create/rename row in the Coder explorer (VS Code-style).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderExplorerInline {
    /// Directory that will contain the new entry, or the parent of the rename target.
    pub parent_dir: String,
    pub kind: CoderExplorerInlineKind,
}

/// Coder IDE workspace state for one work tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderWork {
    pub root_path: Option<String>,
    /// Cached project type for the open root (e.g. Rust when `Cargo.toml` exists).
    pub project_kind: Option<CoderProjectKind>,
    /// True while a Play-started run is considered active (UI toggle; cleared on Stop).
    pub run_active: bool,
    /// Activity-rail Run/Build panel open beside the hammer control.
    pub run_panel_open: bool,
    /// When set (non-empty after trim), replaces auto-detected Run command.
    pub run_command_override: Option<String>,
    /// When set (non-empty after trim), replaces auto-detected Build command.
    pub build_command_override: Option<String>,
    /// Top-level entries under `root_path` (lazy children on folders).
    pub tree: Vec<CoderTreeEntry>,
    /// Absolute paths currently expanded in the file tree.
    pub expanded: Vec<String>,
    /// When set, the explorer shows an inline name field (create or rename).
    pub explorer_inline: Option<CoderExplorerInline>,
    /// Open markdown paths currently shown as rendered preview (not source).
    pub markdown_preview_paths: Vec<String>,
    pub open_files: Vec<CoderOpenFile>,
    /// Editor panes (leaves). Indexed by [`CoderPane::id`] into the editor pool.
    pub panes: Vec<CoderPane>,
    /// Recursive split tree; every leaf id must exist in `panes`.
    pub layout: CoderLayout,
    /// Pane that receives open/save and keyboard focus.
    pub focused_pane: u8,
    pub undo_stack: Vec<CoderFsOp>,
    pub redo_stack: Vec<CoderFsOp>,
    /// Which activity-bar view is selected.
    pub sidebar: CoderSidebar,
    /// When false, only the activity bar shows (panel collapsed).
    pub sidebar_open: bool,
    /// Bottom terminal dock visibility.
    pub terminal_open: bool,
    /// Open terminal tabs (each maps to a backend PTY session).
    pub terminals: Vec<CoderTerminal>,
    /// Active terminal tab id.
    pub active_terminal_id: Option<String>,
    /// Monotonic counter for default titles (`Terminal 1`, …).
    pub terminal_seq: u32,
    /// Right-side Halo Agent panel visibility.
    pub agent_open: bool,
    pub agent_running: bool,
    pub agent_run_id: Option<String>,
    pub agent_harness: String,
    pub agent_model: Option<String>,
    /// The model **each** harness last ran. A model ID only means something
    /// inside the harness that served it: `opencode/beam-free` is a real
    /// OpenCode model and a provider Pi knows but has no credentials for, so
    /// carrying one selection across a switch started a turn that could only
    /// die with `Unauthorized`.
    pub agent_model_by_harness: std::collections::HashMap<String, String>,
    pub agent_harness_installed: bool,
    /// Human-readable reason when Pi is missing / wrong version / probe failed.
    pub agent_pi_setup_hint: Option<String>,
    /// True when a Pi binary exists but is not the pinned 1.4.0 integration.
    pub agent_pi_needs_upgrade: bool,
    /// True when the harness binary works but its own state directory is not
    /// writable, so the setup card must offer the ownership repair rather than
    /// a reinstall.
    pub agent_state_blocked: bool,
    /// Which harness install/upgrade is in flight (`pi` / `opencode` / `antigravity` / `cursor`).
    pub agent_installing: Option<String>,
    /// Live human-readable install stage (e.g., `"Downloading (42%)…"`).
    pub agent_install_stage: Option<String>,
    pub agent_install_error: Option<String>,
    /// Selected provider tab in the inline API key setup card (`anthropic`, `openai`, `gemini`, `deepseek`, `ollama`).
    pub agent_inline_key_provider: String,
    /// Cached harness rows for the picker (refreshed on open / harness change).
    pub agent_harness_catalog: Vec<halo_backend::commands::halo_agent::HarnessInfo>,
    /// Per-harness install and sign-in snapshot for the provider sheet.
    /// Filled off the UI thread when the sheet opens; not written to disk.
    pub agent_harness_readiness: Vec<halo_backend::commands::halo_agent::HarnessReadiness>,
    pub agent_harness_readiness_loading: bool,
    pub agent_harness_readiness_at: Option<std::time::Instant>,
    pub agent_harness_readiness_epoch: u64,
    /// Explicit hold: the header opened the sheet, or a row action still needs it.
    /// The sheet also shows, without this flag, when the selected harness is
    /// not usable and the user has not dismissed it.
    pub agent_provider_sheet_open: bool,
    pub agent_provider_sheet_dismissed: bool,
    /// Harness the sheet radio shows before [`Self::agent_harness`] catches up.
    pub agent_provider_pending_harness: Option<String>,
    /// Harness whose Install or Upgrade control shows busy before the bridge flag flips.
    pub agent_provider_pending_action: Option<String>,
    /// Harness whose install failed. Cleared on the next successful install.
    pub agent_install_error_harness: Option<String>,
    /// Cached models for the current harness (same refresh cadence).
    pub agent_model_catalog: Vec<halo_backend::commands::halo_agent::ModelInfo>,
    /// Last known models **per harness**, so switching back to a harness shows
    /// its list immediately instead of clearing the picker and re-probing
    /// (`opencode models` is a process spawn that can take seconds).
    pub agent_model_cache:
        std::collections::HashMap<String, Vec<halo_backend::commands::halo_agent::ModelInfo>>,
    /// When each harness's [`Self::agent_model_cache`] entry was last probed.
    /// Used to skip re-spawning CLIs on every picker open while the list is
    /// still fresh; the strip's refresh button always re-probes.
    pub agent_catalog_fetched_at: std::collections::HashMap<String, std::time::Instant>,
    /// True while a background catalog probe/`opencode models` is in flight.
    /// The model picker opens immediately and shows this inside the card.
    pub agent_catalog_loading: bool,
    /// Monotonic id so a superseded refresh cannot clear a newer load.
    pub agent_catalog_refresh_epoch: u64,
    /// Project path being hydrated in the background after an Agent rail switch.
    /// The accordion expands and chat switches immediately; explorer/git fill in
    /// when [`None`] again.
    pub agent_project_loading: Option<String>,
    /// Monotonic id so a superseded project hydrate cannot clobber a newer switch.
    pub agent_project_switch_epoch: u64,
    /// All chat sessions for this project tab (active included).
    pub agent_sessions: Vec<HaloAgentSession>,
    /// Session whose transcript is shown in the panel.
    pub agent_active_session_id: String,
    /// History popover open above the transcript.
    pub agent_history_open: bool,
    /// Collapsed project keys in the Agent sessions rail (`""` = "No project").
    pub agent_rail_collapsed: Vec<String>,
    /// Right-side Context rail visibility (Artifacts, skills, MCP, sources).
    pub agent_context_open: bool,
    /// Skill catalog for the composer and Context rail.
    pub agent_skills: Vec<halo_backend::commands::halo_agent::SkillInfo>,
    pub agent_skills_loading: bool,
    pub agent_skills_error: Option<String>,
    /// Monotonic id so a superseded discovery cannot clobber a newer load.
    pub agent_skills_epoch: u64,
    /// Skill ids enabled for the active chat.
    pub agent_enabled_skills: Vec<String>,
    /// MCP server names enabled for the active chat.
    pub agent_enabled_mcp: Vec<String>,
    /// Browser tooling availability, resolved off the UI thread.
    pub agent_browser: halo_backend::commands::halo_agent::BrowserStatus,
    pub agent_browser_loading: bool,
    /// Desktop engine, grants, and whether this chat holds the lease.
    pub agent_desktop: DesktopStatus,
    pub agent_desktop_loading: bool,
    /// Todo tray opened from the one-line summary while an approval is up.
    pub agent_dock_todo_open: bool,
    /// Queue tray opened from the one-line summary while an approval is up.
    pub agent_dock_queue_open: bool,
    /// Approval request the dock summaries were collapsed for.
    pub agent_dock_pending_id: Option<String>,
    pub agent_messages: crate::HaloAgentMessages,
    /// The exact currently streaming UI message and part (never historical reasoning).
    pub agent_streaming_part: Option<(String, usize)>,
    pub agent_draft: String,
    pub agent_todos: Vec<HaloAgentTodo>,
    pub agent_todo_expanded: bool,
    pub agent_todo_dismissed: bool,
    /// Expanded Context rail section keys (`session`, `subagents`, `skills`, …).
    /// Empty means every section starts collapsed.
    pub agent_context_expanded: Vec<String>,
    pub agent_queue: Vec<HaloAgentQueuedMessage>,
    /// Write/Edit file paths from the active session (Artifacts rail).
    pub agent_file_changes: Vec<HaloAgentFileChange>,
    /// When true, show harness/model picker popover.
    pub agent_picker_open: bool,
    /// Picker filter: `None` = All, `Some("recent")`, or a provider id (`anthropic`, …).
    pub agent_picker_filter: Option<String>,
    /// Recently selected model ids for the current process (most recent first, capped).
    pub agent_recent_models: Vec<String>,
    /// When true, show the per-session approval-mode flyout on the composer.
    pub agent_approval_picker_open: bool,
    /// When true, the next agent run uses plan mode (explore / propose, no edits).
    pub agent_plan_mode: bool,
    /// Active session approval mode (mirrored from [`HaloAgentSession::approval_mode`]).
    pub agent_approval_mode: AgentApprovalMode,
    /// `@` file / `/` skill completion popup above the composer.
    pub agent_completion: Option<HaloAgentCompletion>,
    /// Staged image paths for the next agent turn (png/jpeg/gif/webp).
    pub agent_attachments: Vec<String>,
    /// Human approver / question panel.
    pub agent_pending_input: Option<HaloAgentPendingInput>,
    /// Child agents for the active chat (task / subagent tool calls).
    pub agent_subagents: Vec<HaloAgentSubagent>,
    /// Child session id whose drawer is open (`None` = drawer closed).
    pub agent_open_subagent: Option<String>,
    /// Latest context window fill for the composer meter.
    pub agent_context_usage: Option<HaloAgentContextUsage>,
    /// Language server status for the active workspace.
    pub lsp: CoderLspStatus,
    /// Quick Open (Go to File, ⌘P / Ctrl+P) modal visibility.
    pub quick_open_open: bool,
    /// Current search query in Quick Open.
    pub quick_open_query: String,
    /// Matched files from workspace search.
    pub quick_open_matches: Vec<halo_backend::commands::halo_agent::FileSearchMatch>,
    /// Currently highlighted index in `quick_open_matches`.
    pub quick_open_selected: usize,
    /// Ephemeral footer hint (e.g. "Saved path") — not toasted.
    pub status_message: Option<String>,
}

impl Default for CoderWork {
    fn default() -> Self {
        Self {
            root_path: None,
            project_kind: None,
            run_active: false,
            run_panel_open: false,
            run_command_override: None,
            build_command_override: None,
            tree: Vec::new(),
            expanded: Vec::new(),
            explorer_inline: None,
            markdown_preview_paths: Vec::new(),
            open_files: Vec::new(),
            panes: vec![CoderPane::new(0)],
            layout: CoderLayout::Leaf(0),
            focused_pane: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            sidebar: CoderSidebar::Explorer,
            sidebar_open: true,
            terminal_open: false,
            terminals: Vec::new(),
            active_terminal_id: None,
            terminal_seq: 0,
            agent_open: false,
            agent_running: false,
            agent_run_id: None,
            agent_harness: "pi".into(),
            // No invented default: the first model comes from the harness's own
            // catalog, which is the only list that can actually run.
            agent_model: None,
            agent_model_by_harness: std::collections::HashMap::new(),
            agent_harness_installed: false,
            agent_pi_setup_hint: None,
            agent_pi_needs_upgrade: false,
            agent_state_blocked: false,
            agent_installing: None,
            agent_install_stage: None,
            agent_install_error: None,
            agent_inline_key_provider: "anthropic".into(),
            agent_harness_catalog: Vec::new(),
            agent_harness_readiness: Vec::new(),
            agent_harness_readiness_loading: false,
            agent_harness_readiness_at: None,
            agent_harness_readiness_epoch: 0,
            agent_provider_sheet_open: false,
            agent_provider_sheet_dismissed: false,
            agent_provider_pending_harness: None,
            agent_provider_pending_action: None,
            agent_install_error_harness: None,
            agent_model_catalog: Vec::new(),
            agent_model_cache: std::collections::HashMap::new(),
            agent_catalog_fetched_at: std::collections::HashMap::new(),
            agent_catalog_loading: false,
            agent_catalog_refresh_epoch: 0,
            agent_project_loading: None,
            agent_project_switch_epoch: 0,
            agent_sessions: Vec::new(),
            agent_active_session_id: String::new(),
            agent_history_open: true,
            agent_rail_collapsed: Vec::new(),
            agent_context_open: true,
            agent_skills: Vec::new(),
            agent_skills_loading: false,
            agent_skills_error: None,
            agent_skills_epoch: 0,
            agent_enabled_skills: Vec::new(),
            agent_enabled_mcp: Vec::new(),
            agent_browser: Default::default(),
            agent_browser_loading: false,
            agent_desktop: DesktopStatus::default(),
            agent_desktop_loading: false,
            agent_dock_todo_open: false,
            agent_dock_queue_open: false,
            agent_dock_pending_id: None,
            agent_messages: Default::default(),
            agent_streaming_part: None,
            agent_draft: String::new(),
            agent_todos: Vec::new(),
            agent_todo_expanded: true,
            agent_todo_dismissed: false,
            agent_context_expanded: Vec::new(),
            agent_queue: Vec::new(),
            agent_file_changes: Vec::new(),
            agent_picker_open: false,
            agent_picker_filter: None,
            agent_recent_models: Vec::new(),
            agent_approval_picker_open: false,
            agent_plan_mode: false,
            agent_approval_mode: AgentApprovalMode::Ask,
            agent_completion: None,
            agent_attachments: Vec::new(),
            agent_pending_input: None,
            agent_subagents: Vec::new(),
            agent_open_subagent: None,
            agent_context_usage: None,
            lsp: CoderLspStatus::Idle,
            quick_open_open: false,
            quick_open_query: String::new(),
            quick_open_matches: Vec::new(),
            quick_open_selected: 0,
            status_message: None,
        }
    }
}

/// One terminal tab in the Coder bottom dock.
///
/// Layout only. Runtime state — bytes, epoch, liveness, cwd — lives in
/// [`crate::TerminalStore`] keyed by `id`, so it survives tab switches and is
/// never clobbered by `load_active_tab`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderTerminal {
    pub id: String,
    pub title: String,
    /// Interactive shell vs. the static, read-only Git Log view.
    pub kind: CoderTerminalKind,
}

/// What a terminal tab displays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoderTerminalKind {
    /// Interactive PTY shell.
    Shell,
    /// Read-only `git log` text — no PTY, no keyboard input.
    GitLog,
}

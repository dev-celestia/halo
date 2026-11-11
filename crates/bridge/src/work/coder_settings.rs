//! Coder IDE configuration shared by every Coder tab and the settings window.

/// One reversible filesystem mutation in the Coder explorer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoderFsOp {
    /// Soft-deleted: item moved from `path` to `trash_path`.
    /// Empty `trash_path` means "re-delete on redo" after a successful undo restore.
    SoftDelete {
        path: String,
        trash_path: String,
    },
    Rename {
        from: String,
        to: String,
    },
    /// Created file or folder; undo removes it.
    Create {
        path: String,
        is_dir: bool,
    },
}

/// Left activity-bar panel in the Coder IDE (VS Code–style).
///
/// Source Control is its own top-level tool ([`WorkKind::SourceControl`]), not
/// a Coder sidebar mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CoderSidebar {
    #[default]
    Explorer,
}

/// Line ending written for new files and on save.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoderLineEnding {
    #[default]
    Lf,
    Crlf,
}

impl CoderLineEnding {
    pub const ALL: [Self; 2] = [Self::Lf, Self::Crlf];

    pub fn id(self) -> &'static str {
        match self {
            Self::Lf => "lf",
            Self::Crlf => "crlf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
        }
    }
}

/// One language server row in Coder settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderLspServerRow {
    pub id: String,
    pub display_name: String,
    pub languages: String,
    pub command: String,
    pub installed: bool,
    pub install_hint: String,
    pub can_auto_install: bool,
}

/// In-flight Google Antigravity ACP browser OAuth (agent settings).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AntigravityLoginStatus {
    #[default]
    Idle,
    /// Waiting for the browser callback. `auth_url` is set once the helper emits it.
    Waiting {
        auth_url: Option<String>,
    },
    Failed {
        message: String,
    },
}

impl AntigravityLoginStatus {
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::Waiting { .. })
    }
}

/// How far an agent run may go without asking the human first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentApprovalMode {
    /// Ask before editing files or running commands.
    #[default]
    Ask,
    /// Edit files freely; ask before running commands.
    ApproveEdits,
    /// Ask only for actions that look unsafe.
    ApproveSafe,
    /// Any file, any command, the whole internet.
    FullAccess,
}

impl AgentApprovalMode {
    pub const ALL: [Self; 4] = [
        Self::Ask,
        Self::ApproveEdits,
        Self::ApproveSafe,
        Self::FullAccess,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::ApproveEdits => "approve_edits",
            Self::ApproveSafe => "approve_safe",
            Self::FullAccess => "full_access",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ask" => Some(Self::Ask),
            "approve_edits" => Some(Self::ApproveEdits),
            "approve_safe" => Some(Self::ApproveSafe),
            "full_access" => Some(Self::FullAccess),
            _ => None,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Ask => "Ask for approval",
            Self::ApproveEdits => "Approve edits",
            Self::ApproveSafe => "Approve for me",
            Self::FullAccess => "Full access",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Ask => "Ask before editing files or running commands.",
            Self::ApproveEdits => "Edit files freely, ask before running commands.",
            Self::ApproveSafe => "Ask only for actions that look unsafe.",
            Self::FullAccess => "Any file, any command, the whole internet.",
        }
    }

    pub fn is_default(self) -> bool {
        matches!(self, Self::Ask)
    }

    /// Pi loads its approval hook for every mode. Plan mode limits the tool
    /// list instead of skipping the hook.
    pub fn needs_approval_hook(self) -> bool {
        match self {
            Self::Ask | Self::ApproveEdits | Self::ApproveSafe | Self::FullAccess => true,
        }
    }
}

/// IDE setup shared by every Coder tab.
///
/// Lives on [`BridgeState`](crate::BridgeState) rather than on a work tab so
/// the dedicated settings window and every editor pane read one copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderSettings {
    // ── Editor ─────────────────────────────────────────────────────────────
    /// Selected id from [`crate::coder_fonts`] (`assets/coder-fonts.json`).
    pub font_id: String,
    pub font_size: u16,
    pub tab_size: u8,
    pub word_wrap: bool,
    pub line_numbers: bool,
    pub minimap: bool,
    pub autosave: bool,
    pub trim_trailing_whitespace: bool,
    pub format_on_save: bool,
    pub line_ending: CoderLineEnding,
    // ── Terminal ───────────────────────────────────────────────────────────
    /// Empty means "inherit `$SHELL`".
    pub shell: String,
    pub terminal_font_size: u16,
    pub scrollback_lines: u32,
    pub confirm_close_running: bool,
    /// Directory a new shell starts in when no project folder is open.
    /// Empty means `$HOME`.
    pub default_cwd: String,
    // ── Agent ──────────────────────────────────────────────────────────────
    pub agent_approval_mode: AgentApprovalMode,
    /// Play chimes when a run finishes, asks a question, or fails.
    pub agent_sound_enabled: bool,
    pub agent_max_turns: u16,
    pub agent_include_open_files: bool,
    pub agent_include_git_diff: bool,
    /// Master switch for desktop computer use. Off by default.
    pub computer_use: bool,
    /// Physical pointer and keyboard. Stays off until its own switch is on.
    pub computer_use_headed: bool,
    pub pi_executable_path: Option<String>,
    /// Optional absolute path to the `opencode` binary (OpenCode harness).
    pub opencode_executable_path: Option<String>,
    pub antigravity_executable_path: Option<String>,
    /// MCP servers available to chats (Settings → Agent). Only OpenCode runs
    /// receive them; a chat also has to enable each server.
    pub mcp_servers: Vec<halo_backend::commands::halo_agent::McpServerConfig>,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub deepseek_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
    // ── Source control ─────────────────────────────────────────────────────
    pub sign_off: bool,
    /// `0` disables background fetching.
    pub auto_fetch_minutes: u16,
    pub confirm_force_push: bool,
    /// Ask before Doctor changes anything on the machine (installs, ownership
    /// repairs, cache reclaims). On by default: those actions touch paths
    /// outside the app's own data directory.
    pub confirm_system_changes: bool,
    // ── Workspace ──────────────────────────────────────────────────────────
    pub restore_last_project: bool,
    pub show_hidden_files: bool,
    /// Newline-separated globs kept out of the explorer and search.
    pub ignore_globs: String,
}

impl Default for CoderSettings {
    fn default() -> Self {
        Self {
            font_id: crate::coder_fonts::default_font_id().to_string(),
            font_size: 13,
            tab_size: 4,
            word_wrap: true,
            line_numbers: true,
            minimap: false,
            autosave: false,
            trim_trailing_whitespace: true,
            format_on_save: false,
            line_ending: CoderLineEnding::Lf,
            shell: default_shell(),
            terminal_font_size: 13,
            scrollback_lines: 5_000,
            confirm_close_running: true,
            default_cwd: String::new(),
            agent_approval_mode: AgentApprovalMode::Ask,
            agent_sound_enabled: true,
            agent_max_turns: 12,
            agent_include_open_files: true,
            agent_include_git_diff: true,
            computer_use: false,
            computer_use_headed: false,
            pi_executable_path: None,
            opencode_executable_path: None,
            antigravity_executable_path: None,
            mcp_servers: Vec::new(),
            anthropic_api_key: None,
            openai_api_key: None,
            gemini_api_key: None,
            deepseek_api_key: None,
            ollama_base_url: None,
            sign_off: false,
            auto_fetch_minutes: 0,
            confirm_force_push: true,
            confirm_system_changes: true,
            restore_last_project: true,
            show_hidden_files: false,
            ignore_globs: "target\nnode_modules\ndist\n.git".into(),
        }
    }
}

impl CoderSettings {
    /// Configured starting directory for new shells, trimmed.
    ///
    /// `None` when unset, so callers can fall through to `$HOME`.
    pub fn effective_cwd(&self) -> Option<&str> {
        let dir = self.default_cwd.trim();
        if dir.is_empty() { None } else { Some(dir) }
    }

    /// Default ignore globs as a single-line summary for the settings row.
    pub fn ignore_globs_summary(&self) -> String {
        let globs: Vec<&str> = self
            .ignore_globs
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if globs.is_empty() {
            "No ignore rules".into()
        } else {
            globs.join(", ")
        }
    }
}

fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())
    }
}

//! Persisted editor layout types (panes, tabs, and IDE settings).

use super::*;

/// One editor leaf in a persisted layout.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoderPersistedPane {
    pub id: u8,
    #[serde(default)]
    pub tabs: Vec<String>,
    pub active: Option<String>,
}

/// Recursive layout node. Legacy two-pane fields below still deserialize for migration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoderPersistedLayout {
    Leaf {
        id: u8,
    },
    Split {
        axis: String,
        first: Box<CoderPersistedLayout>,
        second: Box<CoderPersistedLayout>,
    },
}

/// Editor tabs remembered for one project root (paths only — buffers reload from disk).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoderPersistedTabs {
    /// Legacy single/dual-pane fields — still read for migration.
    #[serde(default)]
    pub primary_tabs: Vec<String>,
    pub active_path: Option<String>,
    #[serde(default)]
    pub secondary_tabs: Vec<String>,
    pub secondary_active: Option<String>,
    /// `"horizontal"` or `"vertical"` when a split is open (legacy).
    pub split_axis: Option<String>,
    #[serde(default)]
    pub focused_group: u8,
    /// Nested pane layout (preferred).
    #[serde(default)]
    pub panes: Vec<CoderPersistedPane>,
    pub layout: Option<CoderPersistedLayout>,
    #[serde(default)]
    pub focused_pane: u8,
}

/// IDE setup remembered across launches (editor, terminal, agent, git, workspace).
///
/// Stored under [`CoderPrefs::settings`]. Missing fields keep serde defaults so
/// older `coder.json` files keep loading after new knobs land.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoderPersistedSettings {
    #[serde(default)]
    pub font_id: String,
    #[serde(default = "default_font_size")]
    pub font_size: u16,
    #[serde(default = "default_tab_size")]
    pub tab_size: u8,
    #[serde(default = "default_true")]
    pub word_wrap: bool,
    #[serde(default = "default_true")]
    pub line_numbers: bool,
    #[serde(default)]
    pub minimap: bool,
    #[serde(default)]
    pub autosave: bool,
    #[serde(default = "default_true")]
    pub trim_trailing_whitespace: bool,
    #[serde(default)]
    pub format_on_save: bool,
    /// `"lf"` or `"crlf"`.
    #[serde(default = "default_line_ending")]
    pub line_ending: String,
    #[serde(default)]
    pub shell: String,
    #[serde(default = "default_font_size")]
    pub terminal_font_size: u16,
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: u32,
    #[serde(default = "default_true")]
    pub confirm_close_running: bool,
    /// Directory a new shell starts in when no project folder is open.
    /// Empty = `$HOME`.
    #[serde(default)]
    pub default_cwd: String,
    /// `"ask"` | `"approve_edits"` | `"approve_safe"` | `"full_access"`.
    #[serde(default)]
    pub agent_approval_mode: Option<String>,
    /// Legacy flag — migrated into [`Self::agent_approval_mode`] on load.
    #[serde(default)]
    pub agent_auto_approve: bool,
    #[serde(default = "default_true")]
    pub agent_sound_enabled: bool,
    #[serde(default = "default_agent_max_turns")]
    pub agent_max_turns: u16,
    #[serde(default = "default_true")]
    pub agent_include_open_files: bool,
    #[serde(default = "default_true")]
    pub agent_include_git_diff: bool,
    /// Desktop computer use. Missing keys stay off.
    #[serde(default)]
    pub computer_use: bool,
    /// Physical pointer and keyboard. Independent of [`Self::computer_use`].
    #[serde(default)]
    pub computer_use_headed: bool,
    #[serde(default)]
    pub pi_executable_path: Option<String>,
    #[serde(default)]
    pub opencode_executable_path: Option<String>,
    #[serde(default)]
    pub antigravity_executable_path: Option<String>,
    /// MCP servers the user configured in Settings → Agent. Chats opt in
    /// per server; only OpenCode consumes them today.
    #[serde(default)]
    pub mcp_servers: Vec<halo_harness::McpServerConfig>,
    #[serde(default)]
    pub anthropic_api_key: Option<String>,
    #[serde(default)]
    pub openai_api_key: Option<String>,
    #[serde(default)]
    pub gemini_api_key: Option<String>,
    #[serde(default)]
    pub deepseek_api_key: Option<String>,
    #[serde(default)]
    pub ollama_base_url: Option<String>,
    #[serde(default)]
    pub sign_off: bool,
    #[serde(default)]
    pub auto_fetch_minutes: u16,
    #[serde(default = "default_true")]
    pub confirm_force_push: bool,
    /// Ask before Doctor changes anything outside the app's own data directory.
    /// Defaults to `true`, so a settings file written before this existed gets
    /// the safe behaviour.
    #[serde(default = "default_true")]
    pub confirm_system_changes: bool,
    #[serde(default = "default_true")]
    pub restore_last_project: bool,
    #[serde(default)]
    pub show_hidden_files: bool,
    #[serde(default = "default_ignore_globs")]
    pub ignore_globs: String,
}

fn default_true() -> bool {
    true
}
fn default_font_size() -> u16 {
    13
}
fn default_tab_size() -> u8 {
    4
}
fn default_line_ending() -> String {
    "lf".into()
}
fn default_scrollback_lines() -> u32 {
    5_000
}
fn default_agent_max_turns() -> u16 {
    12
}
fn default_ignore_globs() -> String {
    "target\nnode_modules\ndist\n.git".into()
}

impl Default for CoderPersistedSettings {
    fn default() -> Self {
        Self {
            font_id: String::new(),
            font_size: default_font_size(),
            tab_size: default_tab_size(),
            word_wrap: true,
            line_numbers: true,
            minimap: false,
            autosave: false,
            trim_trailing_whitespace: true,
            format_on_save: false,
            line_ending: default_line_ending(),
            shell: String::new(),
            terminal_font_size: default_font_size(),
            scrollback_lines: default_scrollback_lines(),
            confirm_close_running: true,
            default_cwd: String::new(),
            agent_approval_mode: None,
            agent_auto_approve: false,
            agent_sound_enabled: true,
            agent_max_turns: default_agent_max_turns(),
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
            ignore_globs: default_ignore_globs(),
        }
    }
}

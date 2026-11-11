//! Installs the backend and exposes UI-facing command + reactive state APIs.
//!
//! Replaces Tauri `invoke` / `emit`:
//! - UI calls [`Bridge`] command methods
//! - UI observes [`BridgeState`] via [`celestia_ui::state::StoreContext`]

use std::sync::Arc;
use std::time::Duration;

use celestia_ui::components::CodeEditor;
use celestia_ui::state::{StoreContext, StoreHandle};
use gpui::{
    App, AppContext as _, AsyncApp, ClipboardEntry, ClipboardItem, Entity, Global, Image,
    ImageFormat, PathPromptOptions, PromptLevel, Window,
};

use self::comparer::ComparerView;
use self::file_explorer::FileExplorerView;
use self::r2::R2View;
use self::wordlists::WordlistsView;
use crate::backend::Backend;
use crate::craft::{expand_request, parse_context_variables};
use crate::error::{BridgeError, BridgeResult};
use crate::native::{summary_to_row, NativeBackend};
use crate::terminal::TerminalStore;
use crate::types::{
    ActiveRequestState, BackendEvent, ChronicleLogDto, CollectionDto, ContextDto,
    CrawlerCrawlConfig, CrawlerInsightDto, CrawlerLogDto, CrawlerPageDto, CrawlerSessionDto,
    CrawlerStatus, CrawlerViewTab, DebugLogEntry, DebugLogLevel, DetailSide, DetailTab,
    ForgeActiveView, ForgeRequestTab, ForgeResponseTab, GitOutputEntry, HistoryFilter,
    HttpLogDetail, HttpLogRow, HttpSessionDto, InterceptSnapshot, IntruderDraft, IntruderResultRow,
    IntruderStatus, MockForgeView, MockLogDto, MockServerStatusDto, NoteRecord, NotesFilterTab,
    NotesSortOption, NotesViewMode, PausedRow, PortScanRequestDto, PortScanResultDto,
    PortScannerView, ProxyStatus, RepeaterRequestDto, RepeaterResponseDto, SettingsTab, StashDto,
    StashEndpointDto, StorageInfo,
};
use crate::work::{
    work_title, ActivitySession, CoderExplorerInline, CoderExplorerInlineKind, CoderFileKind,
    CoderFsOp, CoderLayout, CoderLineEnding, CoderOpenFile, CoderPane, CoderSettings,
    CoderSidebar, CoderSplitAxis, CoderTreeEntry, CoderWork, GitDiff, HaloAgentMessage,
    HaloAgentPart, HaloAgentQueuedMessage, HaloAgentRole, HaloAgentSessionStatus, WorkData,
    WorkKind, WorkTab,
};
use halo_backend::commands::halo_agent::{AgentEvent, DoneStatus, ToolCall};
use halo_backend::commands::shell::ShellAppearance;

const HISTORY_LIMIT: u32 = 500;

/// Reactive snapshot the UI reads and re-renders from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeState {
    pub proxy: ProxyStatus,
    /// True while start/stop is running off the UI thread (show loading).
    pub proxy_busy: bool,
    pub history: Vec<HttpLogRow>,
    pub history_filter: HistoryFilter,
    pub selected_log_id: Option<String>,
    pub selected_detail: Option<HttpLogDetail>,
    /// True while a history row context menu is open — freezes the live stream.
    pub history_context_menu_open: bool,
    /// Proxy-record events held back while the stream is paused (selection or menu).
    pub history_pending_count: u32,
    pub detail_tab: DetailTab,
    pub detail_side: DetailSide,
    pub sessions: Vec<HttpSessionDto>,
    pub intercept: InterceptSnapshot,
    pub paused: Vec<PausedRow>,
    pub selected_paused_id: Option<String>,
    pub repeater_response: Option<RepeaterResponseDto>,
    /// Exact request that was sent (post variable expansion) for the Request response tab.
    pub last_sent_request: Option<RepeaterRequestDto>,
    /// When set, the UI should load this into Repeater editors and navigate.
    pub pending_repeater: Option<RepeaterRequestDto>,
    /// When set, the UI should load this into Intruder editors and navigate.
    pub pending_intruder: Option<IntruderDraft>,
    /// Design-mode change brief waiting to be dropped into the agent composer.
    pub pending_agent_prompt: Option<String>,
    pub intruder: IntruderStatus,
    pub crawler: CrawlerStatus,
    pub coder: CoderWork,
    /// Runtime PTY buffers for every live terminal, keyed by session id.
    ///
    /// Lives outside [`WorkData`] so background output survives work-tab
    /// switches, splits, and multiple windows. Reads are cursor-based.
    /// Used by the Coder dock terminal (and any other shell hosts).
    pub terminal_sessions: TerminalStore,
    /// Recently opened Coder project folders (most recent first).
    pub coder_projects: Vec<String>,
    /// IDE setup shared by every Coder tab (Settings → Coder / Agent).
    pub coder_settings: CoderSettings,
    /// Browser OAuth progress for the Antigravity ACP harness (`agy_acp_server`).
    pub antigravity_acp_login: crate::work::AntigravityLoginStatus,
    /// Cached answer to "does the ACP server hold Google credentials?".
    /// Refreshed off the UI thread — never probe the keychain during paint.
    pub antigravity_acp_signed_in: bool,
    /// Snapshot of known language servers for the Language Servers settings UI.
    pub coder_lsp_servers: Vec<crate::work::CoderLspServerRow>,
    /// Server id currently being installed, if any.
    pub coder_lsp_installing_id: Option<String>,
    /// Last install / refresh message for the Language Servers section.
    pub coder_lsp_install_message: Option<String>,
    /// When set with `last_error`, the UI toast offers one-click install for this server id.
    pub coder_lsp_offer_install: Option<String>,
    /// Button label for [`Self::coder_lsp_offer_install`] (e.g. `"Install Rust"`).
    pub coder_lsp_offer_install_label: Option<String>,
    // ── Forge / folders ─────────────────────────────────────────────────
    pub collections: Vec<CollectionDto>,
    pub active_collection_id: Option<String>,
    pub stashes: Vec<StashDto>,
    pub endpoints: Vec<StashEndpointDto>,
    pub contexts: Vec<ContextDto>,
    pub chronicle: Vec<ChronicleLogDto>,
    pub selected_node_id: Option<String>,
    pub active_context_id: Option<String>,
    pub env_modal_open: bool,
    pub active_request: ActiveRequestState,
    pub forge_view: ForgeActiveView,
    pub forge_request_tab: ForgeRequestTab,
    pub forge_response_tab: ForgeResponseTab,
    pub tree_filter: String,
    /// Folder IDs currently collapsed in the Repeater accordion.
    /// Absent IDs are expanded (default: all open).
    pub collapsed_stashes: Vec<String>,
    pub last_message: Option<String>,
    pub last_error: Option<String>,
    /// `true` when `last_error` is a pointer to a log (Git Output) rather than
    /// the failure itself — the UI shows it as an info toast, not a red one.
    pub last_error_info: bool,
    /// Set when the app booted into the stub backend: the error that stopped
    /// the native backend. `None` means the real backend is installed.
    pub backend_failure: Option<String>,
    /// Ring buffer for the in-app debugger Logs tab (newest at the end).
    pub debug_logs: Vec<DebugLogEntry>,
    /// Open tool tabs. Each one stores that tool's working data.
    pub work_tabs: Vec<WorkTab>,
    pub active_work_tab_id: Option<String>,
    /// Workspaces. Each one holds its own tabs and captured activity.
    pub activity_sessions: Vec<ActivitySession>,
    pub active_activity_session_id: Option<String>,
    // ── Settings ───────────────────────────────────────────────────────────
    pub settings_tab: SettingsTab,
    /// Interface typeface + size (Settings → Appearance → Typography).
    /// Persisted in `shell.json` with the tab layout.
    pub appearance: ShellAppearance,
    /// Tools pinned as icons on the shell desktop (persisted in `shell.json`).
    /// Membership decides which apps the desktop shows; order is canonical
    /// (`WorkKind::ALL`) at render time.
    pub pinned_apps: Vec<WorkKind>,
    /// First-run bottom-right stepper finished or dismissed (`shell.json`).
    pub onboarding_completed: bool,
    /// Current onboarding step while the card is open (0 = welcome).
    pub onboarding_step: u8,
    pub preferred_http_port: u16,
    pub preferred_tls_port: u16,
    pub storage: Option<StorageInfo>,
    pub storage_busy: bool,
    /// Cached `git status` for the open project (Source Control + explorer).
    pub git: crate::work::GitState,
    /// Cached text for the read-only Git Log terminal tab.
    pub git_log: Option<String>,
    /// Diff preview open in the Source Control panel.
    pub git_diff: Option<GitDiff>,
    /// Split (side-by-side) layout for the diff pane; `false` = unified.
    /// Defaults to split — the git review workspace is wide enough for it.
    pub git_diff_split: bool,
    /// Soft-wrap long diff lines inside each pane. When false, each pane
    /// scrolls horizontally on its own (like a split editor).
    pub git_diff_wrap: bool,
    /// Whether the Ignored section in Source Control is expanded.
    pub git_ignored_open: bool,
    /// Label of the remote operation currently running off the UI thread
    /// (`"Pushing to origin"`, `"Fetching"`, …). `None` when idle. Doubles as
    /// a re-entry guard so a double-click cannot start two pushes.
    pub git_busy: Option<String>,
    /// Paths ticked in the Source Control panel for bulk stage/unstage.
    /// Pruned on every refresh so a renamed or committed path cannot linger.
    pub git_selection: Vec<String>,
    /// The inline "new branch" field is open in the Source Control panel.
    pub git_new_branch: bool,
    /// Activity-rail selection on the Source Control page.
    pub source_control_pane: crate::work::SourceControlPane,
    /// Git Graph & Branch Explorer state.
    pub git_graph: crate::work::GitGraphState,
    /// Ring of failed git commands shown in the Source Control Git Output dock
    /// (newest at the end).
    pub git_output: Vec<GitOutputEntry>,
    /// Whether the Source Control Git Output dock is expanded.
    pub git_output_open: bool,
    /// Pull requests loaded for the open project (`gh pr list`).
    pub pulls: crate::work::PullRequestView,
    /// Bumps on each pull-request fetch so a stale worker cannot overwrite a newer one.
    pub pulls_epoch: u64,
    pub ca_busy: bool,
    /// API Mock + API Override shared view (domains, routes, logs, server).
    pub mock: MockForgeView,
    /// JWT toolkit state (decode / audit / generate).
    pub jwt: crate::bridge::jwt::JwtView,
    /// Port Scanner page state.
    pub port_scanner: PortScannerView,
    /// Comparer page state (inputs + computed diff rows).
    pub comparer: ComparerView,
    /// File Explorer page state (sandboxed local FS).
    pub file_explorer: FileExplorerView,
    /// Wordlists app state (bundled catalog + remote manifest).
    pub wordlists: WordlistsView,
    /// R2 Storage app state (credentials, buckets, objects).
    pub r2: R2View,
    /// Split View layout + slot assignments (shared across the Split View tab).
    pub split_view: crate::work::SplitViewState,
    /// Doctor dependency health & installer state.
    pub doctor: doctor::DoctorView,
    /// 2D Diagram, Flowchart, and Table studio state.
    pub diagram: crate::work::DiagramWork,
    // ── Notes ──────────────────────────────────────────────────────────────
    pub notes: Vec<NoteRecord>,
    pub notes_loaded: bool,
    pub notes_open_tab_ids: Vec<String>,
    pub notes_active_id: Option<String>,
    pub notes_search_query: String,
    pub notes_view_mode: NotesViewMode,
    pub notes_library_open: bool,
    pub notes_library_search: String,
    pub notes_library_filter: NotesFilterTab,
    pub notes_library_sort: NotesSortOption,
    pub notes_editing_title_id: Option<String>,
    pub notes_drawing_open: bool,
    /// Bumped for every non-token update. Token deltas leave it unchanged so
    /// the shell can skip cloning this snapshot.
    pub shell_epoch: u64,
    /// `(chat_id, queue_item_id)` pairs waiting for the event pump to dispatch.
    /// Filled on successful `Done { Completed }` when the queue head is Pending;
    /// never set for Errored / Interrupted / Uncertain rows.
    pub(crate) agent_queue_drains: Vec<(String, String, bool)>,
}

impl Default for BridgeState {
    fn default() -> Self {
        let mut state = Self {
            proxy: ProxyStatus::default(),
            proxy_busy: false,
            history: Vec::new(),
            history_filter: HistoryFilter::default(),
            selected_log_id: None,
            selected_detail: None,
            history_context_menu_open: false,
            history_pending_count: 0,
            detail_tab: DetailTab::default(),
            detail_side: DetailSide::default(),
            sessions: Vec::new(),
            intercept: InterceptSnapshot::default(),
            paused: Vec::new(),
            selected_paused_id: None,
            repeater_response: None,
            last_sent_request: None,
            pending_repeater: None,
            pending_intruder: None,
            pending_agent_prompt: None,
            intruder: IntruderStatus::default(),
            crawler: CrawlerStatus::default(),
            coder: CoderWork::default(),
            terminal_sessions: TerminalStore::default(),
            coder_projects: Vec::new(),
            coder_settings: CoderSettings::default(),
            antigravity_acp_login: crate::work::AntigravityLoginStatus::default(),
            antigravity_acp_signed_in: false,
            coder_lsp_servers: Vec::new(),
            coder_lsp_installing_id: None,
            coder_lsp_install_message: None,
            coder_lsp_offer_install: None,
            coder_lsp_offer_install_label: None,
            collections: Vec::new(),
            active_collection_id: None,
            stashes: Vec::new(),
            endpoints: Vec::new(),
            contexts: Vec::new(),
            chronicle: Vec::new(),
            selected_node_id: None,
            active_context_id: None,
            env_modal_open: false,
            active_request: ActiveRequestState::default(),
            forge_view: ForgeActiveView::default(),
            forge_request_tab: ForgeRequestTab::default(),
            forge_response_tab: ForgeResponseTab::default(),
            tree_filter: String::new(),
            collapsed_stashes: Vec::new(),
            last_message: None,
            last_error: None,
            last_error_info: false,
            backend_failure: None,
            debug_logs: Vec::new(),
            work_tabs: Vec::new(),
            active_work_tab_id: None,
            activity_sessions: Vec::new(),
            active_activity_session_id: None,
            settings_tab: SettingsTab::General,
            appearance: ShellAppearance::default(),
            pinned_apps: WorkKind::ALL
                .into_iter()
                .filter(|kind| kind.ships())
                .collect(),
            onboarding_completed: false,
            onboarding_step: 0,
            preferred_http_port: 8888,
            preferred_tls_port: 8443,
            storage: None,
            storage_busy: false,
            git: crate::work::GitState::default(),
            git_log: None,
            git_diff: None,
            git_diff_split: true,
            git_diff_wrap: true,
            git_ignored_open: false,
            git_busy: None,
            git_selection: Vec::new(),
            git_new_branch: false,
            source_control_pane: crate::work::SourceControlPane::default(),
            git_graph: crate::work::GitGraphState::default(),
            git_output: Vec::new(),
            git_output_open: false,
            pulls: crate::work::PullRequestView::default(),
            pulls_epoch: 0,
            ca_busy: false,
            mock: MockForgeView::default(),
            jwt: Default::default(),
            port_scanner: PortScannerView::default(),
            comparer: ComparerView::default(),
            file_explorer: FileExplorerView::default(),
            wordlists: WordlistsView::default(),
            r2: R2View::default(),
            split_view: crate::work::SplitViewState::default(),
            doctor: doctor::DoctorView::default(),
            diagram: crate::work::DiagramWork::default(),
            notes: Vec::new(),
            notes_loaded: false,
            notes_open_tab_ids: Vec::new(),
            notes_active_id: None,
            notes_search_query: String::new(),
            notes_view_mode: NotesViewMode::Edit,
            notes_library_open: false,
            notes_library_search: String::new(),
            notes_library_filter: NotesFilterTab::All,
            notes_library_sort: NotesSortOption::UpdatedDesc,
            notes_editing_title_id: None,
            notes_drawing_open: false,
            shell_epoch: 0,
            agent_queue_drains: Vec::new(),
        };
        state.ensure_activity_session();
        state
    }
}

mod agent_state;
mod coder_autofill;
mod coder_backup;
mod coder_fs;
mod coder_lsp;
mod coder_quick_open;
mod coder_run;
mod coder_settings;
mod coder_term;
mod coder_undo;
pub mod comparer;
mod crawler;
pub mod diagram;
pub mod doctor;
mod doctor_actions;
mod doctor_probes;
pub mod file_explorer;
mod git;
mod halo_agent;
mod ingress;
mod install;
mod intercept;
mod intruder;
pub mod jwt;
pub mod mock_forge;
mod notes;
pub mod port_scanner;
mod postman_import;
mod proxy;
pub mod r2;
mod repeater;
mod sessions;
mod settings;
mod split_view;
mod state;
mod terminal;
mod util;
mod view;
pub mod wordlists;
mod workspace;

pub use util::FilterChip;

struct BackendSlot(Arc<dyn Backend>);

impl Global for BackendSlot {}

/// Facade for bootstrap + UI command dispatch.
pub struct Bridge;

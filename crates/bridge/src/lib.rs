//! Halo UI ↔ backend bridge.
//!
//! Replaces Tauri IPC with in-process typed calls and a Celestia
//! [`StoreContext`](celestia_ui::state::StoreContext) for reactive UI state.
//!
//! ## Bootstrap
//!
//! ```ignore
//! use halo_bridge::{Bridge, NativeBackend};
//!
//! Bridge::install_native(NativeBackend::boot()?, cx);
//! ```
//!
//! ## UI commands
//!
//! ```ignore
//! Bridge::start_proxy(8888, 8443, cx)?;
//! let running = Bridge::proxy_status(cx).running;
//! ```
//!
//! ## Observe state
//!
//! ```ignore
//! use celestia_ui::state::StoreContext;
//! use halo_backend::BridgeState;
//!
//! let bridge = StoreContext::<BridgeState>::require(cx);
//! let running = bridge.read(cx).proxy.running;
//! ```

mod agent_messages;
pub use agent_messages::HaloAgentMessages;
mod backend;
mod bridge;
pub mod coder_fonts;
mod craft;
mod error;
pub mod http_format;
mod native;
mod stub;
mod terminal;
mod types;
mod work;

pub use backend::Backend;
pub use bridge::comparer::{
    unified_text as comparer_unified_text, DiffInlinePart, DiffMode, DiffPartKind, DiffRow, RowKind,
};
pub use bridge::doctor::{DoctorCategory, DoctorItem, DoctorLogEntry, DoctorStatus, DoctorView};
pub use bridge::file_explorer::{
    is_safe_file_name as file_explorer_is_safe_name, safe_join as file_explorer_safe_join,
    safe_path_segments as file_explorer_safe_segments, ExplorerViewMode, FileExplorerView,
    LocalItem,
};
pub use bridge::jwt::{
    JwtAlgorithm, JwtClaim, JwtDecoded, JwtMode, JwtSeverity, JwtView, JwtVulnerability,
    JWT_DEFAULT_HEADER, JWT_DEFAULT_PAYLOAD,
};
pub use bridge::mock_forge::simulate_template as mock_simulate_template;
pub use bridge::port_scanner::{parse_ports, PORT_PRESET_COUNTS};
pub use bridge::r2::{
    is_valid_bucket_name as r2_is_valid_bucket_name, normalize_prefix as r2_normalize_prefix,
    R2CredentialsStatus, R2FileEntry, R2FolderEntry, R2Listing, R2UploadProgress, R2View,
};
pub use bridge::wordlists::{WordlistItem, WordlistStatus, WordlistTag, WordlistsView};
pub use bridge::{Bridge, BridgeState, FilterChip};
pub use coder_fonts::{
    default_font_id, font_family, font_label, font_option, font_options, CoderFontOption,
};
pub use craft::{expand_request, expand_vars, parse_context_variables};
pub use error::{BridgeError, BridgeResult};
pub use halo_backend::commands::halo_agent::{
    BrowserStatus, HarnessReadiness, McpKeyValue, McpServerConfig, McpTransport, SkillInfo,
};
pub use halo_backend::commands::shell::ShellAppearance;
pub use http_format::{
    format_json_body, format_raw_request, format_raw_request_parts, format_raw_response,
    format_raw_response_parts, parse_raw_request, parse_raw_response,
};
pub use native::NativeBackend;
pub use stub::StubBackend;
pub use terminal::{TerminalMeta, TerminalRead, TerminalStore, TERMINAL_BUFFER_CAP};
pub use types::MockChaosDto;
pub use types::{
    ActiveRequestState, BackendEvent, ChronicleLogDto, CollectionDto, ContextDto,
    CrawlerCrawlConfig, CrawlerInsightDto, CrawlerLogDto, CrawlerPageDto, CrawlerSessionDto,
    CrawlerStatus, CrawlerViewTab, DebugLogEntry, DebugLogLevel, DeleteArtifactResult, DetailSide,
    DetailTab, ForgeActiveView, ForgeRequestTab, ForgeResponseTab, GitOutputEntry, HistoryFilter,
    HttpLogDetail,
    HttpLogRow, HttpSessionDto, InterceptSnapshot, IntruderAttackMode, IntruderConfig,
    IntruderConfigTab, IntruderDraft, IntruderPayloadType, IntruderProcessingStep,
    IntruderResultRow, IntruderStatus, MockDomainDto, MockForgeView, MockLogDto, MockRouteDto,
    MockServerStatusDto, NoteRecord, NotesFilterTab, NotesSortOption, NotesViewMode, PausedRow,
    ProxyStatus, RepeaterRequestDto, RepeaterResponseDto, ResetLocalDataResult, SettingsTab,
    StashDto, StashEndpointDto, StorageInfo,
};
pub use types::{PortScanRequestDto, PortScanResultDto, PortScannerView};
pub use work::{
    work_title, ActivitySession, AgentApprovalMode, AgentWork, AntigravityLoginStatus,
    Camera2D, CoderDirEntry, CoderExplorerInline, CoderExplorerInlineKind, CoderFileKind,
    CoderFsOp, CoderLayout, CoderLineEnding, CoderLspServerRow, CoderLspStatus, CoderOpenFile,
    CoderPane, CoderProjectKind, CoderSettings, DesktopStatus, CoderSidebar, CoderSplitAxis,
    CoderTerminal, CoderTerminalKind, CoderTreeEntry, CoderWork,
    DiagramEdge, DiagramEdgeRoute, DiagramNode, DiagramShapeKind, DiagramTable, DiagramTool,
    DiagramWork, GitCheckoutBlock, GitDiff, GitFile, GitGraphBranch, GitGraphCommit, GitGraphCommitFile,
    GitGraphRef, GitGraphRefKind, GitGraphRemote, GitGraphStash, GitGraphState, GitGraphWorktree,
    GitInspectorTab, GitState, HaloAgentCompletion, HaloAgentCompletionItem,
    HaloAgentCompletionKind, HaloAgentContextUsage, HaloAgentDelivery, HaloAgentFileChange,
    HaloAgentFileChangeKind, HaloAgentInputQuestion, HaloAgentMessage, HaloAgentPart,
    HaloAgentPendingInput, HaloAgentQueuedMessage, HaloAgentRole, HaloAgentSession,
    HaloAgentSessionStatus, HaloAgentSource, HaloAgentSubagent, HaloAgentSubagentStatus,
    HaloAgentTodo, HttpWork, InterceptWork, IntruderWork, NotesWork, Port, PullRequest,
    PullRequestFilter, PullRequestView, RepeaterWork, SourceControlPane, SplitLayoutKind,
    SplitViewState, SplitViewWork, WorkData, AppFlag, WorkKind, WorkTab, CODER_MAX_PANES,
    CODER_MAX_SPLIT_DEPTH,
};

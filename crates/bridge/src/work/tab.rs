//! One browser-style work tab and the per-tool working data it owns.

use super::*;

use crate::types::{
    ActiveRequestState, CrawlerStatus, DetailSide, DetailTab, ForgeActiveView, ForgeRequestTab,
    ForgeResponseTab, HistoryFilter, HttpLogDetail, IntruderDraft, IntruderStatus, NotesViewMode,
    RepeaterRequestDto, RepeaterResponseDto, SettingsTab,
};

/// One browser-style tab. `data` is the working set for [`WorkKind`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkTab {
    pub id: String,
    pub title: String,
    pub kind: WorkKind,
    /// Activity session this tab belongs to.
    pub session_id: String,
    pub data: WorkData,
}

/// A workspace that holds the open tabs and the activity they captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivitySession {
    pub id: String,
    pub title: String,
    pub active_work_tab_id: Option<String>,
    /// HTTP rows last shown for this session.
    pub history: Vec<crate::types::HttpLogRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkData {
    Http(HttpWork),
    Intercept(InterceptWork),
    Repeater(RepeaterWork),
    Intruder(IntruderWork),
    Crawler(CrawlerStatus),
    Coder(CoderWork),
    Settings(SettingsTab),
    Notes(NotesWork),
    Mock(MockForgeWork),
    Override(MockForgeWork),
    Jwt(JwtWork),
    PortScanner(PortScannerWork),
    Comparer(ComparerWork),
    FileExplorer(FileExplorerWork),
    R2Storage(R2Work),
    Wordlists(WordlistsWork),
    SplitView(SplitViewWork),
    Agent(AgentWork),
    Doctor(DoctorWork),
    SourceControl(SourceControlWork),
    Diagram(DiagramWork),
}

impl WorkData {
    pub fn for_kind(kind: WorkKind) -> Self {
        match kind {
            WorkKind::HttpHistory => Self::Http(HttpWork::default()),
            WorkKind::Intercept => Self::Intercept(InterceptWork::default()),
            WorkKind::Repeater => Self::Repeater(RepeaterWork::default()),
            WorkKind::Intruder => Self::Intruder(IntruderWork::default()),
            WorkKind::Crawler => Self::Crawler(CrawlerStatus::default()),
            WorkKind::Coder => Self::Coder(CoderWork::default()),
            WorkKind::SourceControl => Self::SourceControl(SourceControlWork),
            WorkKind::Agent => Self::Agent(AgentWork::default()),
            WorkKind::Doctor => Self::Doctor(DoctorWork::default()),
            WorkKind::Settings => Self::Settings(SettingsTab::General),
            WorkKind::Notes => Self::Notes(NotesWork::default()),
            WorkKind::Diagram => Self::Diagram(DiagramWork::default()),
            WorkKind::ApiMock | WorkKind::ApiOverride => Self::Mock(MockForgeWork::default()),
            WorkKind::Jwt => Self::Jwt(JwtWork::default()),
            WorkKind::PortScanner => Self::PortScanner(PortScannerWork::default()),
            WorkKind::Comparer => Self::Comparer(ComparerWork::default()),
            WorkKind::FileExplorer => Self::FileExplorer(FileExplorerWork::default()),
            WorkKind::R2Storage => Self::R2Storage(R2Work),
            WorkKind::Wordlists => Self::Wordlists(WordlistsWork),
            WorkKind::SplitView => Self::SplitView(SplitViewWork::default()),
        }
    }
}

/// Layout arrangement for the Split View page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SplitLayoutKind {
    #[default]
    Split2,
    Split3,
    Split4,
}

impl SplitLayoutKind {
    pub const ALL: [Self; 3] = [Self::Split2, Self::Split3, Self::Split4];

    pub fn id(self) -> &'static str {
        match self {
            Self::Split2 => "split-2",
            Self::Split3 => "split-3",
            Self::Split4 => "split-4",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Split2 => "2-Split",
            Self::Split3 => "3-Split",
            Self::Split4 => "4-Split",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Split2 => "Dual Split Screen",
            Self::Split3 => "Sub-Split Screen",
            Self::Split4 => "Quad Split Screen",
        }
    }

    pub fn slot_count(self) -> usize {
        match self {
            Self::Split2 => 2,
            Self::Split3 => 3,
            Self::Split4 => 4,
        }
    }
}

/// Shared Split View workspace: layout + which app fills each slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitViewState {
    pub layout: SplitLayoutKind,
    /// Page ids for slots 0..3 (`None` = empty picker).
    pub slots: [Option<String>; 4],
}

impl Default for SplitViewState {
    fn default() -> Self {
        Self {
            layout: SplitLayoutKind::Split2,
            slots: [None, None, None, None],
        }
    }
}

impl SplitViewState {
    pub fn filled_count(&self) -> usize {
        self.slots
            .iter()
            .take(self.layout.slot_count())
            .filter(|s| s.is_some())
            .count()
    }
}

/// Per-tab marker — layout lives on [`crate::bridge::BridgeState::split_view`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SplitViewWork;

/// View state for Notes tab.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NotesWork {
    pub open_note_ids: Vec<String>,
    pub active_note_id: Option<String>,
    pub search_query: String,
    pub view_mode: NotesViewMode,
}

/// HTTP view state for one tab. The visible log list is stored on the activity session.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HttpWork {
    pub filter: HistoryFilter,
    pub selected_log_id: Option<String>,
    pub selected_detail: Option<HttpLogDetail>,
    pub detail_tab: DetailTab,
    pub detail_side: DetailSide,
}

/// Intercept host scope for one work tab. Paused rows are tagged with the tab id.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct InterceptWork {
    pub capture_hosts: Vec<String>,
    pub selected_paused_id: Option<String>,
}

/// API Mock / API Override work tab. Route data lives in the shared
/// `MockForgeView`; per-tab state is just the list filter.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MockForgeWork {
    pub filter: String,
}

/// Agent work tab. Session state lives on [`CoderWork`] agent fields for now
/// (coding agent); this marker keeps the tab identity separate from Coder.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AgentWork;

/// Doctor work tab. All state lives in the shared `DoctorView`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DoctorWork;

/// Source Control work tab. Git snapshot lives on [`crate::BridgeState::git`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SourceControlWork;

/// JWT page work tab. All state lives in the shared `JwtView`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct JwtWork {
    pub mode: String,
}

/// Port Scanner work tab. All state lives in the shared `PortScannerView`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PortScannerWork {
    pub filter: String,
}

/// Comparer work tab. State lives in the shared `ComparerView`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ComparerWork {
    pub mode: String,
}

/// File Explorer work tab. State lives in the shared `FileExplorerView`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FileExplorerWork {
    pub mode: String,
}

/// R2 Storage work tab. State lives in the shared `R2View`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct R2Work;

/// Wordlists work tab. State lives in the shared `WordlistsView`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WordlistsWork;

/// Repeater draft, response, and which folder node is open.
/// Folders themselves stay in the database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepeaterWork {
    pub response: Option<RepeaterResponseDto>,
    pub last_sent_request: Option<RepeaterRequestDto>,
    pub pending_repeater: Option<RepeaterRequestDto>,
    pub selected_node_id: Option<String>,
    pub active_context_id: Option<String>,
    pub active_request: ActiveRequestState,
    pub forge_view: ForgeActiveView,
    pub forge_request_tab: ForgeRequestTab,
    pub forge_response_tab: ForgeResponseTab,
    pub tree_filter: String,
    pub collapsed_stashes: Vec<String>,
}

impl Default for RepeaterWork {
    fn default() -> Self {
        Self {
            response: None,
            last_sent_request: None,
            pending_repeater: None,
            selected_node_id: None,
            active_context_id: None,
            active_request: ActiveRequestState::default(),
            forge_view: ForgeActiveView::default(),
            forge_request_tab: ForgeRequestTab::default(),
            forge_response_tab: ForgeResponseTab::default(),
            tree_filter: String::new(),
            collapsed_stashes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct IntruderWork {
    pub status: IntruderStatus,
    pub pending: Option<IntruderDraft>,
}

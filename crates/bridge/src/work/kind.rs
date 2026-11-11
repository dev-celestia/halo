//! Work-tab tool kinds ([`WorkKind`]) and tab title helpers.

/// Maturity of a launchable app.
///
/// Shipping builds ([`WorkKind::ships`]) only expose [`AppFlag::Release`]
/// apps. Debug builds always include alpha apps so they stay reachable while
/// iterating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppFlag {
    /// Ready for shipping builds.
    Release,
    /// Experimental — hidden from shipping builds.
    Alpha,
}

impl AppFlag {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Alpha => "alpha",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Release => "Release",
            Self::Alpha => "Alpha",
        }
    }
}

/// Tool a work tab opens. Desktop is not a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkKind {
    HttpHistory,
    Intercept,
    Repeater,
    Intruder,
    Crawler,
    Coder,
    /// Source Control — git changes, commit, sync (own tool, not a Coder mode).
    SourceControl,
    Agent,
    Settings,
    Notes,
    ApiMock,
    ApiOverride,
    Jwt,
    PortScanner,
    Comparer,
    FileExplorer,
    /// Cloudflare R2 object browser.
    R2Storage,
    /// Fuzzing wordlist catalog.
    Wordlists,
    SplitView,
    Doctor,
    Diagram,
}

impl WorkKind {
    /// Every launchable tool, in dock / desktop order.
    pub const ALL: [WorkKind; 21] = [
        Self::HttpHistory,
        Self::Intercept,
        Self::Repeater,
        Self::Intruder,
        Self::Crawler,
        Self::Coder,
        Self::SourceControl,
        Self::Agent,
        Self::Doctor,
        Self::Notes,
        Self::Diagram,
        Self::Settings,
        Self::ApiMock,
        Self::ApiOverride,
        Self::Jwt,
        Self::PortScanner,
        Self::Comparer,
        Self::FileExplorer,
        Self::R2Storage,
        Self::Wordlists,
        Self::SplitView,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::HttpHistory => "HTTP",
            Self::Intercept => "Intercept",
            Self::Repeater => "Repeater",
            Self::Intruder => "Intruder",
            Self::Crawler => "Crawler",
            Self::Coder => "Coder",
            Self::SourceControl => "Source Control",
            Self::Agent => "Agent",
            Self::Doctor => "Doctor",
            Self::Settings => "Settings",
            Self::Notes => "Notes",
            Self::Diagram => "Diagram",
            Self::ApiMock => "API Mock",
            Self::ApiOverride => "API Override",
            Self::Jwt => "JWT",
            Self::PortScanner => "Port Scanner",
            Self::Comparer => "Comparer",
            Self::FileExplorer => "File Explorer",
            Self::R2Storage => "R2 Storage",
            Self::Wordlists => "Wordlists",
            Self::SplitView => "Split View",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HttpHistory => "http",
            Self::Intercept => "intercept",
            Self::Repeater => "repeater",
            Self::Intruder => "intruder",
            Self::Crawler => "crawler",
            Self::Coder => "coder",
            Self::SourceControl => "source-control",
            Self::Agent => "agent",
            Self::Doctor => "doctor",
            Self::Settings => "settings",
            Self::Notes => "notes",
            Self::Diagram => "diagram",
            Self::ApiMock => "api-mock",
            Self::ApiOverride => "api-override",
            Self::Jwt => "jwt",
            Self::PortScanner => "port-scanner",
            Self::Comparer => "comparer",
            Self::FileExplorer => "file-explorer",
            Self::R2Storage => "r2-storage",
            Self::Wordlists => "wordlists",
            Self::SplitView => "split-view",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "http" => Some(Self::HttpHistory),
            "intercept" => Some(Self::Intercept),
            "repeater" => Some(Self::Repeater),
            "intruder" => Some(Self::Intruder),
            "crawler" => Some(Self::Crawler),
            "coder" => Some(Self::Coder),
            "source-control" => Some(Self::SourceControl),
            "agent" => Some(Self::Agent),
            "doctor" => Some(Self::Doctor),
            "settings" => Some(Self::Settings),
            "notes" => Some(Self::Notes),
            "diagram" => Some(Self::Diagram),
            "api-mock" => Some(Self::ApiMock),
            "api-override" => Some(Self::ApiOverride),
            "jwt" => Some(Self::Jwt),
            "port-scanner" => Some(Self::PortScanner),
            "comparer" => Some(Self::Comparer),
            "file-explorer" => Some(Self::FileExplorer),
            "r2-storage" => Some(Self::R2Storage),
            "wordlists" => Some(Self::Wordlists),
            "split-view" => Some(Self::SplitView),
            // Retired top-level Terminal app — drop from restored layouts.
            "terminal" => None,
            _ => None,
        }
    }

    pub fn is_core(self) -> bool {
        matches!(
            self,
            Self::HttpHistory | Self::Intercept | Self::Repeater | Self::Intruder | Self::Notes
        )
    }

    /// Maturity flag for this app. Only [`AppFlag::Release`] apps ship.
    pub fn flag(self) -> AppFlag {
        match self {
            Self::Coder | Self::Agent | Self::SourceControl | Self::Notes | Self::Doctor => {
                AppFlag::Release
            }
            _ => AppFlag::Alpha,
        }
    }

    /// Whether this app is available in the current build.
    ///
    /// Release / `.app` builds expose only [`AppFlag::Release`]. Debug builds
    /// include every app so alpha tools stay reachable while iterating.
    pub fn ships(self) -> bool {
        matches!(self.flag(), AppFlag::Release) || cfg!(debug_assertions)
    }
}
pub fn work_title(kind: WorkKind, existing: usize) -> String {
    if existing == 0 {
        kind.label().to_string()
    } else {
        format!("{} {}", kind.label(), existing + 1)
    }
}

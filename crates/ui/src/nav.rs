//! Focused tool pages: HTTP, Intercept, Intruder, Repeater, Coder.

use halo_bridge::{AppFlag, WorkKind};

/// One navigable tool / screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Wallpaper with app icons and session widgets.
    Desktop,
    HttpHistory,
    Intercept,
    Repeater,
    Intruder,
    Crawler,
    Coder,
    SourceControl,
    Agent,
    Settings,
    ApiMock,
    ApiOverride,
    Jwt,
    PortScanner,
    Comparer,
    FileExplorer,
    R2Storage,
    Wordlists,
    SplitView,
    Notes,
    Doctor,
    Diagram,
}

impl Page {
    /// Launchable tools, in dock / desktop / start-menu order.
    ///
    /// Matches [`WorkKind::ALL`]. The dock still skips [`Page::Settings`] (tray
    /// gear), but the start menu lists every tool so each one is reachable.
    pub const ALL: [Page; 21] = [
        Page::HttpHistory,
        Page::Intercept,
        Page::Repeater,
        Page::Intruder,
        Page::Crawler,
        Page::Coder,
        Page::SourceControl,
        Page::Agent,
        Page::Doctor,
        Page::Notes,
        Page::Diagram,
        Page::Settings,
        Page::ApiMock,
        Page::ApiOverride,
        Page::Jwt,
        Page::PortScanner,
        Page::Comparer,
        Page::FileExplorer,
        Page::R2Storage,
        Page::Wordlists,
        Page::SplitView,
    ];

    /// Tools that can fill a Split View slot (excludes Split View itself).
    pub fn split_slot_apps() -> impl Iterator<Item = Page> {
        Self::ALL.into_iter().filter(|p| *p != Self::SplitView)
    }
    // ApiMock/ApiOverride inserted below

    pub fn id(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::HttpHistory => "http-history",
            Self::Intercept => "intercept",
            Self::Repeater => "repeater",
            Self::Intruder => "intruder",
            Self::Crawler => "crawler",
            Self::Coder => "coder",
            Self::SourceControl => "source-control",
            Self::Agent => "agent",
            Self::Doctor => "doctor",
            Self::Notes => "notes",
            Self::Diagram => "diagram",
            Self::Settings => "settings",
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Desktop => "Desktop",
            Self::HttpHistory => "HTTP",
            Self::Intercept => "Intercept",
            Self::Repeater => "Repeater",
            Self::Intruder => "Intruder",
            Self::Crawler => "Crawler",
            Self::Coder => "Coder",
            Self::SourceControl => "Source Control",
            Self::Agent => "Agent",
            Self::Doctor => "Doctor",
            Self::Notes => "Notes",
            Self::Diagram => "Diagram",
            Self::Settings => "Settings",
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

    pub fn description(self) -> &'static str {
        match self {
            Self::Desktop => "App icons and a live view of the proxy session.",
            Self::HttpHistory => "Browse captured HTTP/HTTPS traffic and inspect any request.",
            Self::Intercept => "Pause requests or responses, edit them, then forward or drop.",
            Self::Repeater => "Edit a request, send it again, and compare the response.",
            Self::Intruder => {
                "Mark payload positions and fuzz an endpoint with a wordlist or range."
            }
            Self::Crawler => "Crawl a target through the proxy and review recon findings.",
            Self::Coder => "Open a folder, browse files, edit code, and save.",
            Self::SourceControl => "Review changes, commit, and sync with remotes.",
            Self::Agent => "Universal AI agent — Halo Agent for now.",
            Self::Doctor => {
                "Diagnose and install missing harnesses, language servers, and browser tools."
            }
            Self::Notes => "Tabbed markdown notepad with saved notes library and export.",
            Self::Diagram => "2D vector diagram, flowchart, and table canvas studio.",
            Self::Settings => "App chrome, editor, terminal, agent, network, and storage setup.",
            Self::ApiMock => "Serve canned responses from a local mock server on 127.0.0.1.",
            Self::ApiOverride => {
                "Replace live proxied responses for captured hosts with your own rules."
            }
            Self::Jwt => "Decode JWT tokens, audit them for common flaws, and forge new ones.",
            Self::PortScanner => "TCP connect-scan a host for open ports, services, and banners.",
            Self::Comparer => {
                "Diff two texts side by side at line, word, or character granularity."
            }
            Self::FileExplorer => {
                "A sandboxed workspace for importing, organizing, and opening local files."
            }
            Self::R2Storage => "Browse, upload, and share objects in Cloudflare R2 buckets.",
            Self::Wordlists => "Download fuzzing wordlists and preview them before a scan.",
            Self::SplitView => "Pin 2–4 tools side by side in a resizable multi-pane workspace.",
        }
    }

    /// Maturity flag for this page's tool. Desktop has none.
    pub fn flag(self) -> Option<AppFlag> {
        self.work_kind().map(WorkKind::flag)
    }

    /// Whether this page is available in the current build.
    ///
    /// Desktop is always available; tools defer to [`WorkKind::ships`].
    pub fn ships(self) -> bool {
        match self.work_kind() {
            None => true,
            Some(kind) => kind.ships(),
        }
    }

    pub fn work_kind(self) -> Option<WorkKind> {
        Some(match self {
            // A satellite: no work tab, its own window.
            Self::Desktop => return None,
            Self::HttpHistory => WorkKind::HttpHistory,
            Self::Intercept => WorkKind::Intercept,
            Self::Repeater => WorkKind::Repeater,
            Self::Intruder => WorkKind::Intruder,
            Self::Crawler => WorkKind::Crawler,
            Self::Coder => WorkKind::Coder,
            Self::SourceControl => WorkKind::SourceControl,
            Self::Agent => WorkKind::Agent,
            Self::Doctor => WorkKind::Doctor,
            Self::Notes => WorkKind::Notes,
            Self::Diagram => WorkKind::Diagram,
            Self::Settings => WorkKind::Settings,
            Self::ApiMock => WorkKind::ApiMock,
            Self::ApiOverride => WorkKind::ApiOverride,
            Self::Jwt => WorkKind::Jwt,
            Self::PortScanner => WorkKind::PortScanner,
            Self::Comparer => WorkKind::Comparer,
            Self::FileExplorer => WorkKind::FileExplorer,
            Self::R2Storage => WorkKind::R2Storage,
            Self::Wordlists => WorkKind::Wordlists,
            Self::SplitView => WorkKind::SplitView,
        })
    }

    pub fn from_work_kind(kind: WorkKind) -> Self {
        match kind {
            WorkKind::HttpHistory => Self::HttpHistory,
            WorkKind::Intercept => Self::Intercept,
            WorkKind::Repeater => Self::Repeater,
            WorkKind::Intruder => Self::Intruder,
            WorkKind::Crawler => Self::Crawler,
            WorkKind::Coder => Self::Coder,
            WorkKind::SourceControl => Self::SourceControl,
            WorkKind::Agent => Self::Agent,
            WorkKind::Doctor => Self::Doctor,
            WorkKind::Notes => Self::Notes,
            WorkKind::Diagram => Self::Diagram,
            WorkKind::Settings => Self::Settings,
            WorkKind::ApiMock => Self::ApiMock,
            WorkKind::ApiOverride => Self::ApiOverride,
            WorkKind::Jwt => Self::Jwt,
            WorkKind::PortScanner => Self::PortScanner,
            WorkKind::Comparer => Self::Comparer,
            WorkKind::FileExplorer => Self::FileExplorer,
            WorkKind::R2Storage => Self::R2Storage,
            WorkKind::Wordlists => Self::Wordlists,
            WorkKind::SplitView => Self::SplitView,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain([Self::Desktop])
            .find(|p| p.id() == id)
    }
}

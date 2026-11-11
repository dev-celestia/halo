//! Debugger panel session state (per-window, owned by the UI).

/// Default docked panel height in pixels.
pub const DEFAULT_HEIGHT: f32 = 280.;
/// Minimum resizable height.
pub const MIN_HEIGHT: f32 = 160.;
/// Maximum resizable height.
pub const MAX_HEIGHT: f32 = 520.;

/// Which DevTools tab is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DebuggerTab {
    #[default]
    Elements,
    Network,
    Storage,
    Logs,
}

impl DebuggerTab {
    pub fn id(self) -> &'static str {
        match self {
            Self::Elements => "elements",
            Self::Network => "network",
            Self::Storage => "storage",
            Self::Logs => "logs",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Elements => "Elements",
            Self::Network => "Network",
            Self::Storage => "Storage",
            Self::Logs => "Logs",
        }
    }

    pub fn all() -> &'static [DebuggerTab] {
        &[Self::Elements, Self::Network, Self::Storage, Self::Logs]
    }
}

/// Per-window debugger chrome state.
#[derive(Clone, Debug)]
pub struct DebuggerState {
    pub open: bool,
    pub tab: DebuggerTab,
    /// Panel height in logical pixels.
    pub height: f32,
    /// When true, pointer hits select registered debug regions.
    pub inspect_mode: bool,
    /// Selected network entry id (unified list).
    pub selected_network_id: Option<String>,
    /// Selected prefs tree path (slash-joined keys).
    pub selected_prefs_path: Option<String>,
}

impl Default for DebuggerState {
    fn default() -> Self {
        Self {
            open: false,
            tab: DebuggerTab::Elements,
            height: DEFAULT_HEIGHT,
            inspect_mode: false,
            selected_network_id: None,
            selected_prefs_path: None,
        }
    }
}

impl DebuggerState {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if !self.open {
            self.inspect_mode = false;
        }
    }

    pub fn set_tab(&mut self, tab: DebuggerTab) {
        self.tab = tab;
    }

    pub fn set_height(&mut self, height: f32) {
        self.height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
    }
}

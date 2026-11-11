//! In-app DevTools for Halo — Elements, Network, Storage, Logs.
//!
//! Mount [`panel::render_debugger`] from the main window when the debugger is open.
//! Register UI regions with [`element::register_region`] / [`debug_region`].

mod confirm;
mod element;
mod logs;
mod network;
mod panel;
mod state;
mod storage;

pub use element::{
    DebugBounds, DebugRegion, DebugRegionMeta, clear_regions, debug_region, region_bounds_sensor,
    region_list, register_region, select_region_at, selected_region, set_selected,
};
pub use logs::{DebugLogEntry, DebugLogLevel};
pub use network::{NetworkEntry, NetworkSource, collect_network};
pub use panel::render_debugger;
pub use state::{DEFAULT_HEIGHT, DebuggerState, DebuggerTab, MAX_HEIGHT, MIN_HEIGHT};
pub use storage::{PrefsNode, PrefsTree, format_bytes, load_prefs_tree};

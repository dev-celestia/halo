//! Shared UI controls reused across the shell and tool pages.

pub mod confirm;
pub mod prompt;
pub mod proxy_button;

/// Re-export all primitive components from celestia_ui so every primitive
/// UI component in Halo is always mapped to the Celestia design system.
#[allow(unused_imports)]
pub use celestia_ui::components::primitive;
#[allow(unused_imports)]
pub use celestia_ui::components::primitive::*;

pub use confirm::{confirm_delete, confirm_destructive};
pub use prompt::prompt_text;
pub use proxy_button::{
    ProxyActionState, ProxyButtonCopy, proxy_dock_button, proxy_start_button, proxy_stop_button,
    proxy_toggle_button, toggle_proxy,
};

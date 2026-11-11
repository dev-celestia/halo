//! Port Scanner page — TCP connect scans with presets, stealth mode, and a
//! live results ledger.

pub mod results;
pub mod toolbar;
pub mod welcome;

use gpui::*;
use halo_bridge::BridgeState;

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

pub use results::render_results;
pub use toolbar::render_toolbar;
pub use welcome::render_welcome;

pub fn render_port_scanner(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let ps = &state.port_scanner;
    if !ps.has_run {
        return render_welcome(state, editors, cx).into_any_element();
    }

    div()
        .id("port-scanner-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_toolbar(state, editors, cx))
        .child(render_results(state, cx))
        .into_any_element()
}

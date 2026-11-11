//! HTTP history page — traffic table above a bottom dock holding the request
//! detail.
//!
//! The request detail used to open as a window-layer sheet over the table. It
//! now lives in a docked panel under the table so the traffic list stays
//! readable while inspecting, and the split between the two is a drag handle
//! instead of a fixed sheet height.
//!
//! The page is inset from the window edge: the toolbar and the traffic surface
//! share one 16px margin, and the table + dock sit inside a single rounded
//! frame so the traffic list never runs into the window border.

mod detail_view;
mod dock;
mod history_list;
mod history_row;
mod styles;
mod toolbar;

pub use detail_view::DetailEditors;
pub use history_list::{HttpHistoryList, render_history_list};
#[allow(unused_imports)]
pub use history_row::render_history_row;
pub use toolbar::render_http_toolbar;

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::BridgeState;

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

/// Height the dock opens at when a row is first selected.
const DOCK_HEIGHT: f32 = 360.;
/// Floor for the dock: the section bar, the detail tabs and a couple of
/// content lines stay legible.
const DOCK_MIN_HEIGHT: f32 = 168.;

pub fn render_http(state: &BridgeState, editors: &ToolEditors, cx: &mut App) -> impl IntoElement {
    let detail_editors = DetailEditors::new(
        editors.history_request.clone(),
        editors.history_response.clone(),
    );
    // The dock only exists while something is selected, so the table gets the
    // whole page back the moment the detail is closed.
    let dock_open = state.selected_detail.is_some();

    // Sync the virtual list before reading theme paints — `sync` needs `&mut App`.
    let mut split = ResizablePanelGroup::vertical("http-page-split").with_handle(true);
    split = split.child(resizable_panel().child(render_history_list(
        &state.history,
        state.selected_log_id.as_deref(),
        state.history_pending_count,
        &editors.http_history_list,
        cx,
    )));
    if dock_open {
        split = split.child(
            resizable_panel()
                .size(px(DOCK_HEIGHT))
                .size_range(px(DOCK_MIN_HEIGHT)..px(760.))
                .child(dock::render_http_dock(state, &detail_editors, cx)),
        );
    }

    let theme = cx.theme();
    div()
        .id("http-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        // One inset for the whole page, so the toolbar controls and the traffic
        // surface share a left edge instead of the table hugging the window.
        .p_4()
        .child(render_http_toolbar(state, editors, cx))
        .child(
            // Single frame around both halves of the split. The table drops its
            // own border + fill inside it (`history_list`), so this rounded
            // surface is the only chrome — and the only place the table color
            // is painted, so it follows the corner radius instead of bleeding
            // a square fill past the border.
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .bg(theme.table)
                .overflow_hidden()
                .child(split),
        )
}

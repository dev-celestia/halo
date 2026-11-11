//! Pull-request inbox — list column plus a detail pane.

pub mod detail;
pub mod list;

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use gpui::*;
use halo_bridge::BridgeState;

use self::detail::detail_pane;
use self::list::{LIST_MAX, LIST_MIN, LIST_WIDTH, list_column};
use crate::pages::editors::ToolEditors;

pub fn render_pulls(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    ResizablePanelGroup::horizontal("source-control-pulls")
        .with_handle(true)
        .child(
            resizable_panel()
                .size(LIST_WIDTH)
                .size_range(LIST_MIN..LIST_MAX)
                .child(
                    div()
                        .size_full()
                        .overflow_hidden()
                        .child(list_column(state, editors, cx)),
                ),
        )
        .child(
            resizable_panel().child(
                div()
                    .size_full()
                    .min_w_0()
                    .overflow_hidden()
                    .child(detail_pane(state, cx)),
            ),
        )
}

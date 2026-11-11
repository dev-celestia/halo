//! API Mock page — local mock HTTP server (127.0.0.1) with editable endpoints.

pub mod chaos;
pub mod endpoint_list;
pub mod logs;
pub mod route_editor;
pub mod toolbar;

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use gpui::*;
use halo_bridge::BridgeState;

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

pub use endpoint_list::render_endpoint_list;
pub use logs::render_logs_strip;
pub use route_editor::render_route_editor;
pub use toolbar::render_server_toolbar;

pub fn render_api_mock(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .id("api-mock-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_server_toolbar(state, editors, cx))
        .child(
            div().w_full().flex_1().min_h_0().overflow_hidden().child(
                ResizablePanelGroup::horizontal("api-mock-layout")
                    .with_handle(true)
                    .child(
                        resizable_panel()
                            .size(px(300.))
                            .size_range(px(200.)..px(480.))
                            .child(render_endpoint_list(state, editors, cx)),
                    )
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .min_w_0()
                                .overflow_hidden()
                                .child(render_route_editor(state, editors, cx)),
                        ),
                    ),
            ),
        )
        .child(render_logs_strip(state, cx))
}

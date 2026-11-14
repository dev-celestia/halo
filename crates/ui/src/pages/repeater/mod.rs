//! Repeater / Forge workbench — folders tree + request/response split.

mod env_form;
mod env_modal;
mod folder_tree;
mod headers_form;
mod request_form;
mod response_view;

pub use env_form::EnvVarsForm;
pub use env_modal::render_env_modal;
pub use headers_form::HeadersForm;

pub use request_form::{
    headers_map_to_raw, join_scripts, render_request_pane, render_request_toolbar,
    render_workspace_toolbar, split_raw_http,
};
pub use response_view::{render_response_view, response_editor_text};

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::BridgeState;

use super::editors::ToolEditors;
use folder_tree::render_folder_tree;

pub fn render_repeater(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .id("repeater-page")
        .relative()
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        // Workspace bar (collection + env) spans the full workbench width.
        .child(render_workspace_toolbar(state, editors, cx))
        .child(
            div().w_full().flex_1().min_h_0().overflow_hidden().child(
                ResizablePanelGroup::horizontal("repeater-layout")
                    .with_handle(true)
                    .child(
                        resizable_panel()
                            .size(px(272.))
                            .size_range(px(200.)..px(500.))
                            .child(
                                div()
                                    .size_full()
                                    .overflow_hidden()
                                    .border_r_1()
                                    .border_color(cx.theme().border)
                                    .child(render_folder_tree(state, editors, cx)),
                            ),
                    )
                    .child(
                        resizable_panel().child(
                            // Request bar spans both panes; only the tabbed
                            // panes below it split 50/50.
                            div()
                                .size_full()
                                .min_w_0()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .w_full()
                                        .flex_none()
                                        .border_b_1()
                                        .border_color(cx.theme().border)
                                        .child(render_request_toolbar(state, editors, cx)),
                                )
                                .child(
                                    div().w_full().flex_1().min_h_0().overflow_hidden().child(
                                        ResizablePanelGroup::horizontal("repeater-editor-split")
                                            .with_handle(true)
                                            .child(
                                                // Zero flex basis on both sides splits the
                                                // editor row 50/50; a pixel size here would
                                                // pin Forge and hand the remainder to the
                                                // response panel.
                                                resizable_panel()
                                                    .size(px(0.))
                                                    .size_range(px(240.)..Pixels::MAX)
                                                    .child(
                                                        div()
                                                            .size_full()
                                                            .min_w_0()
                                                            .min_h_0()
                                                            .overflow_hidden()
                                                            .child(render_request_pane(
                                                                state, editors, cx,
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                resizable_panel()
                                                    .size(px(0.))
                                                    .size_range(px(240.)..Pixels::MAX)
                                                    .child(
                                                        div()
                                                            .size_full()
                                                            .min_w_0()
                                                            .min_h_0()
                                                            .overflow_hidden()
                                                            .child(render_response_view(
                                                                state, editors, cx,
                                                            )),
                                                    ),
                                            ),
                                    ),
                                ),
                        ),
                    ),
            ),
        )
        .when(state.env_modal_open, |this| {
            this.child(render_env_modal(state, editors, cx))
        })
}

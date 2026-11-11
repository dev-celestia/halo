//! Intruder page — fuzzing / brute-force via Bridge → Invoker.

mod config_panel;
mod markers;
mod result_inspector;
mod results_panel;
mod status_bar;
mod toolbar;

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use gpui::*;
use halo_bridge::BridgeState;

use super::editors::ToolEditors;
use crate::pages::chrome::PAGE_GAP;
use config_panel::render_config_panel;
use result_inspector::render_result_inspector;
use results_panel::render_results_panel;
use status_bar::render_intruder_status;
use toolbar::render_intruder_toolbar;

pub fn render_intruder(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .id("intruder-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_intruder_status(
            state.intruder.running,
            state.intruder.attack_id.as_deref(),
            state.intruder.message.as_deref(),
            cx,
        ))
        .child(render_intruder_toolbar(state, editors, cx))
        .child(
            div().w_full().flex_1().min_h_0().overflow_hidden().child(
                ResizablePanelGroup::horizontal("intruder-layout")
                    .with_handle(true)
                    .child(
                        resizable_panel()
                            .size(px(420.))
                            .size_range(px(280.)..px(720.))
                            .child(
                                div()
                                    .size_full()
                                    .min_w_0()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(render_config_panel(state, editors, cx)),
                            ),
                    )
                    .child(
                        resizable_panel().child(
                            ResizablePanelGroup::vertical("intruder-results-split")
                                .with_handle(true)
                                .child(
                                    resizable_panel()
                                        .size(px(240.))
                                        .size_range(px(140.)..px(480.))
                                        .child(
                                            div()
                                                .size_full()
                                                .min_h_0()
                                                .overflow_hidden()
                                                .child(render_results_panel(state, cx)),
                                        ),
                                )
                                .child(
                                    resizable_panel().child(
                                        div()
                                            .size_full()
                                            .min_h_0()
                                            .overflow_hidden()
                                            .child(render_result_inspector(state, cx)),
                                    ),
                                ),
                        ),
                    ),
            ),
        )
}

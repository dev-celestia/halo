//! Comparer page — side-by-side text diffing at line / word / character
//! granularity.
//!
//! Two columns only (Original A / Modified B). They stay editable while you
//! type; after a short idle with both sides filled they flip to the
//! highlighted diff in-place (click a column to edit again). No Diff panel.

mod inputs;
mod toolbar;

use gpui::*;
use halo_bridge::BridgeState;

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

use inputs::render_inputs;
use toolbar::render_toolbar;

pub fn render_comparer(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .id("comparer-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_toolbar(state, editors, cx))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .child(render_inputs(state, editors, cx)),
        )
}

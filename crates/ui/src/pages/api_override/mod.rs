//! API Override page — replace live proxied responses per captured host.

pub mod rule_editor;
pub mod rules_tree;
pub mod toolbar;

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use gpui::*;
use halo_bridge::BridgeState;

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

pub use rule_editor::render_rule_editor;
pub use rules_tree::render_rules_tree;
pub use toolbar::render_override_toolbar;

pub fn render_api_override(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    div()
        .id("api-override-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_override_toolbar(state, editors, cx))
        .child(
            div().w_full().flex_1().min_h_0().overflow_hidden().child(
                ResizablePanelGroup::horizontal("api-override-layout")
                    .with_handle(true)
                    .child(
                        resizable_panel()
                            .size(px(320.))
                            .size_range(px(220.)..px(520.))
                            .child(render_rules_tree(state, editors, cx)),
                    )
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .min_w_0()
                                .overflow_hidden()
                                .child(render_rule_editor(state, editors, cx)),
                        ),
                    ),
            ),
        )
}

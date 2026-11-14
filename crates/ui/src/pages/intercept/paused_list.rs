//! Queue panel — a scrollable master list of intercepted transactions.

use celestia_ui::components::primitive::item::ItemGroup;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::BridgeState;

use super::paused_row::render_paused_row;
use crate::pages::chrome::{empty_state, panel};
use crate::pages::editors::ToolEditors;

pub fn render_paused_list(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let enabled = state.intercept.enabled;
    let selected_id = state.selected_paused_id.as_deref();
    let active_tab_id = state.active_work_tab_id.as_deref().unwrap_or("");
    let tab_paused: Vec<_> = state
        .paused
        .iter()
        .filter(|row| row.tab_id.as_deref() == Some(active_tab_id))
        .collect();
    let tab_count = tab_paused.len();

    let content = if tab_paused.is_empty() {
        let (title, hint) = if enabled {
            (
                "Waiting for traffic…",
                "Requests matching this tab's scope patterns will be paused here for modification.",
            )
        } else {
            (
                "Intercept is paused",
                "Enable Intercept or launch the Managed Browser above to begin intercepting requests.",
            )
        };
        empty_state(title, hint, cx).into_any_element()
    } else {
        let mut group = ItemGroup::new();
        for row in tab_paused {
            let is_selected = selected_id == Some(&row.id);
            group = group.child(render_paused_row(row, is_selected, editors, cx));
        }
        div()
            .id("intercept-queue-list")
            .w_full()
            .h_full()
            .overflow_y_scrollbar()
            .child(group)
            .into_any_element()
    };

    panel(
        "Intercept Queue",
        Some(format!("{tab_count} transaction(s) held in this tab").into()),
        div()
            .w_full()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(content),
        cx,
    )
}

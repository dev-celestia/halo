//! Queue panel displaying the list of intercepted transactions.

use gpui::*;
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

    let content = if tab_paused.is_empty() {
        if enabled {
            empty_state(
                "Waiting for traffic…",
                "Requests matching this tab's scope patterns will be paused here for modification.",
                cx,
            )
            .into_any_element()
        } else {
            empty_state(
                "Intercept is paused",
                "Enable Intercept or launch the Managed Browser above to begin intercepting requests.",
                cx,
            )
            .into_any_element()
        }
    } else {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(tab_paused.into_iter().map(|row| {
                let is_selected = selected_id == Some(&row.id);
                render_paused_row(row, is_selected, editors, cx)
            }))
            .into_any_element()
    };

    let tab_count = state
        .paused
        .iter()
        .filter(|row| row.tab_id.as_deref() == Some(active_tab_id))
        .count();

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

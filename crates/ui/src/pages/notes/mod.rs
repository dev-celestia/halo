//! Notes (Scratchpad) tool page — sidebar note list + markdown editor.

mod chrome;
mod drawing_dialog;
mod editor_pane;
mod empty_states;
pub mod helpers;
mod library_dialog;
mod sidebar;

use drawing_dialog::render_drawing_dialog;
use editor_pane::render_notes_editor_pane;
use empty_states::{render_notes_empty_state, render_notes_search_empty};
use library_dialog::render_saved_notes_dialog;
use sidebar::render_notes_sidebar;

use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder;
use gpui::*;
use halo_bridge::BridgeState;

use super::editors::ToolEditors;

pub fn render_notes(
    state: &BridgeState,
    editors: &ToolEditors,
    _window: &mut Window,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();

    if !state.notes_loaded {
        return div()
            .id("notes-page-loading")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child("Loading notes…")
            .into_any_element();
    }

    let open_notes = state.filtered_open_notes();
    let has_search = !state.notes_search_query.trim().is_empty();

    let body = if state.notes_open_tab_ids.is_empty() {
        render_notes_empty_state(state.notes.len(), cx).into_any_element()
    } else if open_notes.is_empty() && has_search {
        render_notes_search_empty(&state.notes_search_query, state.notes.len(), editors, cx)
            .into_any_element()
    } else if let Some(active_note) = state.active_note() {
        render_notes_editor_pane(active_note, state, editors, cx).into_any_element()
    } else {
        render_notes_empty_state(state.notes.len(), cx).into_any_element()
    };

    div()
        .id("notes-page")
        .relative()
        .size_full()
        .flex()
        .flex_row()
        .min_h_0()
        .overflow_hidden()
        .bg(theme.background)
        .child(render_notes_sidebar(state, editors, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(body),
        )
        .when(state.notes_library_open, |this| {
            this.child(render_saved_notes_dialog(state, editors, cx))
        })
        .when(state.notes_drawing_open, |this| {
            this.child(render_drawing_dialog(state, cx))
        })
        .into_any_element()
}

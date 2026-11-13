//! Coder IDE — open a folder, browse files, edit, and save.
//!
//! Coding surfaces only. Source Control lives in
//! [`crate::pages::source_control`]. The status bar and terminal Git Log tab
//! are thin hosts for that tool.
//!
//! - `onboarding` — empty-state welcome when no project is open
//! - `status_bar` — bottom status strip
//! - `workspace` — resizable pane layout
//! - `run_panel` — Run/Build side panel (opened from the status bar)
//! - `sidebar` — explorer host
//! - `explorer` — file tree and its context menus
//! - `editor_pane` — tab strip and editor surface
//! - `terminal` — docked CLI terminal
//! - `image_preview` — read-only image tabs

mod editor_pane;
mod explorer;
mod image_preview;
mod lang;
mod onboarding;
mod quick_open;
mod run_panel;
mod sidebar;
mod status_bar;
mod terminal;
mod workspace;

pub(crate) use terminal::render_terminal_panel;

use std::path::Path;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::BridgeState;

use super::editors::ToolEditors;

/// Trailing path segment of a project root, for chips and headers.
pub(crate) fn project_display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}

pub fn render_coder(
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let has_root = state.coder.root_path.is_some();
    // Marks are rasterized 1:1 with the device grid.
    let scale = window.scale_factor();

    div()
        .id("coder-page")
        .relative()
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .child(if has_root {
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(workspace::render_workspace(state, editors, window, cx))
                .into_any_element()
        } else {
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(onboarding::render_onboarding(state, scale, cx))
                .into_any_element()
        })
        .child(status_bar::render_coder_status_bar(state, editors, cx))
        .when(has_root && state.coder.quick_open_open, |el| {
            el.child(quick_open::render_quick_open(state, editors, cx))
        })
}

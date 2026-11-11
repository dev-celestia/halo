//! Two-pane Source Control review: changes list on the leading edge, diff
//! filling the rest.

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::BridgeState;

use super::render_changes_panel;
use crate::pages::editors::ToolEditors;
use crate::pages::source_control::diff_view;

/// Changes column in the Source Control review workspace.
const CHANGES_WIDTH: Pixels = px(360.);
const CHANGES_MIN: Pixels = px(280.);
const CHANGES_MAX: Pixels = px(560.);

pub fn render_review(state: &BridgeState, editors: &ToolEditors, cx: &mut App) -> impl IntoElement {
    ResizablePanelGroup::horizontal("sc-review")
        .with_handle(true)
        .child(
            resizable_panel()
                .size(CHANGES_WIDTH)
                .size_range(CHANGES_MIN..CHANGES_MAX)
                .child(
                    div()
                        .size_full()
                        .overflow_hidden()
                        .child(render_changes_panel(state, editors, cx)),
                ),
        )
        .child(
            resizable_panel().child(
                div()
                    .size_full()
                    .min_w_0()
                    .overflow_hidden()
                    .child(render_diff_pane(state, cx)),
            ),
        )
}

fn render_diff_pane(state: &BridgeState, cx: &App) -> AnyElement {
    if let Some(diff) = state.git_diff.clone() {
        return diff_view::diff_pane(&diff, state.git_diff_split, state.git_diff_wrap, cx)
            .into_any_element();
    }

    let git = &state.git;
    let message = if git.not_a_repo {
        "Open a folder inside a git worktree to use Source Control."
    } else if git.staged.is_empty() && git.unstaged.is_empty() && git.untracked.is_empty() {
        "No changes — working tree clean."
    } else {
        "Select a file to review its diff."
    };

    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .px_3()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(message)
        .into_any_element()
}

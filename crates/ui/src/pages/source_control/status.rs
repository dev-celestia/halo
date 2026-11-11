//! Source Control chips for the coder status bar.

use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, WorkKind};

fn chip_with_icon(
    icon: PhosphorIcon,
    label: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius_sm())
        .hover(|s| s.bg(cx.theme().accent.opacity(0.5)))
        .child(Phosphor::new(icon).size(px(12.)))
        .child(label.into())
}

fn open_source_control(_: &gpui::ClickEvent, _: &mut Window, cx: &mut App) {
    let _ = Bridge::focus_or_open_work_tab(WorkKind::SourceControl, cx);
}

/// Branch chip — opens the Source Control tool when clicked.
pub(crate) fn render_branch_chip(state: &BridgeState, cx: &App) -> impl IntoElement {
    let git = &state.git;
    let branch = if git.not_a_repo {
        "No repository".to_string()
    } else {
        let mut label = git.branch.clone().unwrap_or_else(|| "—".into());
        if git.ahead > 0 || git.behind > 0 {
            label.push_str(&format!(" ↑{} ↓{}", git.ahead, git.behind));
        }
        label
    };

    div()
        .id("coder-status-branch")
        .cursor_pointer()
        .on_click(open_source_control)
        .child(chip_with_icon(PhosphorIcon::GitBranch, branch, cx))
}

/// Dirty-file count chip (hidden when the tree is clean).
pub(crate) fn render_changes_chip(state: &BridgeState, cx: &App) -> AnyElement {
    let git = &state.git;
    let changes = git.staged.len() + git.unstaged.len() + git.untracked.len();
    if changes == 0 {
        return div().into_any_element();
    }

    div()
        .id("coder-status-changes")
        .cursor_pointer()
        .on_click(open_source_control)
        .child(chip_with_icon(
            PhosphorIcon::Circle,
            format!("{changes} changes"),
            cx,
        ))
        .into_any_element()
}

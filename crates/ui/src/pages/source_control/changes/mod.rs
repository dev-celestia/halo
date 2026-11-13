//! Source Control changes column — commit box, sync actions, and file lists.

pub mod branch_bar;
pub mod composer;
pub mod file_row;
pub mod list;
pub mod menus;
pub mod review;

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::BridgeState;

use self::branch_bar::{branch_bar, busy_row};
use self::composer::{commit_composer, selection_footer};
use self::list::GitCaps;
pub use self::list::GitChangesList;
use self::menus::GitMenuCtx;
pub use self::review::render_review;
use crate::pages::editors::ToolEditors;

pub fn render_changes_panel(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &mut App,
) -> impl IntoElement {
    let git = &state.git;
    let busy = state.git_busy.clone();
    let caps = GitCaps::new(git, busy.is_some());
    let selection = state.git_selection.clone();
    let commit_input = editors.sc_commit_msg.clone();
    let branch_input = editors.sc_branch_name.clone();
    let editor_pool = editors.coder_editors.clone();
    let sign_off = state.coder_settings.sign_off;
    let active_diff = state.git_diff.as_ref().map(|d| (d.path.clone(), d.staged));
    let ignored_open = state.git_ignored_open;
    let repo_root = git
        .repo_root
        .clone()
        .or_else(|| state.coder.root_path.clone())
        .unwrap_or_default();

    let mut body = div()
        .id("sc-panel")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden();

    if let Some(alert) = super::render_git_alert(state, cx) {
        body = body.child(div().px_3().py_1().child(alert));
    }

    if git.not_a_repo {
        body = body.child(
            div()
                .flex_1()
                .min_h_0()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Open a folder inside a git worktree to use Source Control."),
        );
    } else {
        body = body.child(branch_bar(
            git,
            &branch_input,
            &GitMenuCtx {
                commit_input: commit_input.clone(),
                remotes: git.remotes.clone(),
                local_branches: git.local_branches.clone(),
                caps,
            },
            state.git_new_branch,
            cx,
        ));

        if let Some(label) = &busy {
            body = body.child(busy_row(label, cx));
        }

        body = body.child(commit_composer(git, &commit_input, caps, sign_off, cx));

        let all_clean = !caps.has_staged && !caps.has_unstaged && git.ignored.is_empty();
        if all_clean {
            body = body.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No changes — working tree clean."),
            );
        } else {
            let changes_list = editors.sc_changes_list.clone();
            changes_list.update(cx, |list, _cx| {
                list.sync(
                    git,
                    &repo_root,
                    &editor_pool,
                    &selection,
                    active_diff.as_ref(),
                    ignored_open,
                );
            });

            body = body.child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .pt_1()
                    .overflow_hidden()
                    .child(changes_list),
            );

            if !selection.is_empty() {
                body = body.child(selection_footer(git, &selection, cx));
            }
        }
    }

    body
}

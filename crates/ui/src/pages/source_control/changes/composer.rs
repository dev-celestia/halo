//! Commit message composer and selection bulk actions.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::{Input, InputState};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, GitState};

use super::list::GitCaps;

/// Renders the commit message input box and the primary Commit button.
pub fn commit_composer(
    git: &GitState,
    commit_input: &Entity<InputState>,
    caps: GitCaps,
    sign_off: bool,
    cx: &App,
) -> impl IntoElement {
    let mut commit_box = div()
        .w_full()
        .flex_none()
        .px_3()
        .pb_2()
        .flex()
        .flex_col()
        .gap_1p5()
        .child(Input::new(commit_input));

    if sign_off {
        commit_box = commit_box.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground.opacity(0.8))
                .child("A Signed-off-by trailer is appended to this commit."),
        );
    }

    let commit_label = if caps.has_staged {
        format!("Commit ({})", git.staged.len())
    } else {
        "Commit".to_string()
    };

    commit_box.child(
        Button::new("sc-commit")
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Small)
            .label(commit_label)
            .disabled(!caps.has_staged)
            .on_click({
                let commit_input = commit_input.clone();
                move |_, window, cx| {
                    let msg = commit_input.read(cx).value().to_string();
                    if msg.trim().is_empty() {
                        commit_input.update(cx, |input, cx| input.focus(window, cx));
                        return;
                    }
                    if Bridge::git_commit(&msg, cx).is_ok() {
                        commit_input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                    }
                }
            }),
    )
}

/// Renders bulk actions footer for the ticked rows in the staging list.
pub fn selection_footer(git: &GitState, selection: &[String], cx: &App) -> impl IntoElement {
    let selected_staged = git
        .staged
        .iter()
        .filter(|f| selection.iter().any(|p| p == &f.path))
        .count();
    let selected_unstaged = git
        .unstaged
        .iter()
        .chain(git.untracked.iter())
        .filter(|f| selection.iter().any(|p| p == &f.path))
        .count();

    let mut actions = div()
        .w_full()
        .flex_none()
        .px_3()
        .py_1p5()
        .border_t_1()
        .border_color(cx.theme().border)
        .flex()
        .items_center()
        .gap_1p5()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{} selected", selection.len())),
        );

    if selected_staged > 0 {
        actions = actions.child(
            Button::new("sc-unstage-selected")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::XSmall)
                .leading_icon(PhosphorIcon::Minus)
                .label(format!("Unstage ({selected_staged})"))
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_unstage_selected(cx);
                }),
        );
    }
    if selected_unstaged > 0 {
        actions = actions.child(
            Button::new("sc-stage-selected")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::XSmall)
                .leading_icon(PhosphorIcon::Plus)
                .label(format!("Stage ({selected_unstaged})"))
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_stage_selected(cx);
                }),
        );
    }
    actions.child(
        Button::new("sc-clear-selection")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::XSmall)
            .leading_icon(PhosphorIcon::X)
            .tooltip("Clear Selection")
            .on_click(|_, _, cx| {
                Bridge::git_clear_selection(cx);
            }),
    )
}

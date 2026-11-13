//! Branch row — branch chip, ahead/behind, and the remote-action icons on one line.

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::{Input, InputState};
use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, GitState};

use super::menus::{GitMenuCtx, changes_more_menu};

/// Pull / Push / Fetch / Sync / Refresh, icon-only.
///
/// Sits on the trailing edge of the branch row. The cluster does not shrink,
/// so a long branch name clips instead of hiding a remote action. Tooltips
/// carry the action names.
fn sync_actions(ctx: &GitMenuCtx) -> impl IntoElement {
    let caps = ctx.caps;
    div()
        .id("sc-sync-actions")
        .flex_none()
        .flex()
        .items_center()
        .gap_1()
        .child(
            Button::new("sc-pull")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::ArrowDown)
                .disabled(!caps.can_sync_with_remote())
                .tooltip("Pull from the tracked upstream")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_pull(cx);
                }),
        )
        .child(
            Button::new("sc-push")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::ArrowUp)
                .disabled(!caps.can_sync_with_remote())
                .tooltip("Push the current branch")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_push(cx);
                }),
        )
        .child(
            Button::new("sc-fetch")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::CloudArrowDown)
                .disabled(!caps.can_fetch())
                .tooltip("Fetch from remotes")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_fetch(cx);
                }),
        )
        .child(
            Button::new("sc-sync")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::ArrowsClockwise)
                .disabled(!caps.can_sync_with_remote())
                .tooltip("Sync — pull, then push")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_sync(cx);
                }),
        )
        .child(
            Button::new("sc-refresh")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .disabled(caps.busy)
                .tooltip("Refresh")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_refresh(cx);
                }),
        )
        .child(
            Button::new("sc-more")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::DotsThreeVertical)
                .tooltip("More Actions…")
                .dropdown_menu({
                    let ctx = ctx.clone();
                    move |menu, window, menu_cx| changes_more_menu(menu, window, menu_cx, &ctx)
                }),
        )
}

/// Progress row shown while a push/pull/fetch/sync runs on the bridge worker.
pub(crate) fn busy_row(label: &str, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .flex_none()
        .px_3()
        .pb_1()
        .flex()
        .items_center()
        .gap_1()
        .child(
            Loader::grid_shift()
                .size(px(16.))
                .color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{label}…")),
        )
}

/// Inline replacement for the branch chip while a new branch is being named.
/// Remote actions stay on the same line, trailing the name field.
pub(crate) fn new_branch_bar(branch_input: &Entity<InputState>, menu: &GitMenuCtx) -> Div {
    let caps = menu.caps;
    div()
        .w_full()
        .min_w_0()
        .flex_none()
        .px_3()
        .pt_2()
        .pb_2()
        .flex()
        .flex_nowrap()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_1()
                .child(div().flex_1().min_w_0().child(Input::new(branch_input)))
                .child(
                    Button::new("sc-create-branch")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::XSmall)
                        .label("Create")
                        .disabled(caps.busy)
                        .on_click({
                            let branch_input = branch_input.clone();
                            move |_, window, cx| {
                                let name = branch_input.read(cx).value().to_string();
                                if Bridge::git_create_branch(&name, cx).is_ok() {
                                    branch_input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                    });
                                }
                            }
                        }),
                )
                .child(
                    Button::new("sc-cancel-branch")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .leading_icon(PhosphorIcon::X)
                        .tooltip("Cancel")
                        .on_click(|_, _, cx| {
                            Bridge::git_cancel_new_branch(cx);
                        }),
                ),
        )
        .child(sync_actions(menu))
}

/// One toolbar: branch identity on the leading edge, remote actions trailing.
///
/// The upstream name lives in the page footer, so this row does not repeat it.
pub(crate) fn branch_bar(
    git: &GitState,
    branch_input: &Entity<InputState>,
    menu: &GitMenuCtx,
    new_branch: bool,
    cx: &App,
) -> Div {
    let caps = menu.caps;
    if new_branch {
        return new_branch_bar(branch_input, menu);
    }

    let current = git.branch.clone().unwrap_or_default();
    let mut branches = git.local_branches.clone();
    if !current.is_empty() && !branches.iter().any(|b| b == &current) {
        branches.insert(0, current.clone());
    }
    let chip_label = if current.is_empty() {
        "detached HEAD".to_string()
    } else {
        current.clone()
    };
    let chip_tooltip = if current.is_empty() {
        "Switch or create branch".to_string()
    } else {
        format!("{current} — switch or create branch")
    };

    let mut identity = div()
        .min_w_0()
        .flex_shrink_1()
        .flex()
        .flex_nowrap()
        .items_center()
        .gap_1()
        .child(
            div().min_w_0().overflow_hidden().flex_shrink_1().child(
                Button::new("sc-branch")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::GitBranch)
                    .label(chip_label)
                    .dropdown_caret(true)
                    .tooltip(chip_tooltip)
                    .disabled(caps.busy)
                    .dropdown_menu({
                        let branch_input = branch_input.clone();
                        move |menu, _window, _cx| {
                            let mut menu = menu;
                            for branch in &branches {
                                let is_current = branch == &current;
                                let name = branch.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(name.clone())
                                        .checked(is_current)
                                        .on_click(move |_, _, cx| {
                                            if !is_current {
                                                let _ = Bridge::git_checkout(&name, cx);
                                            }
                                        }),
                                );
                            }
                            if !branches.is_empty() {
                                menu = menu.separator();
                            }
                            let branch_input = branch_input.clone();
                            menu.item(PopupMenuItem::new("Create New Branch…").on_click(
                                move |_, window, cx| {
                                    Bridge::git_begin_new_branch(cx);
                                    let branch_input = branch_input.clone();
                                    branch_input.update(cx, |input, cx| input.focus(window, cx));
                                },
                            ))
                        }
                    }),
            ),
        );

    if git.upstream.is_some() {
        identity = identity
            .child(sync_counter(
                "sc-behind",
                PhosphorIcon::ArrowDown,
                git.behind,
                cx.theme().warning,
                "pull",
                cx,
            ))
            .child(sync_counter(
                "sc-ahead",
                PhosphorIcon::ArrowUp,
                git.ahead,
                cx.theme().success,
                "push",
                cx,
            ));
    }

    div()
        .w_full()
        .min_w_0()
        .flex_none()
        .px_3()
        .pt_2()
        .pb_2()
        .flex()
        .flex_nowrap()
        .items_center()
        .gap_2()
        .child(identity)
        .child(div().flex_1().min_w_0())
        .child(sync_actions(menu))
}

fn sync_counter(
    id: &'static str,
    icon: PhosphorIcon,
    count: u32,
    accent: Hsla,
    verb: &'static str,
    cx: &App,
) -> impl IntoElement {
    let active = count > 0;
    let color = if active {
        accent
    } else {
        cx.theme().muted_foreground.opacity(0.55)
    };
    let tooltip = if count == 1 {
        format!("1 commit to {verb}")
    } else {
        format!("{count} commits to {verb}")
    };
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap_0p5()
        .px_1()
        .py_0p5()
        .rounded(px(4.))
        .bg(if active {
            accent.opacity(0.12)
        } else {
            cx.theme().muted.opacity(0.35)
        })
        .text_xs()
        .text_color(color)
        .child(Phosphor::new(icon).size(px(11.)).color(color))
        .child(format!("{count}"))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
}

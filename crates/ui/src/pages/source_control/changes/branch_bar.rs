//! Branch switcher chip, new branch inline input, sync bar, and busy indicator.

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::{Input, InputState};
use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, GitState};

use super::list::GitCaps;

/// Pull / Push / Fetch / Sync — gated by [`GitCaps`], always visible so the
/// user does not have to dig into the More menu for the common remote ops.
pub(crate) fn sync_bar(caps: GitCaps, _cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .flex_none()
        .px_3()
        .pb_2()
        .flex()
        .items_center()
        .gap_1p5()
        .child(
            Button::new("sc-pull")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::XSmall)
                .leading_icon(PhosphorIcon::ArrowDown)
                .label("Pull")
                .disabled(!caps.can_sync_with_remote())
                .tooltip("Pull from the tracked upstream")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_pull(cx);
                }),
        )
        .child(
            Button::new("sc-push")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::XSmall)
                .leading_icon(PhosphorIcon::ArrowUp)
                .label("Push")
                .disabled(!caps.can_sync_with_remote())
                .tooltip("Push the current branch")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_push(cx);
                }),
        )
        .child(
            Button::new("sc-fetch")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::XSmall)
                .leading_icon(PhosphorIcon::CloudArrowDown)
                .label("Fetch")
                .disabled(!caps.can_fetch())
                .tooltip("Fetch from remotes")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_fetch(cx);
                }),
        )
        .child(
            Button::new("sc-sync")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::XSmall)
                .leading_icon(PhosphorIcon::ArrowsClockwise)
                .label("Sync")
                .disabled(!caps.can_sync_with_remote())
                .tooltip("Pull then push")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_sync(cx);
                }),
        )
        .child(
            Button::new("sc-refresh")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Icon)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .disabled(caps.busy)
                .tooltip("Refresh")
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_refresh(cx);
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
pub(crate) fn new_branch_bar(branch_input: &Entity<InputState>, busy: bool) -> Div {
    div()
        .w_full()
        .flex_none()
        .px_3()
        .pb_1()
        .flex()
        .items_center()
        .gap_1()
        .child(div().flex_1().min_w_0().child(Input::new(branch_input)))
        .child(
            Button::new("sc-create-branch")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::XSmall)
                .label("Create")
                .disabled(busy)
                .on_click({
                    let branch_input = branch_input.clone();
                    move |_, window, cx| {
                        let name = branch_input.read(cx).value().to_string();
                        if Bridge::git_create_branch(&name, cx).is_ok() {
                            branch_input.update(cx, |input, cx| input.set_value("", window, cx));
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
        )
}

/// Branch chip plus ahead/behind counters and the tracked upstream.
pub(crate) fn branch_bar(
    git: &GitState,
    branch_input: &Entity<InputState>,
    new_branch: bool,
    busy: bool,
    cx: &App,
) -> Div {
    if new_branch {
        return new_branch_bar(branch_input, busy);
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

    let mut bar = div()
        .w_full()
        .flex_none()
        .px_3()
        .pb_1()
        .flex()
        .items_center()
        .gap_1()
        .child(
            Button::new("sc-branch")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::XSmall)
                .leading_icon(PhosphorIcon::GitBranch)
                .label(chip_label)
                .dropdown_caret(true)
                .tooltip(chip_tooltip)
                .disabled(busy)
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
        );

    if git.upstream.is_some() {
        bar = bar
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
    if let Some(upstream) = git.upstream.clone() {
        bar = bar.child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground.opacity(0.8))
                .child(upstream),
        );
    }

    bar
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

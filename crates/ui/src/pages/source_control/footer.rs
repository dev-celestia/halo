//! Full-width bottom bar for Source Control.
//!
//! The project switcher, the upstream branch name, and the terminal live here.
//! Pane navigation is the sidebar. More Actions sits beside Refresh.

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::traits::Selectable as _;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use super::projects::render_project_switcher;

pub(crate) fn render_sc_bar(state: &BridgeState, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let terminal_open = state.coder.terminal_open;
    let upstream = state.git.upstream.clone();

    div()
        .id("sc-universal-bar")
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .px_1()
        .py_1()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.tab_bar)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .flex()
                .flex_row()
                .justify_start()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .child(render_project_switcher(state, cx)),
                )
                .when_some(upstream, |row, name| {
                    let tip = name.clone();
                    row.child(
                        div()
                            .id("sc-upstream")
                            .min_w_0()
                            .overflow_hidden()
                            .truncate()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                            .child(name),
                    )
                }),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new("sc-open-git-log")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .leading_icon(PhosphorIcon::GitCommit)
                        .tooltip("Git log")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::coder_terminal_open_git_log(cx);
                        }),
                )
                .child(
                    Button::new("sc-terminal-toggle")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .leading_icon(PhosphorIcon::TerminalWindow)
                        .selected(terminal_open)
                        .tooltip(if terminal_open {
                            "Hide terminal"
                        } else {
                            "Terminal — run git"
                        })
                        .on_click(|_, _, cx| {
                            let _ = Bridge::coder_toggle_terminal(cx);
                        }),
                ),
        )
}

//! Git Output dock — every failed git command with git's own output.
//!
//! The Branch Explorer deliberately does not paint errors inline: raw stderr
//! is multi-line and pushes the tree around. Failures land here instead, the
//! way a terminal would show them, and the dock stays out of the way until
//! something actually fails.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::{ActiveTheme as _, Theme};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, SourceControlPane};

use super::soft_chip;

pub const OUTPUT_HEIGHT: Pixels = px(200.);
pub const OUTPUT_MIN: Pixels = px(90.);
pub const OUTPUT_MAX: Pixels = px(420.);

pub fn render_git_output(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    let count = state.git_output.len();
    let busy = state.git_busy.is_some();

    // A refused checkout is the one failure with a safe one-click recovery;
    // the inline card lives in Changes, so surface the action here too.
    let recovery = state.git.checkout_block.as_ref().map(|block| {
        let names = if block.files.len() == 1 {
            "1 file".to_string()
        } else {
            format!("{} files", block.files.len())
        };
        (block.branch.clone(), names)
    });

    let count_color = if count > 0 {
        theme.danger
    } else {
        theme.muted_foreground
    };

    div()
        .id("sc-git-output")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.background)
        // No border_t — the vertical split's resize handle is the separator.
        .child(
            div()
                .w_full()
                .flex_none()
                .h(px(30.))
                .px_3()
                .flex()
                .items_center()
                .gap_2()
                .bg(theme.title_bar)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    Phosphor::new(PhosphorIcon::TerminalWindow)
                        .size(px(13.))
                        .color(theme.muted_foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("Git Output"),
                )
                .child(soft_chip(format!("{count}"), count_color, theme))
                .child(div().flex_1())
                .when_some(recovery, |d, (branch, names)| {
                    d.child(
                        div()
                            .flex_none()
                            .min_w_0()
                            .max_w(px(320.))
                            .truncate()
                            .text_xs()
                            .text_color(theme.danger.opacity(0.85))
                            .child(format!(
                                "Can't switch to '{branch}': {names} would be overwritten"
                            )),
                    )
                    .child(
                        Button::new("sc-git-output-stash")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::XSmall)
                            .leading_icon(PhosphorIcon::Archive)
                            .label("Stash & Switch")
                            .disabled(busy)
                            .tooltip("Stash local changes, then switch to this branch")
                            .on_click(|_, _, cx| {
                                let _ = Bridge::git_recover_checkout(cx);
                            }),
                    )
                    .child(
                        Button::new("sc-git-output-changes")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::XSmall)
                            .label("View Changes")
                            .on_click(|_, _, cx| {
                                Bridge::select_source_control_pane(SourceControlPane::Changes, cx);
                            }),
                    )
                })
                .child(
                    Button::new("sc-git-output-clear")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .leading_icon(PhosphorIcon::Trash)
                        .disabled(count == 0)
                        .tooltip("Clear output")
                        .on_click(|_, _, cx| Bridge::git_output_clear(cx)),
                )
                .child(
                    Button::new("sc-git-output-hide")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .leading_icon(PhosphorIcon::CaretDown)
                        .tooltip("Hide Git Output")
                        .on_click(|_, _, cx| Bridge::git_output_close(cx)),
                ),
        )
        .child(
            div()
                .id("sc-git-output-body")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .px_3()
                .py_2()
                .font_family(mono)
                .text_xs()
                .line_height(px(16.))
                .child(if count == 0 {
                    div()
                        .text_color(theme.muted_foreground)
                        .child("No git errors. Failed commands appear here with git's output.")
                        .into_any_element()
                } else {
                    entries(state, theme).into_any_element()
                }),
        )
}

/// Newest failure first — the last thing that went wrong is what needs reading.
fn entries(state: &BridgeState, theme: &Theme) -> impl IntoElement {
    let mut list = div().w_full().flex().flex_col().gap_2();
    for entry in state.git_output.iter().rev() {
        let mut row = div().w_full().flex().flex_col().gap_0p5().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .text_color(theme.muted_foreground.opacity(0.8))
                        .child(entry.timestamp.clone()),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(entry.label.clone()),
                ),
        );

        // One row per line: git indents colliding paths with tabs and a single
        // text run would collapse that indentation. Tabs are expanded here for
        // the same reason — the text engine does not advance them.
        for line in entry.detail.lines() {
            let line = if line.is_empty() {
                " ".to_string()
            } else {
                line.replace('\t', "    ").trim_end().to_string()
            };
            row = row.child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_color(theme.danger.opacity(0.9))
                    .child(line),
            );
        }

        list = list.child(row);
    }
    list
}

//! Shared header bars, toggles, gutters, and styled code line rendering for diffs.

use std::ops::Range;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, GitDiff};

use super::model::{FileDiff, Hunk};
use super::syntax::{Lang, Palette, SyntaxState, combine_line_highlights, tokenize_line};

pub(crate) fn diff_header(
    diff: &GitDiff,
    file: &FileDiff,
    split: bool,
    wrap: bool,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let path = diff.path.clone();
    let staged = diff.staged;

    let path_label = if let (Some(from), Some(to)) = (&file.renamed_from, &file.renamed_to) {
        format!("{from} → {to}")
    } else {
        diff.path.clone()
    };

    div()
        .w_full()
        .flex_none()
        .px_3()
        .py_1p5()
        .flex()
        .items_center()
        .gap_2()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(path_label),
                )
                .when(file.new_file, |this| {
                    this.child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded(theme.radius_sm())
                            .text_xs()
                            .bg(theme.success.opacity(0.16))
                            .text_color(theme.success)
                            .child("New file"),
                    )
                })
                .when(file.deleted_file, |this| {
                    this.child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded(theme.radius_sm())
                            .text_xs()
                            .bg(theme.danger.opacity(0.16))
                            .text_color(theme.danger)
                            .child("Deleted"),
                    )
                })
                .when(file.binary, |this| {
                    this.child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded(theme.radius_sm())
                            .text_xs()
                            .bg(theme.muted.opacity(0.4))
                            .text_color(theme.muted_foreground)
                            .child("Binary"),
                    )
                })
                .when_some(file.mode_change.as_ref(), |this, mode| {
                    this.child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded(theme.radius_sm())
                            .text_xs()
                            .bg(theme.muted.opacity(0.4))
                            .text_color(theme.muted_foreground)
                            .child(mode.clone()),
                    )
                })
                .when(file.additions > 0 || file.deletions > 0, |this| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .font_family(theme.mono_font_family.clone())
                            .child(
                                div()
                                    .text_color(theme.success)
                                    .child(format!("+{}", file.additions)),
                            )
                            .child(
                                div()
                                    .text_color(theme.danger)
                                    .child(format!("-{}", file.deletions)),
                            ),
                    )
                }),
        )
        .child(
            Button::new("sc-diff-more")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .leading_icon(PhosphorIcon::DotsThreeVertical)
                .tooltip("Diff actions")
                .dropdown_menu(move |menu, _, _| {
                    let path = path.clone();
                    let mut menu = if staged {
                        menu.item(
                            PopupMenuItem::new("Unstage")
                                .icon(PhosphorIcon::Minus)
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::git_unstage(&path, cx);
                                }),
                        )
                    } else {
                        menu.item(
                            PopupMenuItem::new("Stage")
                                .icon(PhosphorIcon::Plus)
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::git_stage(&path, cx);
                                }),
                        )
                    };
                    menu = menu
                        .separator()
                        .item(
                            PopupMenuItem::new("Unified")
                                .icon(PhosphorIcon::Rows)
                                .checked(!split)
                                .on_click(|_, _, cx| {
                                    if Bridge::state(cx).read(cx).git_diff_split {
                                        Bridge::git_toggle_split(cx);
                                    }
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Split")
                                .icon(PhosphorIcon::Columns)
                                .checked(split)
                                .on_click(|_, _, cx| {
                                    if !Bridge::state(cx).read(cx).git_diff_split {
                                        Bridge::git_toggle_split(cx);
                                    }
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Wrap")
                                .icon(PhosphorIcon::ArrowsInLineHorizontal)
                                .checked(wrap)
                                .on_click(|_, _, cx| {
                                    if !Bridge::state(cx).read(cx).git_diff_wrap {
                                        Bridge::git_toggle_wrap(cx);
                                    }
                                }),
                        )
                        .item(
                            PopupMenuItem::new("Unwrap")
                                .icon(PhosphorIcon::ArrowLineRight)
                                .checked(!wrap)
                                .on_click(|_, _, cx| {
                                    if Bridge::state(cx).read(cx).git_diff_wrap {
                                        Bridge::git_toggle_wrap(cx);
                                    }
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Close Diff")
                                .icon(PhosphorIcon::X)
                                .on_click(|_, _, cx| {
                                    Bridge::git_close_diff(cx);
                                }),
                        );
                    menu
                }),
        )
}

pub(crate) fn hunk_header(
    index: usize,
    hunk: &Hunk,
    file: &FileDiff,
    staged: bool,
    can_stage: bool,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .px_3()
        .py_1()
        .bg(theme.muted.opacity(0.35))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.info)
                        .child(hunk.coords.clone()),
                )
                .when(!hunk.context.is_empty(), |this| {
                    this.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_family(theme.mono_font_family.clone())
                            .text_color(theme.muted_foreground)
                            .child(hunk.context.clone()),
                    )
                }),
        );

    if can_stage {
        let patch = file.hunk_patch(index);
        let reverse = staged;
        let icon = if staged {
            PhosphorIcon::Minus
        } else {
            PhosphorIcon::Plus
        };
        row = row.child(
            Button::new(SharedString::from(format!("sc-hunk-{index}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .leading_icon(icon)
                .tooltip(if staged {
                    "Unstage hunk — remove from the index"
                } else {
                    "Stage hunk — add to the index"
                })
                .on_click(move |_, _, cx| {
                    if let Some(patch) = patch.clone() {
                        let _ = Bridge::git_apply_hunk(&patch, reverse, cx);
                    }
                }),
        );
    }

    row
}

/// Line-number column. Unified stacks two of these side by side, so it passes
/// a narrower width than the split view does.
pub(crate) fn gutter(no: Option<u32>, width: f32, cx: &App) -> Div {
    div()
        .w(px(width))
        .flex_none()
        .pr_1()
        .text_right()
        .text_color(cx.theme().muted_foreground.opacity(0.75))
        .child(no.map(|n| n.to_string()).unwrap_or_default())
}

/// Renders a single code line as a GPUI [`StyledText`] with syntax foreground highlighting
/// and intraline diff background highlights.
pub(crate) fn render_code_text(
    display_text: &str,
    lang: Lang,
    palette: &Palette,
    highlight: bool,
    state: &mut SyntaxState,
    fallback_fg: Hsla,
    intraline_spans: &[Range<usize>],
    intraline_bg: Option<Hsla>,
    wrap: bool,
) -> Div {
    let empty = display_text.is_empty();
    let text = if empty { "\u{00A0}" } else { display_text };

    let syntax = if highlight && !empty {
        tokenize_line(lang, text, palette, state)
    } else {
        Vec::new()
    };

    let highlights = combine_line_highlights(
        text.len(),
        &syntax,
        intraline_spans,
        intraline_bg,
        fallback_fg,
    );

    let styled = StyledText::new(SharedString::from(text.to_string())).with_highlights(highlights);

    let mut el = div().flex_1().min_w_0().child(styled);

    if wrap {
        el = el.whitespace_normal();
    } else {
        el = el.whitespace_nowrap();
    }

    el
}

//! Split (side-by-side) diff hunk, column, and cell rendering.

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;

use super::model::{DIFF_LINE_PX, Hunk, LineKind, nowrap_content_width};
use super::render_common::{gutter, render_code_text};
use super::split::{SplitCell, split_rows};
use super::syntax::{Lang, Palette, SyntaxState};

/// One slot in a split pane column (keeps left/right row counts aligned).
pub(crate) enum PaneEntry {
    Blank,
    Cell(SplitCell),
    Meta(String),
}

impl PaneEntry {
    fn chars_count(&self) -> usize {
        match self {
            PaneEntry::Cell(c) => c.display_text.chars().count(),
            PaneEntry::Meta(m) => m.chars().count(),
            PaneEntry::Blank => 0,
        }
    }
}

/// Two columns for one hunk. In Wrap mode, rows are synchronized flex rows
/// to prevent vertical drift. In No Wrap mode, two horizontally scrollable columns
/// have fixed row height.
pub(crate) fn split_hunk_panes(
    hunk_i: usize,
    hunk: &Hunk,
    lang: Lang,
    palette: &Palette,
    highlight: bool,
    wrap: bool,
    cx: &App,
) -> impl IntoElement {
    let rows = split_rows(hunk);

    if wrap {
        let mut block = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .text_sm()
            .font_family(cx.theme().mono_font_family.clone());

        let mut left_state = SyntaxState::default();
        let mut right_state = SyntaxState::default();
        for row in rows {
            if let Some(meta) = row.meta {
                block = block.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .px_2()
                        .py_0p5()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(meta),
                );
            } else {
                block = block.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_stretch()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .border_r_1()
                                .border_color(cx.theme().border.opacity(0.7))
                                .child(render_split_cell(
                                    row.left.as_ref(),
                                    lang,
                                    palette,
                                    highlight,
                                    &mut left_state,
                                    true,
                                    cx,
                                )),
                        )
                        .child(div().flex_1().min_w_0().child(render_split_cell(
                            row.right.as_ref(),
                            lang,
                            palette,
                            highlight,
                            &mut right_state,
                            true,
                            cx,
                        ))),
                );
            }
        }

        block.into_any_element()
    } else {
        let mut left_lines = Vec::with_capacity(rows.len());
        let mut right_lines = Vec::with_capacity(rows.len());

        for row in rows {
            if let Some(meta) = row.meta {
                left_lines.push(PaneEntry::Meta(meta));
                right_lines.push(PaneEntry::Blank);
            } else {
                left_lines.push(row.left.map(PaneEntry::Cell).unwrap_or(PaneEntry::Blank));
                right_lines.push(row.right.map(PaneEntry::Cell).unwrap_or(PaneEntry::Blank));
            }
        }

        let max_left = left_lines
            .iter()
            .map(|e| e.chars_count())
            .max()
            .unwrap_or(0);
        let max_right = right_lines
            .iter()
            .map(|e| e.chars_count())
            .max()
            .unwrap_or(0);
        let content_w_l = nowrap_content_width(max_left);
        let content_w_r = nowrap_content_width(max_right);

        div()
            .w_full()
            .min_w_0()
            .max_w_full()
            .flex()
            .items_start()
            .overflow_hidden()
            .text_sm()
            .font_family(cx.theme().mono_font_family.clone())
            .child(split_nowrap_column(
                SharedString::from(format!("coder-diff-pane-l-{hunk_i}")),
                left_lines,
                content_w_l,
                lang,
                palette,
                highlight,
                true,
                cx,
            ))
            .child(split_nowrap_column(
                SharedString::from(format!("coder-diff-pane-r-{hunk_i}")),
                right_lines,
                content_w_r,
                lang,
                palette,
                highlight,
                false,
                cx,
            ))
            .into_any_element()
    }
}

pub(crate) fn split_nowrap_column(
    id: SharedString,
    entries: Vec<PaneEntry>,
    content_w: Pixels,
    lang: Lang,
    palette: &Palette,
    highlight: bool,
    leading: bool,
    cx: &App,
) -> impl IntoElement {
    let mut shell = div().flex_1().min_w_0().w_full().overflow_hidden();
    if leading {
        shell = shell
            .border_r_1()
            .border_color(cx.theme().border.opacity(0.7));
    }

    let mut inner = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(content_w)
        .text_sm()
        .font_family(cx.theme().mono_font_family.clone());

    let mut state = SyntaxState::default();
    for entry in entries {
        match entry {
            PaneEntry::Meta(meta) => {
                inner = inner.child(
                    div()
                        .w_full()
                        .flex_none()
                        .h(px(DIFF_LINE_PX))
                        .flex()
                        .items_center()
                        .px_2()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .whitespace_nowrap()
                        .child(meta),
                );
            }
            PaneEntry::Blank => {
                inner = inner.child(render_split_cell(
                    None, lang, palette, highlight, &mut state, false, cx,
                ));
            }
            PaneEntry::Cell(cell) => {
                inner = inner.child(render_split_cell(
                    Some(&cell),
                    lang,
                    palette,
                    highlight,
                    &mut state,
                    false,
                    cx,
                ));
            }
        }
    }

    shell.child(
        div()
            .w_full()
            .min_w_0()
            .child(inner)
            .overflow_x_scrollbar()
            .id(id),
    )
}

pub(crate) fn render_split_cell(
    cell: Option<&SplitCell>,
    lang: Lang,
    palette: &Palette,
    highlight: bool,
    state: &mut SyntaxState,
    wrap: bool,
    cx: &App,
) -> AnyElement {
    let Some(cell) = cell else {
        let mut filler = div()
            .size_full()
            .min_h(px(DIFF_LINE_PX))
            .flex()
            .items_stretch()
            .px_2()
            .bg(cx.theme().muted.opacity(0.18))
            .child(gutter(None, 44., cx))
            .child(div().w(px(12.)).flex_none())
            .child(div().flex_1().min_w_0());
        if !wrap {
            filler = filler.h(px(DIFF_LINE_PX));
        }
        return filler.into_any_element();
    };

    let (tint, fallback) = match cell.kind {
        LineKind::Add => (Some(cx.theme().success.opacity(0.08)), cx.theme().success),
        LineKind::Del => (Some(cx.theme().danger.opacity(0.08)), cx.theme().danger),
        LineKind::Context => (None, cx.theme().foreground),
        LineKind::Meta => (None, cx.theme().muted_foreground),
    };
    let prefix = match cell.kind {
        LineKind::Add => "+",
        LineKind::Del => "-",
        LineKind::Meta => "\\",
        LineKind::Context => " ",
    };
    let intraline_bg = match cell.kind {
        LineKind::Add => Some(cx.theme().success.opacity(0.24)),
        LineKind::Del => Some(cx.theme().danger.opacity(0.24)),
        _ => None,
    };

    let mut row = div()
        .size_full()
        .min_h(px(DIFF_LINE_PX))
        .flex()
        .items_start()
        .px_2()
        .child(gutter(cell.no, 44., cx))
        .child(
            div()
                .w(px(12.))
                .flex_none()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(fallback)
                .child(prefix),
        )
        .child(render_code_text(
            &cell.display_text,
            lang,
            palette,
            highlight && cell.kind != LineKind::Meta,
            state,
            fallback,
            &cell.inline_spans,
            intraline_bg,
            wrap,
        ));

    if !wrap {
        row = row.h(px(DIFF_LINE_PX)).whitespace_nowrap();
    }

    if let Some(tint) = tint {
        row = row.bg(tint);
    }
    row.into_any_element()
}

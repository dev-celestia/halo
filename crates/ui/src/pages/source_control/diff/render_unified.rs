//! Unified (single column stacked) diff hunk and line rendering.

use std::ops::Range;

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;

use super::model::{DIFF_LINE_PX, DiffLine, Hunk, LineKind, nowrap_content_width};
use super::render_common::{gutter, render_code_text};
use super::split::compute_intraline_spans;
use super::syntax::{Lang, Palette, SyntaxState};

pub(crate) fn pair_unified_intraline(lines: &[DiffLine]) -> Vec<Vec<Range<usize>>> {
    let mut spans = vec![Vec::new(); lines.len()];
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == LineKind::Del {
            let mut j = i;
            while j < lines.len() && lines[j].kind == LineKind::Del {
                j += 1;
            }
            let del_end = j;
            while j < lines.len() && lines[j].kind == LineKind::Add {
                j += 1;
            }
            let add_end = j;

            let dels_len = del_end - i;
            let adds_len = add_end - del_end;

            if dels_len > 0 && adds_len > 0 {
                let pair_count = dels_len.min(adds_len);
                for k in 0..pair_count {
                    let d_idx = i + k;
                    let a_idx = del_end + k;
                    let (d_spans, a_spans) = compute_intraline_spans(
                        &lines[d_idx].display_text(),
                        &lines[a_idx].display_text(),
                    );
                    spans[d_idx] = d_spans;
                    spans[a_idx] = a_spans;
                }
            }
            i = add_end.max(i + 1);
        } else {
            i += 1;
        }
    }
    spans
}

/// Unified hunk: one scroll owner for the whole block when No Wrap.
pub(crate) fn unified_hunk_block(
    hunk_i: usize,
    hunk: &Hunk,
    lang: Lang,
    palette: &Palette,
    highlight: bool,
    wrap: bool,
    cx: &App,
) -> impl IntoElement {
    let scroll_id = SharedString::from(format!("coder-diff-unified-{hunk_i}"));
    let max_chars = hunk
        .lines
        .iter()
        .map(|l| l.display_text().chars().count())
        .max()
        .unwrap_or(0);
    let content_w = nowrap_content_width(max_chars);

    let intraline_spans = pair_unified_intraline(&hunk.lines);

    let mut lines = if wrap {
        div().w_full().min_w_0().flex().flex_col()
    } else {
        div().flex().flex_col().flex_none().w(content_w)
    };
    lines = lines
        .text_sm()
        .font_family(cx.theme().mono_font_family.clone());

    let mut old_state = SyntaxState::default();
    let mut new_state = SyntaxState::default();
    for (line_i, line) in hunk.lines.iter().enumerate() {
        let spans = intraline_spans.get(line_i).cloned().unwrap_or_default();
        match line.kind {
            LineKind::Del => {
                lines = lines.child(unified_line(
                    line,
                    lang,
                    palette,
                    highlight,
                    &mut old_state,
                    &spans,
                    wrap,
                    cx,
                ));
            }
            LineKind::Add => {
                lines = lines.child(unified_line(
                    line,
                    lang,
                    palette,
                    highlight,
                    &mut new_state,
                    &spans,
                    wrap,
                    cx,
                ));
            }
            LineKind::Context => {
                lines = lines.child(unified_line(
                    line,
                    lang,
                    palette,
                    highlight,
                    &mut new_state,
                    &spans,
                    wrap,
                    cx,
                ));
                old_state = new_state;
            }
            // Meta lines are not code: they neither highlight nor advance state.
            LineKind::Meta => {
                lines = lines.child(unified_line(
                    line,
                    lang,
                    palette,
                    false,
                    &mut new_state,
                    &spans,
                    wrap,
                    cx,
                ));
            }
        }
    }

    if wrap {
        div()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .child(lines)
            .into_any_element()
    } else {
        div()
            .w_full()
            .min_w_0()
            .child(lines)
            .overflow_x_scrollbar()
            .id(scroll_id)
            .into_any_element()
    }
}

pub(crate) fn unified_line(
    line: &DiffLine,
    lang: Lang,
    palette: &Palette,
    highlight: bool,
    state: &mut SyntaxState,
    intraline_spans: &[Range<usize>],
    wrap: bool,
    cx: &App,
) -> Div {
    let (tint, fallback) = match line.kind {
        LineKind::Add => (Some(cx.theme().success.opacity(0.08)), cx.theme().success),
        LineKind::Del => (Some(cx.theme().danger.opacity(0.08)), cx.theme().danger),
        LineKind::Meta => (None, cx.theme().muted_foreground),
        LineKind::Context => (None, cx.theme().foreground),
    };

    let intraline_bg = match line.kind {
        LineKind::Add => Some(cx.theme().success.opacity(0.24)),
        LineKind::Del => Some(cx.theme().danger.opacity(0.24)),
        _ => None,
    };

    let prefix = match line.kind {
        LineKind::Add => "+",
        LineKind::Del => "-",
        LineKind::Meta => "\\",
        LineKind::Context => " ",
    };

    let mut row = div()
        .flex()
        .items_start()
        .px_3()
        .child(gutter(line.old_no, 32., cx))
        .child(gutter(line.new_no, 32., cx))
        .child(
            div()
                .w(px(12.))
                .flex_none()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(fallback)
                .child(prefix),
        )
        .child(render_code_text(
            &line.display_text(),
            lang,
            palette,
            highlight,
            state,
            fallback,
            intraline_spans,
            intraline_bg,
            wrap,
        ));

    if wrap {
        row = row
            .w_full()
            .min_w_0()
            .min_h(px(DIFF_LINE_PX))
            .overflow_hidden();
    } else {
        row = row.flex_none().h(px(DIFF_LINE_PX)).whitespace_nowrap();
    }

    if let Some(tint) = tint {
        row = row.bg(tint);
    }
    row
}

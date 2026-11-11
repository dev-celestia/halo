//! The Source Control diff pane: unified and split layouts, line tinting,
//! syntax colouring, intraline change highlights, and per-hunk staging.
//!
//! The backend hands us raw `git diff` text, so everything here is presentation.
//! [`model::parse`] turns that text into hunks and numbered lines; [`split::split_rows`] pairs
//! deletions with additions for the side-by-side layout; [`render_common::render_code_text`] shapes
//! and highlights tokens using GPUI [`StyledText`] with word-level intraline diffs.
//!
//! Hunk staging works by rebuilding a one-hunk patch from the file header plus
//! that hunk's raw lines and handing it to `git apply --cached`. Keeping the raw
//! lines means the patch is byte-identical to what git produced, which is what
//! makes `--cached` (and `--reverse` for unstaging) apply cleanly.

pub mod model;
pub mod render_common;
pub mod render_split;
pub mod render_unified;
pub mod split;
pub mod syntax;

mod keywords;

#[cfg(test)]
mod tests;

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::GitDiff;

pub(crate) use model::*;
pub(crate) use render_common::*;
pub(crate) use render_split::*;
pub(crate) use render_unified::*;
pub(crate) use syntax::*;

pub fn diff_pane(diff: &GitDiff, split: bool, wrap: bool, cx: &App) -> Div {
    let parsed = parse(&diff.text);
    let lang = lang_for(&diff.path);
    let highlight = diff.text.lines().count() <= HIGHLIGHT_LINE_LIMIT;
    let palette = palette(cx);

    let can_stage_hunks = if diff.staged {
        !parsed.hunks.is_empty()
    } else {
        !parsed.new_file && !parsed.deleted_file && !parsed.hunks.is_empty()
    };

    let mut body = div().w_full().min_w_0().flex().flex_col().pb_1();
    if parsed.hunks.is_empty() {
        body = body.child(
            div()
                .px_3()
                .py_2()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("No textual changes — binary or mode-only."),
        );
    } else {
        for (index, hunk) in parsed.hunks.iter().enumerate() {
            body = body.child(hunk_header(
                index,
                hunk,
                &parsed,
                diff.staged,
                can_stage_hunks,
                cx,
            ));
            if split {
                body = body.child(split_hunk_panes(
                    index, hunk, lang, &palette, highlight, wrap, cx,
                ));
            } else {
                body = body.child(unified_hunk_block(
                    index, hunk, lang, &palette, highlight, wrap, cx,
                ));
            }
        }
    }

    div()
        .size_full()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(diff_header(diff, &parsed, split, wrap, cx))
        .child(
            div()
                .id("coder-diff-scroll-y")
                .w_full()
                .min_w_0()
                .flex_1()
                .min_h_0()
                .overflow_x_hidden()
                .overflow_y_scroll()
                .child(body),
        )
}

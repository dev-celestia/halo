//! Intruder results table with status / match filters.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, IntruderResultRow};

use crate::pages::chrome::panel;

pub fn render_results_panel(state: &BridgeState, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let selected = state.intruder.selected_result_id.as_deref();
    let matches_only = state.intruder.config.matches_only;
    let status_classes = &state.intruder.config.status_classes;

    let filtered: Vec<&IntruderResultRow> = state
        .intruder
        .results
        .iter()
        .filter(|row| {
            if matches_only && !row.grep_match {
                return false;
            }
            if status_classes.is_empty() {
                return true;
            }
            match row.status {
                Some(code) => status_classes.iter().any(|c| code / 100 == *c),
                None => false,
            }
        })
        .collect();

    let header = div()
        .w_full()
        .px_2()
        .py_1()
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(muted)
        .border_b_1()
        .border_color(border)
        .child(div().w(px(36.)).child("#"))
        .child(div().flex_1().child("Payload"))
        .child(div().w(px(48.)).child("Status"))
        .child(div().w(px(56.)).child("Length"))
        .child(div().w(px(56.)).child("Time"))
        .child(div().w(px(40.)).child("Match"))
        .child(div().w(px(100.)).child("Extracted"));

    let mut rows = div().w_full().flex().flex_col().gap(px(2.)).pt_1();
    if filtered.is_empty() {
        rows = rows.child(if state.intruder.results.is_empty() {
            crate::pages::chrome::empty_state(
                "No results yet",
                "Start an attack to stream responses here.",
                cx,
            )
            .into_any_element()
        } else {
            crate::pages::chrome::empty_state(
                "No matching rows",
                "Clear or change the status and match filters to see results.",
                cx,
            )
            .into_any_element()
        });
    } else {
        for (i, row) in filtered.iter().enumerate() {
            let is_selected = selected == Some(row.id.as_str());
            rows = rows.child(render_result_row(i + 1, row, is_selected, cx));
        }
    }

    panel(
        format!(
            "Results ({} / {})",
            filtered.len(),
            state.intruder.results.len()
        ),
        Some("Click a row to inspect request payload and response.".into()),
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(render_filters(matches_only, status_classes))
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .id("intruder-results-list")
                    .overflow_y_scrollbar()
                    .child(header)
                    .child(rows),
            ),
        cx,
    )
}

fn render_filters(matches_only: bool, status_classes: &[u16]) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap_1()
        .child(
            Button::new("intruder-filter-matches")
                .variant(if matches_only {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Small)
                .label("Matches only")
                .on_click(|_, _, cx| {
                    Bridge::state(cx).update(cx, |s| {
                        s.intruder.config.matches_only = !s.intruder.config.matches_only;
                    });
                }),
        )
        .children([2u16, 3, 4, 5].into_iter().map(|class| {
            let selected = status_classes.contains(&class);
            Button::new(SharedString::from(format!("intruder-status-{class}")))
                .variant(if selected {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Small)
                .label(format!("{class}xx"))
                .on_click(move |_, _, cx| {
                    Bridge::state(cx).update(cx, |s| {
                        let list = &mut s.intruder.config.status_classes;
                        if let Some(pos) = list.iter().position(|c| *c == class) {
                            list.remove(pos);
                        } else {
                            list.push(class);
                        }
                    });
                })
        }))
}

fn render_result_row(
    index: usize,
    row: &IntruderResultRow,
    selected: bool,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let theme = cx.theme();
    let id = row.id.clone();
    let status = row
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into());
    let status_col = status_color(row.status);
    let length = row
        .response_length
        .map(|n| n.to_string())
        .unwrap_or_else(|| "—".into());
    let time = row
        .response_time_ms
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "—".into());
    let match_label = if row.grep_match { "Yes" } else { "—" };
    let extracted = row
        .grep_extracted
        .clone()
        .or_else(|| row.error.clone())
        .unwrap_or_else(|| "—".into());
    let payload = row.payload_summary();
    let grep_match = row.grep_match;

    div()
        .id(SharedString::from(format!("inv-row-{id}")))
        .w_full()
        .px_2()
        .py_1()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(if selected { border } else { border })
        .bg(if selected {
            theme.accent.opacity(0.12)
        } else {
            theme.background
        })
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .cursor_pointer()
        .hover(|s| s.bg(theme.accent.opacity(0.08)))
        .on_click(move |_, _, cx| {
            let _ = Bridge::select_intruder_result(Some(id.clone()), cx);
        })
        .child(div().w(px(36.)).text_color(muted).child(format!("{index}")))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(payload),
        )
        .child(
            div()
                .w(px(48.))
                .font_weight(FontWeight::NORMAL)
                .text_color(status_col)
                .child(status),
        )
        .child(div().w(px(56.)).text_color(muted).child(length))
        .child(div().w(px(56.)).text_color(muted).child(time))
        .child(
            div()
                .w(px(40.))
                .text_color(if grep_match { theme.success } else { muted })
                .child(match_label),
        )
        .child(
            div()
                .w(px(100.))
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(muted)
                .child(extracted),
        )
}

fn status_color(status: Option<u16>) -> Hsla {
    crate::pages::colors::status_color(status)
}

//! Side-by-side payload / response inspector for a selected Intruder result.

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{BridgeState, IntruderResultRow};

use crate::pages::chrome::{empty_state, panel};

pub fn render_result_inspector(state: &BridgeState, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let Some(row) = selected_row(state) else {
        return panel(
            "Inspector",
            Some("Select a result row to inspect payload and response.".into()),
            empty_state(
                "Nothing selected",
                "Choose a result above to compare payload and response.",
                cx,
            ),
            cx,
        )
        .into_any_element();
    };

    let title = format!(
        "{}  →  {}",
        row.payload_summary(),
        row.status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".into())
    );
    let description = format!(
        "{} ms  ·  {} B{}",
        row.response_time_ms.unwrap_or(0),
        row.response_length.unwrap_or(0),
        row.final_url
            .as_ref()
            .map(|u| format!("  ·  {u}"))
            .unwrap_or_default()
    );

    panel(
        title,
        Some(description.into()),
        div()
            .w_full()
            .h_full()
            .flex()
            .gap_2()
            .child(render_pane(
                "Payload",
                &format_payload(row),
                cx.theme().warning.opacity(0.15),
                muted,
            ))
            .child(render_pane(
                "Response",
                &format_response(row),
                if row.grep_match {
                    cx.theme().success.opacity(0.12)
                } else {
                    gpui::transparent_black()
                },
                muted,
            )),
        cx,
    )
    .into_any_element()
}

fn selected_row(state: &BridgeState) -> Option<&IntruderResultRow> {
    let id = state.intruder.selected_result_id.as_deref()?;
    state.intruder.results.iter().find(|r| r.id == id)
}

fn format_payload(row: &IntruderResultRow) -> String {
    if row.payload_values.is_empty() {
        return "(no payload values)".into();
    }
    let mut pairs: Vec<_> = row.payload_values.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_response(row: &IntruderResultRow) -> String {
    if let Some(err) = &row.error {
        return format!("Error: {err}");
    }
    let mut out = String::new();
    if let Some(status) = row.status {
        out.push_str(&format!(
            "HTTP {} {}\n",
            status,
            row.status_text.as_deref().unwrap_or("")
        ));
    }
    if !row.response_headers.is_empty() {
        let mut headers: Vec<_> = row.response_headers.iter().collect();
        headers.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        for (k, v) in headers {
            out.push_str(&format!("{k}: {v}\n"));
        }
        out.push('\n');
    }
    if let Some(body) = &row.response_body {
        const MAX: usize = 8_000;
        if body.len() > MAX {
            out.push_str(&body[..MAX]);
            out.push_str("\n… truncated …");
        } else {
            out.push_str(body);
        }
    }
    if out.is_empty() {
        "(empty response)".into()
    } else {
        out
    }
}

fn render_pane(title: &str, body: &str, tint: Hsla, muted: Hsla) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .min_h(px(160.))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child(title.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .bg(tint)
                .px_2()
                .py_1()
                .id(SharedString::from(format!("intruder-insp-{title}")))
                .overflow_y_scrollbar()
                .child(div().text_xs().whitespace_normal().child(body.to_string())),
        )
}

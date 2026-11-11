//! Response viewer: status strip, Pretty/Raw/Headers/Request tabs.

use celestia_ui::components::primitive::tabs::{Tabs, TabsList, TabsTrigger};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, ForgeResponseTab, RepeaterRequestDto, RepeaterResponseDto};

use crate::pages::chrome::{code_surface, flush_panel};
use crate::pages::editors::ToolEditors;

/// Wire text ends on its own line so the `http` highlight grammar keeps the
/// final body line inside the body node (see `http_format::finish`).
fn finish(out: &mut String) {
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

fn format_pretty(resp: &RepeaterResponseDto) -> String {
    let mut out = String::new();
    out.push_str(&format!("HTTP/1.1 {} {}\n", resp.status, resp.status_text));
    let mut headers: Vec<_> = resp.headers.iter().collect();
    headers.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push('\n');
    out.push_str(&pretty_body(&resp.body));
    finish(&mut out);
    out
}

fn format_raw(resp: &RepeaterResponseDto) -> String {
    let mut out = String::new();
    out.push_str(&format!("HTTP/1.1 {} {}\n", resp.status, resp.status_text));
    let mut headers: Vec<_> = resp.headers.iter().collect();
    headers.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push('\n');
    out.push_str(&resp.body);
    finish(&mut out);
    out
}

fn format_headers_only(resp: &RepeaterResponseDto) -> String {
    let mut headers: Vec<_> = resp.headers.iter().collect();
    headers.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    headers
        .into_iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_sent_request(req: &RepeaterRequestDto) -> String {
    let mut lines: Vec<String> = req
        .headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    lines.sort();
    let mut out = format!(
        "{} {}\n{}\n\n{}",
        req.method,
        req.url,
        lines.join("\n"),
        req.body
    );
    finish(&mut out);
    out
}

fn pretty_body(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    }
}

fn response_size(resp: &RepeaterResponseDto) -> String {
    let bytes = resp.body.len();
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    }
}

fn status_color(status: u16) -> Hsla {
    crate::pages::colors::status_color_u16(status)
}

pub fn render_response_view(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let tab = state.forge_response_tab;
    let response = state.repeater_response.as_ref();

    let meta = response.map(|resp| {
        div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(status_color(resp.status))
                    .child(format!("{} {}", resp.status, resp.status_text)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{} ms", resp.time_ms)),
            )
            .child(div().text_xs().text_color(muted).child(response_size(resp)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(muted)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(resp.final_url.clone()),
            )
    });

    // Flush to the pane edge so the editor border fills the split — no inset gap.
    flush_panel(
        "repeater-response",
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .when_some(meta, |this, meta| {
                this.child(div().w_full().flex_none().px_1().pt_1().child(meta))
            })
            .child(
                div()
                    .w_full()
                    .flex_none()
                    .px_1()
                    .pt_1()
                    .pb_0p5()
                    .child(render_response_tabs(tab)),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .px_1()
                    .pb_1()
                    .child(code_surface(editors.repeater_response.clone(), cx)),
            ),
        cx,
    )
}

fn render_response_tabs(tab: ForgeResponseTab) -> impl IntoElement {
    Tabs::new("forge-response-tabs").child(
        TabsList::new("forge-response-tab-list")
            .child(
                TabsTrigger::new("forge-response-pretty")
                    .label("Pretty")
                    .selected(matches!(tab, ForgeResponseTab::Pretty))
                    .on_click(|_, _, cx| {
                        Bridge::set_forge_response_tab(ForgeResponseTab::Pretty, cx);
                    }),
            )
            .child(
                TabsTrigger::new("forge-response-raw")
                    .label("Raw")
                    .selected(matches!(tab, ForgeResponseTab::Raw))
                    .on_click(|_, _, cx| {
                        Bridge::set_forge_response_tab(ForgeResponseTab::Raw, cx);
                    }),
            )
            .child(
                TabsTrigger::new("forge-response-headers")
                    .label("Headers")
                    .selected(matches!(tab, ForgeResponseTab::Headers))
                    .on_click(|_, _, cx| {
                        Bridge::set_forge_response_tab(ForgeResponseTab::Headers, cx);
                    }),
            )
            .child(
                TabsTrigger::new("forge-response-request")
                    .label("Request")
                    .selected(matches!(tab, ForgeResponseTab::Request))
                    .on_click(|_, _, cx| {
                        Bridge::set_forge_response_tab(ForgeResponseTab::Request, cx);
                    }),
            ),
    )
}

/// Text to push into the response editor for the active response tab.
pub fn response_editor_text(state: &BridgeState) -> Option<String> {
    match state.forge_response_tab {
        ForgeResponseTab::Pretty => state.repeater_response.as_ref().map(format_pretty),
        ForgeResponseTab::Raw => state.repeater_response.as_ref().map(format_raw),
        ForgeResponseTab::Headers => state.repeater_response.as_ref().map(format_headers_only),
        ForgeResponseTab::Request => state
            .last_sent_request
            .as_ref()
            .map(format_sent_request)
            .or_else(|| {
                Some(
                    "// Exact wire request (post {{var}} expansion) appears here after Send."
                        .into(),
                )
            }),
    }
}

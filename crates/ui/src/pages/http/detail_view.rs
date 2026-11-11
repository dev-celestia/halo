//! Request / response detail inspector (Raw, Headers, Body, Params, Cookies).

use celestia_ui::components::CodeEditor;
use celestia_ui::components::composite::tab_bar::TabBarItem;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, DetailSide, DetailTab, HttpLogDetail};

use crate::pages::chrome::{bare_panel, content_tabs, empty_state, flush_panel};

/// The two editors behind the Raw request / response panes.
#[derive(Clone)]
pub struct DetailEditors {
    pub request: Entity<CodeEditor>,
    pub response: Entity<CodeEditor>,
}

impl DetailEditors {
    pub fn new(request: Entity<CodeEditor>, response: Entity<CodeEditor>) -> Self {
        Self { request, response }
    }
}

/// Bare inspector — tabs and content only. The dock's section bar carries the
/// close control, so the pane repeats neither a title nor an action row.
pub fn render_detail_view(
    state: &BridgeState,
    editors: &DetailEditors,
    cx: &App,
) -> impl IntoElement {
    let Some(detail) = state.selected_detail.as_ref() else {
        return bare_panel(
            "http-detail",
            empty_state(
                "Nothing selected",
                "Choose a request above to inspect headers, body, params, and cookies.",
                cx,
            ),
            cx,
        )
        .into_any_element();
    };

    let tab = state.detail_tab;
    let side = state.detail_side;

    // Full-bleed pane: the tab strip's underline must meet the dock's edges,
    // so the strip gets no inset. The panes below keep their own insets.
    flush_panel(
        "http-detail",
        div()
            .w_full()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(render_content_tabs(tab))
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .pb_2()
                    .child(render_tab_body(detail, side, tab, editors, cx)),
            ),
        cx,
    )
    .into_any_element()
}

fn render_content_tabs(tab: DetailTab) -> impl IntoElement {
    content_tabs("http-detail-tabs")
        .item(TabBarItem::new("raw", "Raw").closable(false))
        .item(TabBarItem::new("headers", "Headers").closable(false))
        .item(TabBarItem::new("body", "Body").closable(false))
        .item(TabBarItem::new("params", "Params").closable(false))
        .item(TabBarItem::new("cookies", "Cookies").closable(false))
        .active(match tab {
            DetailTab::Raw => "raw",
            DetailTab::Headers => "headers",
            DetailTab::Body => "body",
            DetailTab::Params => "params",
            DetailTab::Cookies => "cookies",
        })
        .on_select(|id, _, cx| {
            let tab = match id.as_ref() {
                "headers" => DetailTab::Headers,
                "body" => DetailTab::Body,
                "params" => DetailTab::Params,
                "cookies" => DetailTab::Cookies,
                _ => DetailTab::Raw,
            };
            Bridge::set_detail_tab(tab, cx);
        })
}

fn render_tab_body(
    detail: &HttpLogDetail,
    _side: DetailSide,
    tab: DetailTab,
    editors: &DetailEditors,
    cx: &App,
) -> impl IntoElement {
    // Raw: half-and-half request | response code editors. The editors fill
    // their pane, so each wrapper must hand down a definite height.
    if tab == DetailTab::Raw {
        return split_panes(
            pane_label(
                "Request",
                div().w_full().h_full().child(editors.request.clone()),
                cx,
            ),
            pane_label(
                "Response",
                div().w_full().h_full().child(editors.response.clone()),
                cx,
            ),
            cx,
        )
        .into_any_element();
    }

    let req_text = match tab {
        DetailTab::Raw => String::new(),
        DetailTab::Body => detail.req_body.clone(),
        DetailTab::Headers => build_headers(detail, DetailSide::Request),
        DetailTab::Params => build_params(detail),
        DetailTab::Cookies => build_cookies(detail, DetailSide::Request),
    };
    let res_text = match tab {
        DetailTab::Raw => String::new(),
        DetailTab::Body => detail.res_body.clone(),
        DetailTab::Headers => build_headers(detail, DetailSide::Response),
        DetailTab::Params => String::new(),
        DetailTab::Cookies => build_cookies(detail, DetailSide::Response),
    };

    // Params are request-only — still show empty response pane for layout consistency.
    split_panes(
        pane_label("Request", text_pane("http-detail-req", &req_text, cx), cx),
        pane_label("Response", text_pane("http-detail-res", &res_text, cx), cx),
        cx,
    )
    .into_any_element()
}

fn split_panes(
    leading: impl IntoElement,
    trailing: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let border = cx.theme().border;
    div()
        .w_full()
        .h_full()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_row()
        .gap(px(0.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .w_full()
                .h_full()
                .min_h_0()
                .overflow_hidden()
                .child(leading),
        )
        .child(div().w(px(1.)).flex_none().bg(border).self_stretch())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .w_full()
                .h_full()
                .min_h_0()
                .overflow_hidden()
                .child(trailing),
        )
}

fn pane_label(title: &'static str, body: impl IntoElement, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child(title),
        )
        .child(div().w_full().flex_1().min_h_0().child(body))
}

fn text_pane(id: &'static str, text: &str, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let display = if text.trim().is_empty() {
        "(empty)".to_string()
    } else {
        text.to_string()
    };

    div()
        .id(id)
        .w_full()
        .h_full()
        .min_h_0()
        .overflow_y_scrollbar()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(border)
        .p_3()
        .font_family("monospace")
        .text_xs()
        .text_color(if display == "(empty)" {
            muted
        } else {
            cx.theme().foreground
        })
        .child(display)
}

fn build_headers(detail: &HttpLogDetail, side: DetailSide) -> String {
    let headers = match side {
        DetailSide::Request => &detail.req_headers,
        DetailSide::Response => &detail.res_headers,
    };
    let mut pairs: Vec<_> = headers.iter().collect();
    pairs.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_params(detail: &HttpLogDetail) -> String {
    let mut lines = Vec::new();
    if let Some(q) = detail.url.split_once('?').map(|(_, q)| q) {
        lines.push("── Query ──".to_string());
        for part in q.split('&') {
            if part.is_empty() {
                continue;
            }
            if let Some((k, v)) = part.split_once('=') {
                lines.push(format!("{k} = {v}"));
            } else {
                lines.push(part.to_string());
            }
        }
    }
    let ct = detail
        .req_headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    if ct.contains("application/x-www-form-urlencoded") && !detail.req_body.is_empty() {
        lines.push("── Form body ──".to_string());
        for part in detail.req_body.split('&') {
            if part.is_empty() {
                continue;
            }
            if let Some((k, v)) = part.split_once('=') {
                lines.push(format!("{k} = {v}"));
            } else {
                lines.push(part.to_string());
            }
        }
    }
    lines.join("\n")
}

fn build_cookies(detail: &HttpLogDetail, side: DetailSide) -> String {
    match side {
        DetailSide::Request => detail
            .req_headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("cookie"))
            .map(|(_, v)| {
                v.split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        DetailSide::Response => detail
            .res_headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

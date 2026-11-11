//! Network tab — unified proxy / forge / intruder traffic.

use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState};

use crate::network::{self, NetworkDetail, NetworkEntry, NetworkSource};
use crate::state::DebuggerState;

pub fn render(
    state: &DebuggerState,
    bridge: &BridgeState,
    on_select: impl Fn(Option<String>, &mut App) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let entries = network::collect_network(bridge);
    let selected_id = state.selected_network_id.clone();
    let selected = selected_id
        .as_ref()
        .and_then(|id| entries.iter().find(|e| &e.id == id).cloned());

    div()
        .id("debugger-network")
        .size_full()
        .flex()
        .min_h_0()
        .child(
            div()
                .id("debugger-network-list")
                .w(px(340.))
                .flex_none()
                .h_full()
                .min_h_0()
                .border_r_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("{} requests", entries.len())),
                        )
                        .child(
                            div()
                                .id("debugger-network-refresh")
                                .text_xs()
                                .text_color(theme.primary)
                                .cursor_pointer()
                                .child("Refresh")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::refresh_history(cx);
                                    let _ = Bridge::refresh_storage(cx);
                                }),
                        ),
                )
                .child(
                    div()
                        .id("debugger-network-list-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .w_full()
                                .children(entries.into_iter().map(|entry| {
                                    let selected =
                                        selected_id.as_deref() == Some(entry.id.as_str());
                                    let on_select = on_select.clone();
                                    let id = entry.id.clone();
                                    entry_row(
                                        entry,
                                        selected,
                                        move |cx| on_select(Some(id.clone()), cx),
                                        cx,
                                    )
                                })),
                        ),
                ),
        )
        .child(
            div()
                .id("debugger-network-detail")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_y_scrollbar()
                .px_3()
                .py_2()
                .child(match selected {
                    Some(entry) => detail(&entry, cx).into_any_element(),
                    None => empty_hint(
                        "Select a request to inspect method, URL, status, and sizes.",
                        cx,
                    )
                    .into_any_element(),
                }),
        )
}

fn entry_row(
    entry: NetworkEntry,
    selected: bool,
    on_click: impl Fn(&mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let status = entry
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into());

    div()
        .w_full()
        .flex_shrink_0()
        .px_1()
        .child(
            div()
                .id(SharedString::from(format!("dbg-net-{}", entry.id)))
                .w_full()
                .px_2()
                .py_1()
                .rounded(px(6.))
                .cursor_pointer()
                .when(selected, |this| {
                    this.bg(theme.accent.opacity(0.22))
                        .border_1()
                        .border_color(theme.accent.opacity(0.45))
                })
                .when(!selected, |this| {
                    this.border_1().border_color(gpui::transparent_black())
                })
                .hover(|this| {
                    if selected {
                        this
                    } else {
                        this.bg(theme.accent.opacity(0.10))
                    }
                })
                .on_click(move |_, _, cx| on_click(cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(source_badge(entry.source, cx))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.foreground)
                                .child(entry.method.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(status),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(truncate_url(&entry.url, 64)),
                ),
        )
        .into_any_element()
}

fn empty_hint(message: &'static str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .child(
            div()
                .max_w(px(280.))
                .text_xs()
                .text_color(theme.muted_foreground)
                .text_center()
                .child(message),
        )
}

fn source_badge(source: NetworkSource, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    // Inner chip radius ≈ row radius (6) − horizontal pad (8) → keep ≥ 2px.
    div()
        .px_1()
        .rounded(px(3.))
        .bg(theme.accent.opacity(0.2))
        .text_xs()
        .text_color(theme.primary)
        .child(source.label())
}

fn detail(entry: &NetworkEntry, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let mut lines: Vec<(String, String)> = vec![
        ("source".into(), entry.source.label().into()),
        ("method".into(), entry.method.clone()),
        ("url".into(), entry.url.clone()),
        (
            "status".into(),
            entry
                .status
                .map(|s| format!("{s} {}", entry.status_text.clone().unwrap_or_default()))
                .unwrap_or_else(|| "—".into()),
        ),
        ("timestamp".into(), entry.timestamp.clone()),
    ];
    if let Some(ms) = entry.duration_ms {
        lines.push(("duration_ms".into(), ms.to_string()));
    }
    if let Some(n) = entry.request_size {
        lines.push(("request_size".into(), format!("{n} B")));
    }
    if let Some(n) = entry.response_size {
        lines.push(("response_size".into(), format!("{n} B")));
    }

    match &entry.detail {
        NetworkDetail::Proxy(row) => {
            lines.push(("id".into(), row.id.clone()));
            lines.push(("host".into(), row.host.clone().unwrap_or_default()));
            lines.push(("session".into(), row.session_id.clone()));
        }
        NetworkDetail::Forge(row) => {
            lines.push(("id".into(), row.id.clone()));
            if let Some(body) = &row.response_body {
                let preview = if body.len() > 400 {
                    format!("{}…", &body[..400])
                } else {
                    body.clone()
                };
                lines.push(("response_body".into(), preview));
            }
        }
        NetworkDetail::Intruder(row) => {
            lines.push(("id".into(), row.id.clone()));
            lines.push(("payload".into(), row.payload_summary()));
            if let Some(err) = &row.error {
                lines.push(("error".into(), err.clone()));
            }
        }
    }

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .child(format!("{} {}", entry.method, truncate_url(&entry.url, 80))),
        )
        .children(lines.into_iter().map(|(k, v)| {
            let primary = theme.primary;
            let foreground = theme.foreground;
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .w(px(120.))
                        .text_xs()
                        .font_family("monospace")
                        .text_color(primary)
                        .child(k),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .font_family("monospace")
                        .text_color(foreground)
                        .child(v),
                )
        }))
}

fn truncate_url(url: &str, max: usize) -> String {
    if url.chars().count() <= max {
        url.to_string()
    } else {
        let truncated: String = url.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

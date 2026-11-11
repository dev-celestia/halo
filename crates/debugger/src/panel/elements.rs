//! Elements tab — registered regions + selected props / theme tokens.

use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::BridgeState;

use crate::element::{self, DebugRegion};
use crate::state::DebuggerState;

pub fn render(
    state: &DebuggerState,
    bridge: &BridgeState,
    on_select_region: impl Fn(String, &mut App) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let regions = element::region_list();
    let selected_id = element::selected_region().map(|r| r.id);
    let selected = selected_id
        .as_ref()
        .and_then(|id| regions.iter().find(|r| &r.id == id).cloned());

    div()
        .id("debugger-elements")
        .size_full()
        .flex()
        .min_h_0()
        .child(
            div()
                .id("debugger-elements-list")
                .w(px(260.))
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
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!(
                            "{} region{}",
                            regions.len(),
                            if regions.len() == 1 { "" } else { "s" }
                        )),
                )
                .child(
                    div()
                        .id("debugger-elements-list-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .w_full()
                                .children(regions.into_iter().map(|region| {
                                    let selected = selected_id.as_deref() == Some(region.id.as_str());
                                    let on_select = on_select_region.clone();
                                    let id = region.id.clone();
                                    region_row(region, selected, move |cx| on_select(id.clone(), cx), cx)
                                })),
                        ),
                ),
        )
        .child(
            div()
                .id("debugger-elements-detail")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_y_scrollbar()
                .px_3()
                .py_2()
                .child(match selected {
                    Some(region) => detail_panel(&region, bridge, state, cx).into_any_element(),
                    None => empty_hint(
                        if state.inspect_mode {
                            "Inspect mode on — click a registered region, or pick one from the list."
                        } else {
                            "Select a region, or turn on Inspect to click one in the UI."
                        },
                        cx,
                    )
                    .into_any_element(),
                }),
        )
}

fn region_row(
    region: DebugRegion,
    selected: bool,
    on_click: impl Fn(&mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let bounds_hint = region
        .bounds
        .map(|b| format!("{:.0}×{:.0}", b.width, b.height))
        .unwrap_or_else(|| "—".into());

    // Inset + radius so the selected chip sits concentrically in the list
    // (list has no pad; row uses mx_1 / rounded-md ≈ 6px).
    // flex_shrink_0 so rows overflow the scrollport instead of compressing.
    div()
        .w_full()
        .flex_shrink_0()
        .px_1()
        .child(
            div()
                .id(SharedString::from(format!("dbg-region-{}", region.id)))
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
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(region.label.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("{} · {}", region.id, bounds_hint)),
                ),
        )
        .into_any_element()
}

fn detail_panel(
    region: &DebugRegion,
    bridge: &BridgeState,
    state: &DebuggerState,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let mut rows: Vec<(String, String)> = vec![
        ("id".into(), region.id.clone()),
        ("label".into(), region.label.clone()),
    ];

    if let Some(b) = region.bounds {
        rows.push((
            "bounds".into(),
            format!(
                "x={:.1} y={:.1} w={:.1} h={:.1}",
                b.x, b.y, b.width, b.height
            ),
        ));
        rows.push(("size".into(), format!("{:.0} × {:.0}", b.width, b.height)));
    } else {
        rows.push(("bounds".into(), "(not measured)".into()));
    }

    if let Some(p) = &region.meta.padding {
        rows.push(("padding".into(), p.clone()));
    }
    if let Some(g) = &region.meta.gap {
        rows.push(("gap".into(), g.clone()));
    }
    if let Some(n) = &region.meta.notes {
        rows.push(("notes".into(), n.clone()));
    }

    rows.push(("inspect_mode".into(), state.inspect_mode.to_string()));
    rows.push((
        "proxy".into(),
        if bridge.proxy.running {
            format!(
                "running :{} / TLS {}",
                bridge.proxy.port.unwrap_or(0),
                bridge
                    .proxy
                    .tls_port
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "—".into())
            )
        } else {
            "stopped".into()
        },
    ));
    rows.push(("history_rows".into(), bridge.history.len().to_string()));
    rows.push(("theme.radius".into(), format!("{:?}", theme.radius)));
    rows.push(("theme.background".into(), format!("{:?}", theme.background)));
    rows.push(("theme.foreground".into(), format!("{:?}", theme.foreground)));

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .child(region.label.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(rows.into_iter().map(|(k, v)| prop_row(k, v, cx))),
        )
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

fn prop_row(key: String, value: String, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .gap_2()
        .items_start()
        .child(
            div()
                .w(px(120.))
                .flex_none()
                .text_xs()
                .font_family("monospace")
                .text_color(theme.primary)
                .child(key),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .font_family("monospace")
                .text_color(theme.foreground)
                .child(value),
        )
        .into_any_element()
}

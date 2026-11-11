use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, PortScanResultDto};

use crate::components::confirm_destructive;
use crate::pages::chrome::empty_state;
use crate::pages::colors::status_color_u16;

pub fn render_results(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let ps = &state.port_scanner;
    let open = ps.open_results();
    let open_count = open.len();

    div()
        .w_full()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!(
                                    "Scanned {}",
                                    if ps.target.is_empty() {
                                        "—"
                                    } else {
                                        &ps.target
                                    }
                                )),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(theme.radius_sm())
                                .border_1()
                                .border_color(theme.border)
                                .text_xs()
                                .child(format!("{open_count} open")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .children(
                            (open_count > 0)
                                .then(|| copy_open_ports_button(&open).into_any_element()),
                        )
                        .child(
                            Button::icon("ps-clear-btn", PhosphorIcon::Trash)
                                .variant(ButtonVariant::Destructive)
                                .size(ButtonSize::Small)
                                .accessibility_label("Clear scan results")
                                .tooltip("Clear scan results")
                                .disabled(open_count == 0)
                                .on_click(move |_, window, cx| {
                                    confirm_destructive(
                                        window,
                                        cx,
                                        "Clear scan results?",
                                        format!(
                                            "{open_count} scan result(s) will be removed. This cannot be undone."
                                        ),
                                        "Clear",
                                        |_, cx| {
                                            let _ = Bridge::port_scan_clear(cx);
                                        },
                                    );
                                }),
                        ),
                ),
        )
        .child(
            div()
                .id("ps-results")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap_0p5()
                .children(if open.is_empty() {
                    vec![
                        empty_state(
                            if ps.is_running {
                                "Scanning…"
                            } else {
                                "No open ports discovered"
                            },
                            if ps.is_running {
                                "Results stream in as probes complete."
                            } else {
                                "Adjust the target or ports and start a new scan."
                            },
                            cx,
                        )
                        .into_any_element(),
                    ]
                } else {
                    vec![
                        results_header(cx).into_any_element(),
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .children(
                                open.iter()
                                    .map(|row| result_row(row, cx))
                                    .collect::<Vec<_>>(),
                            )
                            .into_any_element(),
                    ]
                }),
        )
        .into_any_element()
}

pub fn results_header(cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(theme.muted_foreground)
        .bg(theme.muted.opacity(0.2))
        .rounded(theme.radius_sm())
        .child(div().w(px(70.)).child("PORT"))
        .child(div().w(px(80.)).child("STATE"))
        .child(div().w(px(100.)).child("SERVICE"))
        .child(div().w(px(110.)).child("LATENCY"))
        .child(div().flex_1().child("BANNER"))
        .into_any_element()
}

pub fn result_row(row: &PortScanResultDto, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let ms = row.response_time_ms.unwrap_or(0);
    let latency_color = if ms > 800 {
        hsla(0., 0.75, 0.5, 1.)
    } else if ms > 300 {
        hsla(45. / 360., 0.9, 0.5, 1.)
    } else {
        hsla(150. / 360., 0.65, 0.4, 1.)
    };

    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border.opacity(0.6))
        .hover(|s| s.bg(theme.muted.opacity(0.2)))
        .child(
            div()
                .w(px(70.))
                .text_xs()
                .font_family("Menlo")
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.primary)
                .child(row.port.to_string()),
        )
        .child(
            div()
                .w(px(80.))
                .text_xs()
                .text_color(status_color_u16(200))
                .child(row.state.clone()),
        )
        .child(
            div()
                .w(px(100.))
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(row.service.clone()),
        )
        .child(
            div()
                .w(px(110.))
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .w(px(40.))
                        .text_xs()
                        .font_family("Menlo")
                        .text_color(theme.muted_foreground)
                        .child(if ms > 0 {
                            format!("{ms}ms")
                        } else {
                            "—".into()
                        }),
                )
                .child(
                    div()
                        .w(px(48.))
                        .h(px(4.))
                        .rounded_full()
                        .bg(theme.muted)
                        .overflow_hidden()
                        .child(
                            div()
                                .h_full()
                                .rounded_full()
                                .bg(latency_color)
                                .w(relative((ms as f32 / 1000.0).min(1.0))),
                        ),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .font_family("Menlo")
                .truncate()
                .text_color(theme.foreground.opacity(0.85))
                .child(row.banner.clone().unwrap_or_else(|| "—".into())),
        )
        .into_any_element()
}

pub fn copy_open_ports_button(open: &[&PortScanResultDto]) -> Button {
    let text: String = open
        .iter()
        .map(|r| format!("{}:{}", r.host, r.port))
        .collect::<Vec<_>>()
        .join("\n");
    Button::new("ps-copy-btn")
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::Copy)
        .label("Copy")
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        })
}

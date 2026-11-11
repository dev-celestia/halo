use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, PortScannerView};

use crate::pages::chrome::{StatusTone, status_pill};
use crate::pages::editors::ToolEditors;

pub fn render_toolbar(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let ps = &state.port_scanner;
    let running = ps.is_running;

    let port_count = halo_bridge::parse_ports(&ps.ports_text).len();
    let can_scan = !ps.target.trim().is_empty() && port_count > 0;

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(div().w(px(200.)).child(editors.ps_target.clone()))
                .child(if running {
                    Button::new("ps-stop-btn")
                        .variant(ButtonVariant::Destructive)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Stop)
                        .label("Stop Scan")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::port_scan_stop(cx);
                        })
                        .into_any_element()
                } else {
                    Button::new("ps-start-btn")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Play)
                        .label("Start Scan")
                        .disabled(!can_scan)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::port_scan_start(None, cx);
                        })
                        .into_any_element()
                })
                .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
                .children(preset_chips(ps, cx))
                .child(if ps.preset == "custom" {
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(div().w(px(200.)).child(editors.ps_ports.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("{port_count}p")),
                        )
                        .into_any_element()
                } else {
                    div().into_any_element()
                })
                .child(if running && ps.progress_total > 0 {
                    status_pill(
                        format!(
                            "{}/{} ({}%)",
                            ps.progress_current,
                            ps.progress_total,
                            ps.progress_pct()
                        ),
                        StatusTone::Busy,
                        cx,
                    )
                    .into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
        .child(render_options_row(state, editors, cx))
        .children(ps.error.clone().map(|error| {
            div()
                .w_full()
                .px_2()
                .py_1p5()
                .rounded(theme.radius_sm())
                .bg(theme.danger.opacity(0.08))
                .text_color(theme.danger)
                .text_xs()
                .font_family("Menlo")
                .child(format!("Error: {error}"))
        }))
        .child(
            // 2px bottom progress line.
            div()
                .w_full()
                .h(px(2.))
                .rounded_full()
                .bg(theme.border)
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .bg(theme.primary)
                        .w(relative(ps.progress_pct() as f32 / 100.0)),
                ),
        )
}

pub fn preset_chips(ps: &PortScannerView, _cx: &App) -> Vec<AnyElement> {
    halo_bridge::PORT_PRESET_COUNTS
        .iter()
        .map(|(value, label)| {
            let selected = ps.preset == *value;
            Button::new(SharedString::from(format!("ps-preset-{value}")))
                .variant(if selected {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .size(ButtonSize::Small)
                .label(*label)
                .on_click({
                    let value = value.to_string();
                    move |_, _, cx| {
                        Bridge::port_scan_set_preset(&value, cx);
                    }
                })
                .into_any_element()
        })
        .collect()
}

pub fn render_options_row(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let ps = &state.port_scanner;

    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.15))
        // Stealth toggle: amber when stealth, red when noisy.
        .child(if ps.stealth_mode {
            Button::new("ps-stealth-toggle")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::EyeSlash)
                .label("Stealth")
                .on_click(|_, _, cx| Bridge::port_scan_set_stealth(false, cx))
                .into_any_element()
        } else {
            Button::new("ps-stealth-toggle")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Warning)
                .label("Noisy - High Performance")
                .on_click(|_, _, cx| Bridge::port_scan_set_stealth(true, cx))
                .into_any_element()
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Timeout ms:"),
                )
                .child(div().w(px(64.)).child(editors.ps_timeout.clone())),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Concurrency:"),
                )
                .child(div().w(px(64.)).child(editors.ps_concurrency.clone())),
        )
        .child(
            Button::new("ps-banner-toggle")
                .variant(if ps.banner_grab {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Eye)
                .label("Grab Banners")
                .on_click(|_, _, cx| {
                    let next = !Bridge::state(cx).read(cx).port_scanner.banner_grab;
                    Bridge::port_scan_set_banner_grab(next, cx);
                }),
        )
        .children(ps.stealth_mode.then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Delay ms:"),
                        )
                        .child(div().w(px(64.)).child(editors.ps_delay.clone())),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Jitter ms:"),
                        )
                        .child(div().w(px(64.)).child(editors.ps_jitter.clone())),
                )
                .child(
                    Button::new("ps-randomize-toggle")
                        .variant(if ps.randomize_ports {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        })
                        .size(ButtonSize::Small)
                        .label("Randomize Ports")
                        .on_click(|_, _, cx| {
                            let next = !Bridge::state(cx).read(cx).port_scanner.randomize_ports;
                            Bridge::port_scan_set_randomize(next, cx);
                        }),
                )
                .into_any_element()
        }))
}

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::editors::ToolEditors;

pub fn render_welcome(_state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let ps_target = editors.ps_target.clone();
    let quick_start = |preset: &'static str, title: &'static str, hint: &'static str| {
        div()
            .id(SharedString::from(format!("ps-welcome-{preset}")))
            .flex_1()
            .flex()
            .flex_col()
            .items_start()
            .p_3()
            .rounded(theme.radius_sm())
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .cursor_pointer()
            .hover(|s| s.border_color(theme.primary.opacity(0.5)).bg(theme.accent))
            .active(|s| s.opacity(0.9))
            .on_click({
                let ps_target = ps_target.clone();
                move |_, _, cx| {
                    // Welcome quick start: default target + preset ports.
                    let mut target = ps_target.read(cx).value().to_string();
                    if target.trim().is_empty() {
                        target = "127.0.0.1".into();
                    }
                    Bridge::port_scan_set_target(&target, cx);
                    Bridge::port_scan_set_preset(preset, cx);
                    let _ = Bridge::port_scan_start(None, cx);
                }
            })
            .child(div().text_xs().font_weight(FontWeight::NORMAL).child(title))
            .child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(hint),
            )
    };

    div()
        .id("ps-welcome")
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .child(
            div()
                .p_4()
                .rounded_full()
                .bg(theme.primary.opacity(0.1))
                .text_color(theme.primary)
                .child(
                    div()
                        .size(px(40.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(PhosphorIcon::ShieldCheck),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::NORMAL)
                        .child("Fast, Local Port Scanner"),
                )
                .child(
                    div()
                        .max_w(px(520.))
                        .text_center()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(
                            "Probe TCP services with configurable presets, stealth rate-limiting, and live banner grabbing. Results stream in as ports answer.",
                        ),
                ),
        )
        .child(
            div()
                .w_full()
                .max_w(px(640.))
                .flex()
                .items_center()
                .gap_2()
                .p_2()
                .rounded(theme.radius_sm())
                .border_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.2))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(editors.ps_target.clone()),
                )
                .child(
                    Button::new("ps-welcome-start-btn")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Play)
                        .label("Scan 127.0.0.1")
                        .on_click({
                            let ps_target = editors.ps_target.clone();
                            move |_, _, cx| {
                                let mut target = ps_target.read(cx).value().to_string();
                                if target.trim().is_empty() {
                                    target = "127.0.0.1".into();
                                }
                                Bridge::port_scan_set_target(&target, cx);
                                let _ = Bridge::port_scan_start(None, cx);
                            }
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .max_w(px(640.))
                .flex()
                .items_center()
                .gap_2()
                .child(quick_start(
                    "quick",
                    "Quick Scan",
                    "Top 20 most common service ports",
                ))
                .child(quick_start(
                    "web",
                    "Web Ports",
                    "HTTP, HTTPS, proxies & dev servers",
                ))
                .child(quick_start(
                    "top100",
                    "Top 100",
                    "Broad sweep across core infra ports",
                )),
        )
}

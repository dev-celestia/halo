//! General settings — proxy listener ports, storage cleanup, factory reset.

use celestia_ui::components::HorizontalAlignment;
use celestia_ui::components::VStack;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, StorageInfo};

use crate::components::confirm_destructive;
use crate::pages::chrome::{StatusTone, format_bytes, section, status_pill};
use crate::pages::editors::ToolEditors;

use super::proxy_controls::render_proxy_controls;

pub fn render_general_tab(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .child(render_proxy_controls(
            &state.proxy,
            state.proxy_busy,
            state.last_message.as_deref(),
            state.last_error.as_deref(),
            cx,
        ))
        .child(render_listener_ports(
            state.preferred_http_port,
            state.preferred_tls_port,
            state.proxy.running,
            editors,
            cx,
        ))
        .child(render_storage_section(
            state.storage.as_ref(),
            state.storage_busy,
            cx,
        ))
}

fn render_listener_ports(
    http: u16,
    tls: u16,
    running: bool,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;

    section(
        "Proxy Listener",
        Some(
            format!(
                "Configured: HTTP {http} / TLS {tls}. Save applies the next time you start the proxy."
            )
            .into(),
        ),
        VStack::new(HorizontalAlignment::Leading)
            .spacing(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(100.))
                            .child(Input::new(&editors.settings_http_port)),
                    )
                    .child(
                        div()
                            .w(px(100.))
                            .child(Input::new(&editors.settings_tls_port)),
                    )
                    .child(
                        Button::new("settings-port-save")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::FloppyDisk)
                            .label("Save")
                            .on_click({
                                let http_ed = editors.settings_http_port.clone();
                                let tls_ed = editors.settings_tls_port.clone();
                                move |_, _, cx| {
                                    let http_s = http_ed.read(cx).value().to_string();
                                    let tls_s = tls_ed.read(cx).value().to_string();
                                    let http = http_s.trim().parse::<u16>().unwrap_or(0);
                                    let tls = tls_s.trim().parse::<u16>().unwrap_or(0);
                                    let _ = Bridge::set_preferred_ports(http, tls, cx);
                                }
                            }),
                    )
                    .child(
                        Button::new("settings-port-reset")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::ArrowCounterClockwise)
                            .label("Reset")
                            .on_click({
                                let http_ed = editors.settings_http_port.clone();
                                let tls_ed = editors.settings_tls_port.clone();
                                move |_, window, cx| {
                                    Bridge::reset_preferred_ports(cx);
                                    http_ed.update(cx, |s, cx| {
                                        s.set_value("8888", window, cx);
                                    });
                                    tls_ed.update(cx, |s, cx| {
                                        s.set_value("8443", window, cx);
                                    });
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(if running {
                        "Proxy is running — stop and start again to apply new ports."
                    } else {
                        "Ports must be between 1 and 65535."
                    }),
            ),
        cx,
    )
}

fn render_storage_section(storage: Option<&StorageInfo>, busy: bool, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;

    let mut body = VStack::new(HorizontalAlignment::Leading).spacing(px(10.));

    if let Some(info) = storage {
        body = body
            .child(storage_row(
                "SQL Database & Payloads",
                format_bytes(info.database_size_bytes),
                "database",
                "Deletes data.db, WAL/SHM, and payload segments, then recreates an empty schema.",
                busy,
                cx,
            ))
            .child(storage_row(
                "Log File",
                format_bytes(info.log_file_size_bytes),
                "log_file",
                "Truncates logs/halo.log in the app data folder without breaking open file descriptors.",
                busy,
                cx,
            ))
            .child(storage_row(
                "Crawler Artifacts",
                format_bytes(info.crawler_artifacts_size_bytes),
                "crawler_artifacts",
                "Removes screenshots and DOM snapshots under crawler-artifacts.",
                busy,
                cx,
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("App data: {}", info.app_data_dir)),
            );
    } else {
        body = body.child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Loading storage info…"),
        );
    }

    body = body.child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .pt_2()
            .child(
                Button::new("settings-refresh-storage")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::ArrowClockwise)
                    .label("Refresh")
                    .disabled(busy)
                    .on_click(|_, _, cx| {
                        let _ = Bridge::refresh_storage(cx);
                    }),
            )
            .child(
                Button::icon("settings-reset-all", PhosphorIcon::Trash)
                    .variant(ButtonVariant::Destructive)
                    .size(ButtonSize::Small)
                    .accessibility_label("Delete all data")
                    .tooltip("Delete all data")
                    .disabled(busy)
                    .on_click(|_, window, cx| {
                        confirm_destructive(
                            window,
                            cx,
                            "Delete all application data?",
                            "This stops the proxy and permanently deletes the database, sessions, CA keys, crawler artifacts, and settings files. The app reinitializes with a fresh state. This cannot be undone.",
                            "Delete everything",
                            |_, cx| {
                                let _ = Bridge::reset_all_app_data(cx);
                            },
                        );
                    }),
            ),
    );

    section(
        "Storage",
        Some("Observe and clear on-disk Halo data.".into()),
        body,
        cx,
    )
}

fn storage_row(
    label: &str,
    size: String,
    artifact: &'static str,
    description: &str,
    busy: bool,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let title = format!("Clear {label}?");
    let body = format!("{description} This cannot be undone.");
    let action_label = format!("Clear {label}");
    let tooltip = format!("Clear {label}");

    div()
        .w_full()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::NORMAL)
                                .child(label.to_string()),
                        )
                        .child(status_pill(size, StatusTone::Idle, cx)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(description.to_string()),
                ),
        )
        .child(
            Button::icon(
                SharedString::from(format!("clear-{artifact}")),
                PhosphorIcon::Trash,
            )
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Small)
            .accessibility_label(tooltip.clone())
            .tooltip(tooltip)
            .disabled(busy)
            .on_click(move |_, window, cx| {
                confirm_destructive(
                    window,
                    cx,
                    title.clone(),
                    body.clone(),
                    action_label.clone(),
                    move |_, cx| {
                        let _ = Bridge::delete_storage_artifact(artifact, cx);
                    },
                );
            }),
        )
}

//! Logs tab — in-memory ring + contexts + chronicle + optional file tail.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState};

use halo_bridge::DebugLogLevel;

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::confirm::confirm_destructive;
use crate::state::DebuggerState;
use crate::storage::tail_log_file;

fn cached_tail_log_file(app_data_dir: &str, max_bytes: u64) -> Option<String> {
    static CACHE: OnceLock<Mutex<Option<(Instant, String, Option<String>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let Ok(mut guard) = cache.lock() else {
        return None;
    };
    let now = Instant::now();
    if let Some((ts, cached_dir, cached_tail)) = guard.as_ref() {
        if cached_dir == app_data_dir && now.duration_since(*ts) < Duration::from_secs(2) {
            return cached_tail.clone();
        }
    }
    let tail = tail_log_file(app_data_dir, max_bytes).ok();
    *guard = Some((now, app_data_dir.to_string(), tail.clone()));
    tail
}

pub fn render(_state: &DebuggerState, bridge: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let logs = &bridge.debug_logs;
    let file_tail = bridge
        .storage
        .as_ref()
        .and_then(|s| cached_tail_log_file(&s.app_data_dir, 8_192));

    div()
        .id("debugger-logs")
        .size_full()
        .flex()
        .min_h_0()
        .child(
            div()
                .id("debugger-logs-ring")
                .flex_1()
                .min_w_0()
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
                                .child(format!("{} log entries", logs.len())),
                        )
                        .child(
                            Button::icon("dbg-logs-clear", PhosphorIcon::Trash)
                                .variant(ButtonVariant::Destructive)
                                .size(ButtonSize::Small)
                                .accessibility_label("Clear debug logs")
                                .tooltip("Clear")
                                .disabled(logs.is_empty())
                                .on_click({
                                    let count = logs.len();
                                    move |_, window, cx| {
                                        confirm_destructive(
                                            window,
                                            cx,
                                            "Clear debug logs?",
                                            format!(
                                                "{count} log entries will be removed from this session. This cannot be undone."
                                            ),
                                            "Clear",
                                            |_, cx| {
                                                Bridge::clear_debug_logs(cx);
                                            },
                                        );
                                    }
                                }),
                        ),
                )
                .child(
                    div()
                        .id("debugger-logs-ring-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .px_2()
                        .py_1()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .children(logs.iter().rev().map(|entry| {
                                    let color = match entry.level {
                                        DebugLogLevel::Error => theme.danger,
                                        DebugLogLevel::Warn => theme.warning,
                                        DebugLogLevel::Debug => theme.muted_foreground,
                                        DebugLogLevel::Info => theme.foreground,
                                    };
                                    div()
                                        .flex_shrink_0()
                                        .flex()
                                        .gap_2()
                                        .items_start()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family("monospace")
                                                .text_color(theme.muted_foreground)
                                                .child(entry.timestamp.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family("monospace")
                                                .text_color(theme.primary)
                                                .child(entry.level.as_str().to_string()),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_family("monospace")
                                                .text_color(theme.muted_foreground)
                                                .child(entry.source.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_xs()
                                                .font_family("monospace")
                                                .text_color(color)
                                                .child(entry.message.clone()),
                                        )
                                })),
                        ),
                ),
        )
        .child(
            div()
                .id("debugger-logs-side")
                .w(px(280.))
                .flex_none()
                .h_full()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child("Contexts"),
                )
                .child(
                    div()
                        .id("debugger-logs-contexts-scroll")
                        .flex_none()
                        .max_h(relative(0.35))
                        .overflow_y_scrollbar()
                        .px_2()
                        .child(div().flex().flex_col().gap_1().children(
                            bridge.contexts.iter().map(|ctx_dto| {
                                let active = bridge.active_context_id.as_deref()
                                    == Some(ctx_dto.id.as_str());
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .when(active, |this| {
                                        this.font_weight(FontWeight::NORMAL)
                                            .text_color(theme.primary)
                                    })
                                    .when(!active, |this| this.text_color(theme.foreground))
                                    .child(format!(
                                        "{} ({})",
                                        ctx_dto.name,
                                        if active {
                                            "active"
                                        } else {
                                            &ctx_dto.id[..8.min(ctx_dto.id.len())]
                                        }
                                    ))
                            }),
                        )),
                )
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .border_t_1()
                        .border_color(theme.border)
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child("Chronicle (recent)"),
                )
                .child(
                    div()
                        .id("debugger-logs-chronicle-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .px_2()
                        .child(div().flex().flex_col().gap_1().children(
                            bridge.chronicle.iter().take(30).map(|row| {
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!(
                                        "{} {} → {}",
                                        row.method,
                                        truncate(&row.url, 40),
                                        row.response_status
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| "—".into())
                                    ))
                            }),
                        )),
                )
                .when_some(file_tail, |this, tail| {
                    this.child(
                        div()
                            .flex_none()
                            .max_h(relative(0.3))
                            .border_t_1()
                            .border_color(theme.border)
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .child("halo.log (tail)"),
                            )
                            .child(
                                div()
                                    .id("debugger-logs-file-scroll")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .px_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family("monospace")
                                            .text_color(theme.muted_foreground)
                                            .child(tail),
                                    ),
                            ),
                    )
                }),
        )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

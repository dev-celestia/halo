//! App event log flyout — opened from the dock notification bell.
//!
//! Surfaces [`BridgeState::debug_logs`], which already records every
//! `apply_ok_msg` / `apply_err` and other bridge events.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, DebugLogLevel};

use crate::app::HaloApp;
use crate::components::confirm_destructive;

const PANEL_W: f32 = 420.;
const PANEL_H: f32 = 360.;

pub(crate) fn render_notifications(
    state: &BridgeState,
    weak: &WeakEntity<HaloApp>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let logs = &state.debug_logs;
    let weak_scrim = weak.clone();

    div()
        .id("notifications-overlay")
        .absolute()
        .inset_0()
        .child(
            div()
                .id("notifications-scrim")
                .absolute()
                .inset_0()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    if let Some(app) = weak_scrim.upgrade() {
                        app.update(cx, |this, cx| this.close_notifications(cx));
                    }
                }),
        )
        .child(
            div()
                .id("notifications-panel")
                .absolute()
                .bottom(px(48.))
                .right(px(8.))
                .w(px(PANEL_W))
                .max_w(relative(0.94))
                .h(px(PANEL_H))
                .max_h(relative(0.7))
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .shadow(vec![BoxShadow {
                    offset: point(px(0.), px(12.)),
                    blur_radius: px(32.),
                    spread_radius: px(0.),
                    inset: false,
                    color: rgba(0x00000055).into(),
                }])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(header(logs.len(), weak, cx))
                .child(if logs.is_empty() {
                    empty_state(cx).into_any_element()
                } else {
                    log_list(logs, cx).into_any_element()
                }),
        )
}

fn header(count: usize, weak: &WeakEntity<HaloApp>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let weak_clear = weak.clone();
    let weak_close = weak.clone();

    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(
                    Phosphor::new(PhosphorIcon::Bell)
                        .size(px(14.))
                        .color(theme.primary),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .child("Notifications"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("{count}")),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::icon("notifications-clear", PhosphorIcon::Trash)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .accessibility_label("Clear notifications")
                        .tooltip("Clear notifications")
                        .disabled(count == 0)
                        .on_click({
                            let weak_clear = weak_clear.clone();
                            move |_, window, cx| {
                                let weak_clear = weak_clear.clone();
                                confirm_destructive(
                                    window,
                                    cx,
                                    "Clear notifications?",
                                    format!(
                                        "{count} event(s) will be removed from the log. This cannot be undone."
                                    ),
                                    "Clear",
                                    move |_, cx| {
                                        Bridge::clear_debug_logs(cx);
                                        if let Some(app) = weak_clear.upgrade() {
                                            app.update(cx, |this, cx| {
                                                this.mark_notifications_seen(cx);
                                            });
                                        }
                                    },
                                );
                            }
                        }),
                )
                .child(
                    Button::icon(
                        "notifications-close",
                        Phosphor::new(PhosphorIcon::X).size(px(14.)),
                    )
                    .size(ButtonSize::IconSm)
                    .variant(ButtonVariant::Ghost)
                    .accessibility_label("Close notifications")
                    .tooltip("Close")
                    .on_click(move |_, _, cx| {
                        if let Some(app) = weak_close.upgrade() {
                            app.update(cx, |this, cx| this.close_notifications(cx));
                        }
                    }),
                ),
        )
}

fn empty_state(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .px_4()
        .child(
            Phosphor::new(PhosphorIcon::BellSlash)
                .size(px(28.))
                .color(theme.muted_foreground),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("No events yet — app actions will show up here."),
        )
}

fn log_list(logs: &[halo_bridge::DebugLogEntry], cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .id("notifications-list")
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .overflow_y_scrollbar()
        .px_2()
        .py_1()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .children(logs.iter().rev().map(|entry| {
                    let level_color = match entry.level {
                        DebugLogLevel::Error => theme.danger,
                        DebugLogLevel::Warn => theme.warning,
                        DebugLogLevel::Debug => theme.muted_foreground,
                        DebugLogLevel::Info => theme.primary,
                    };
                    let msg_color = match entry.level {
                        DebugLogLevel::Error => theme.danger,
                        DebugLogLevel::Warn => theme.warning,
                        DebugLogLevel::Debug => theme.muted_foreground,
                        DebugLogLevel::Info => theme.foreground,
                    };

                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .px_2()
                        .py_1p5()
                        .rounded(theme.radius_sm())
                        .hover(|s| s.bg(theme.accent.opacity(0.08)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
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
                                        .font_weight(FontWeight::NORMAL)
                                        .font_family("monospace")
                                        .text_color(level_color)
                                        .child(entry.level.as_str().to_uppercase()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(entry.source.clone()),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(msg_color)
                                .child(entry.message.clone()),
                        )
                })),
        )
}

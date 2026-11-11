//! Toolbar containing master toggle, managed browser launcher, batch actions, and scope chips.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_destructive;
use crate::pages::chrome::{StatusTone, status_pill};
use crate::pages::editors::ToolEditors;

pub fn render_intercept_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let enabled = state.intercept.enabled;
    let active_tab_id = state.active_work_tab_id.as_deref().unwrap_or("");
    let tab_paused_count = state
        .paused
        .iter()
        .filter(|row| row.tab_id.as_deref() == Some(active_tab_id))
        .count();
    let has_paused = tab_paused_count > 0;
    let paused_count = tab_paused_count;
    let scope_input = editors.intercept_scope_input.clone();

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            // Top action bar
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .items_center()
                .child(
                    Button::new("intercept-toggle")
                        .variant(if enabled {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        })
                        .size(ButtonSize::Small)
                        .leading_icon(if enabled {
                            PhosphorIcon::Pause
                        } else {
                            PhosphorIcon::Play
                        })
                        .label(if enabled {
                            "Intercept is ON"
                        } else {
                            "Intercept is OFF"
                        })
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::set_intercept_enabled(!enabled, cx);
                        }),
                )
                .child(if enabled {
                    status_pill(
                        format!("{paused_count} held"),
                        if has_paused {
                            StatusTone::Busy
                        } else {
                            StatusTone::Active
                        },
                        cx,
                    )
                    .into_any_element()
                } else {
                    status_pill("DISABLED", StatusTone::Idle, cx).into_any_element()
                })
                .child(
                    Button::new("intercept-open-browser")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Globe)
                        .label("Open Browser")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::open_intercept_browser(cx);
                        }),
                )
                .child(
                    Button::new("intercept-refresh")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::ArrowClockwise)
                        .label("Refresh")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::refresh_intercept(cx);
                        }),
                )
                .child(
                    Button::new("intercept-fwd-all")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Forward all")
                        .disabled(!has_paused)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::forward_all_paused(cx);
                        }),
                )
                .child(
                    Button::icon("intercept-drop-all", PhosphorIcon::Trash)
                        .variant(ButtonVariant::Destructive)
                        .size(ButtonSize::Small)
                        .accessibility_label("Drop all paused items")
                        .tooltip("Drop all")
                        .disabled(!has_paused)
                        .on_click(move |_, window, cx| {
                            confirm_destructive(
                                window,
                                cx,
                                "Drop all paused items?",
                                format!(
                                    "{paused_count} paused item(s) in this tab will be discarded and never forwarded. This cannot be undone."
                                ),
                                "Drop all",
                                |_, cx| {
                                    let _ = Bridge::drop_all_paused(cx);
                                },
                            );
                        }),
                ),
        )
        .child(
            // Scope patterns chip bar
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .items_center()
                .py_1()
                .px_2()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(muted)
                        .child("Target Scope:"),
                )
                .children(if state.intercept.capture_patterns.is_empty() {
                    vec![
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("All hosts (*) [Intercepts everything]")
                            .into_any_element(),
                    ]
                } else {
                    state
                        .intercept
                        .capture_patterns
                        .iter()
                        .map(|pat| {
                            let p = pat.clone();
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .px_1p5()
                                .py_0p5()
                                .rounded(theme.radius_sm())
                                .bg(theme.accent)
                                .text_xs()
                                .child(pat.clone())
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .hover(|s| s.text_color(theme.danger))
                                        .child("×")
                                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                            let _ = Bridge::remove_intercept_scope(&p, cx);
                                        }),
                                )
                                .into_any_element()
                        })
                        .collect()
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .w(px(260.))
                        .child(div().flex_1().child(scope_input.clone()))
                        .child(
                            Button::new("add-scope-btn")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Plus)
                                .on_click({
                                    let input = scope_input.clone();
                                    move |_, window, cx| {
                                        let val = input.read(cx).value().to_string();
                                        let trimmed = val.trim();
                                        if !trimmed.is_empty() {
                                            let _ = Bridge::add_intercept_scope(trimmed, cx);
                                            input.update(cx, |s, cx| {
                                                s.set_value("", window, cx);
                                            });
                                        }
                                    }
                                }),
                        ),
                ),
        )
}

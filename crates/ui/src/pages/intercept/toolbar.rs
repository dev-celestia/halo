//! Command bar — master switch, target scope, and batch actions on one line.
//!
//! Mirrors the `pages/crawler` command bar: one wrapping row, a `flex_1` field
//! in the middle, `flex_none` clusters either side, 12px between zones and 8px
//! inside a cluster.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::chip::Chip;
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_destructive;
use crate::pages::chrome::{SECTION_GAP, StatusTone, status_pill};
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
    let paused_count = state
        .paused
        .iter()
        .filter(|row| row.tab_id.as_deref() == Some(active_tab_id))
        .count();
    let has_paused = paused_count > 0;
    let scope_input = editors.intercept_scope_input.clone();

    // Scope tokens stay inline with the field they belong to. With no pattern
    // configured the scope is open, which the field's placeholder cannot say.
    let mut tokens: Vec<AnyElement> = Vec::new();
    if state.intercept.capture_patterns.is_empty() {
        tokens.push(
            div()
                .flex_none()
                .text_xs()
                .text_color(muted)
                .child("All hosts (*)")
                .into_any_element(),
        );
    } else {
        for (index, pattern) in state.intercept.capture_patterns.iter().enumerate() {
            let removable = pattern.clone();
            tokens.push(
                Chip::new(
                    SharedString::from(format!("scope-{index}")),
                    pattern.clone(),
                )
                .soft()
                .on_dismiss(move |_, _, cx| {
                    let _ = Bridge::remove_intercept_scope(&removable, cx);
                })
                .into_any_element(),
            );
        }
    }

    div()
        .id("intercept-toolbar")
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .px_1()
        .gap(px(SECTION_GAP))
        // State: the master switch and what it is holding right now.
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .flex_none()
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
                .child(status_pill(
                    if enabled {
                        format!("{paused_count} held")
                    } else {
                        "DISABLED".to_string()
                    },
                    if !enabled {
                        StatusTone::Idle
                    } else if has_paused {
                        StatusTone::Busy
                    } else {
                        StatusTone::Active
                    },
                    cx,
                )),
        )
        // Target scope: live tokens, the next pattern, and its submit.
        .child(
            div()
                .flex_1()
                .min_w(px(260.))
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .children(tokens)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .child(Input::new(&scope_input)),
                )
                .child(
                    Button::new("add-scope-btn")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Plus)
                        .label("Add")
                        .on_click(move |_, window, cx| {
                            let value = scope_input.read(cx).value().to_string();
                            let trimmed = value.trim();
                            if !trimmed.is_empty() {
                                let _ = Bridge::add_intercept_scope(trimmed, cx);
                                scope_input.update(cx, |input, cx| {
                                    input.set_value("", window, cx);
                                });
                            }
                        }),
                ),
        )
        // Batch actions on everything held in this tab.
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .flex_none()
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
                    Button::icon("intercept-refresh", PhosphorIcon::ArrowClockwise)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .accessibility_label("Refresh paused queue")
                        .tooltip("Refresh")
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
}

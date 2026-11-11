//! Environment variables setup modal dialog.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_delete;
use crate::pages::editors::ToolEditors;

const DEFAULT_ENV_JSON: &str = r#"{"host":{"value":"example.com","enabled":true}}"#;

pub fn render_env_modal(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let contexts = state.contexts.clone();
    let active_id = state.active_context_id.clone();
    let active_ctx = contexts.iter().find(|c| Some(&c.id) == active_id.as_ref());
    let active_name = active_ctx
        .map(|c| c.name.as_str())
        .unwrap_or("No environment");

    let env_vars = editors.repeater_env_vars.clone();

    div()
        .id("env-modal-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background.opacity(0.60))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            Bridge::set_env_modal_open(false, cx);
        })
        .child(
            div()
                .id("env-modal-panel")
                .role(Role::Dialog)
                .aria_label("Environment Setup")
                .w(px(720.))
                .max_w(relative(0.95))
                .h(px(520.))
                .max_h(relative(0.92))
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(theme.radius_xl())
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .shadow(vec![BoxShadow {
                    offset: point(px(0.), px(18.)),
                    blur_radius: px(48.),
                    spread_radius: px(0.),
                    inset: false,
                    color: hsla(0., 0., 0., 0.35),
                }])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Header
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_5()
                        .py_3p5()
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.12))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(28.))
                                        .h(px(28.))
                                        .rounded(theme.radius)
                                        .bg(theme.primary.opacity(0.12))
                                        .child(
                                            Phosphor::new(PhosphorIcon::SlidersHorizontal)
                                                .size(px(16.))
                                                .color(theme.primary),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(theme.foreground)
                                                .child("Environment Variables"),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child(
                                                    "Configure environments and variables expanded into {{var}} placeholders on Send.",
                                                ),
                                        ),
                                ),
                        )
                        .child(
                            Button::icon(
                                "env-modal-close",
                                Phosphor::new(PhosphorIcon::X).size(px(14.)),
                            )
                            .size(ButtonSize::IconSm)
                            .variant(ButtonVariant::Ghost)
                            .accessibility_label("Close")
                            .tooltip("Close")
                            .on_click(|_, _, cx| Bridge::set_env_modal_open(false, cx)),
                        ),
                )
                // Body: Split view (Environments list on left, form on right)
                .child(
                    div()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        // Left Rail
                        .child(
                            div()
                                .w(px(220.))
                                .flex_none()
                                .border_r_1()
                                .border_color(theme.border)
                                .bg(theme.muted.opacity(0.06))
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .px_3()
                                        .py_2()
                                        .border_b_1()
                                        .border_color(theme.border)
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(theme.muted_foreground)
                                                .child("ENVIRONMENTS"),
                                        )
                                        .child(
                                            Button::new("env-modal-add")
                                                .variant(ButtonVariant::Ghost)
                                                .size(ButtonSize::Small)
                                                .leading_icon(PhosphorIcon::Plus)
                                                .label("New")
                                                .on_click({
                                                    let env_vars = env_vars.clone();
                                                    move |_, window, cx| {
                                                        let n = Bridge::state(cx)
                                                            .read(cx)
                                                            .contexts
                                                            .len()
                                                            + 1;
                                                        let _ = Bridge::create_context(
                                                            &format!("Env {n}"),
                                                            DEFAULT_ENV_JSON,
                                                            cx,
                                                        );
                                                        env_vars.update(cx, |form, cx| {
                                                            form.set_from_json(
                                                                DEFAULT_ENV_JSON,
                                                                window,
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .overflow_y_scrollbar()
                                        .p_1p5()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .children(contexts.iter().map(|ctx| {
                                            let id = ctx.id.clone();
                                            let name = ctx.name.clone();
                                            let name_for_delete = name.clone();
                                            let vars = ctx.variables.clone();
                                            let is_selected = active_id.as_deref() == Some(id.as_str());
                                            let env_vars = env_vars.clone();
                                            let can_delete = contexts.len() > 1;

                                            div()
                                                .id(SharedString::from(format!("env-item-{}", ctx.id)))
                                                .w_full()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .px_2p5()
                                                .py_1p5()
                                                .rounded(theme.radius)
                                                .cursor_pointer()
                                                .when(is_selected, |this| {
                                                    this.bg(theme.accent.opacity(0.14))
                                                        .border_1()
                                                        .border_color(theme.accent.opacity(0.3))
                                                })
                                                .when(!is_selected, |this| {
                                                    this.hover(|h| h.bg(theme.muted.opacity(0.2)))
                                                })
                                                .on_click({
                                                    let id = id.clone();
                                                    let env_vars = env_vars.clone();
                                                    let vars = vars.clone();
                                                    move |_, window, cx| {
                                                        Bridge::set_active_context(Some(id.clone()), cx);
                                                        env_vars.update(cx, |form, cx| {
                                                            form.set_from_json(&vars, window, cx);
                                                        });
                                                    }
                                                })
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .min_w_0()
                                                        .flex_1()
                                                        .child(
                                                            div()
                                                                .w(px(6.))
                                                                .h(px(6.))
                                                                .rounded(px(3.))
                                                                .bg(if is_selected {
                                                                    theme.primary
                                                                } else {
                                                                    theme.muted_foreground.opacity(0.3)
                                                                }),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_weight(FontWeight::NORMAL)
                                                                .text_color(if is_selected {
                                                                    theme.foreground
                                                                } else {
                                                                    theme.muted_foreground
                                                                })
                                                                .overflow_hidden()
                                                                .whitespace_nowrap()
                                                                .child(name),
                                                        ),
                                                )
                                                .when(can_delete, |this| {
                                                    let id = id.clone();
                                                    let env_vars = env_vars.clone();
                                                    let name = name_for_delete.clone();
                                                    this.child(
                                                        Button::icon(
                                                            SharedString::from(format!("env-del-{}", ctx.id)),
                                                            Phosphor::new(PhosphorIcon::Trash).size(px(12.)),
                                                        )
                                                        .size(ButtonSize::IconSm)
                                                        .variant(ButtonVariant::Ghost)
                                                        .accessibility_label("Delete environment")
                                                        .tooltip("Delete environment")
                                                        .on_click(move |_, window, cx| {
                                                            confirm_delete(
                                                                window,
                                                                cx,
                                                                "Delete environment?",
                                                                format!(
                                                                    "{name} and its variables will be removed. This cannot be undone."
                                                                ),
                                                                {
                                                                    let id = id.clone();
                                                                    let env_vars = env_vars.clone();
                                                                    move |window, cx| {
                                                                        let _ = Bridge::delete_context(&id, cx);
                                                                        let state = Bridge::state(cx).read(cx);
                                                                        let variables = state
                                                                            .active_context_id
                                                                            .as_ref()
                                                                            .and_then(|active_id| {
                                                                                state
                                                                                    .contexts
                                                                                    .iter()
                                                                                    .find(|c| &c.id == active_id)
                                                                                    .map(|c| c.variables.clone())
                                                                            });
                                                                        if let Some(v) = variables {
                                                                            env_vars.update(cx, |form, cx| {
                                                                                form.set_from_json(&v, window, cx);
                                                                            });
                                                                        }
                                                                    }
                                                                },
                                                            );
                                                        }),
                                                    )
                                                })
                                        })),
                                ),
                        )
                        // Right form pane
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .p_4()
                                .gap_2()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(theme.foreground)
                                                .child(format!("{active_name} Variables")),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child("Expanded on Send: {{var}}"),
                                        ),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .flex_1()
                                        .min_h_0()
                                        .child(editors.repeater_env_vars.clone()),
                                ),
                        ),
                )
                // Footer
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_5()
                        .py_3()
                        .border_t_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.06))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("Active: {active_name}")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("env-modal-save")
                                        .variant(ButtonVariant::Primary)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::FloppyDisk)
                                        .label("Save Environment")
                                        .on_click({
                                            let env_vars = env_vars.clone();
                                            move |_, _, cx| {
                                                let Some(id) = Bridge::state(cx)
                                                    .read(cx)
                                                    .active_context_id
                                                    .clone()
                                                else {
                                                    return;
                                                };
                                                let json = env_vars.read(cx).to_json(cx);
                                                let _ = Bridge::save_context_variables(
                                                    &id, &json, cx,
                                                );
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("env-modal-done")
                                        .variant(ButtonVariant::Secondary)
                                        .size(ButtonSize::Small)
                                        .label("Done")
                                        .on_click(|_, _, cx| {
                                            Bridge::set_env_modal_open(false, cx);
                                        }),
                                ),
                        ),
                ),
        )
}

//! Action triggers, model picker anchor, and draft submission controls.

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::InputState;
use celestia_ui::components::primitive::textarea::TextareaState;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{AgentApprovalMode, Bridge, BridgeState};

use super::super::model_picker;

/// Format `84200` → `84.2k` for the context tooltip.
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Context window fill — sits before the model picker once Pi reports usage.
pub(super) fn context_usage_chip(
    state: &BridgeState,
    muted: Hsla,
    accent: Hsla,
    cx: &App,
) -> AnyElement {
    let Some(usage) = state.coder.agent_context_usage.as_ref() else {
        return Empty.into_any_element();
    };
    if usage.window_tokens == 0 {
        return Empty.into_any_element();
    }

    let pct = usage.percent();
    let theme = cx.theme();
    let tone = if pct >= 90 {
        theme.danger
    } else if pct >= 75 {
        theme.warning
    } else {
        muted
    };
    let tip = format!(
        "Context · {} / {} tokens · {} in · {} out",
        format_tokens(usage.used_tokens),
        format_tokens(usage.window_tokens),
        format_tokens(usage.input_tokens),
        format_tokens(usage.output_tokens),
    );

    // Mini meter: track + fill proportional to percent used.
    let track_w = 28.0_f32;
    let fill_w = ((pct as f32 / 100.0) * track_w).clamp(1.5, track_w);

    div()
        .id("coder-agent-context-usage")
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .gap(px(6.))
        .h(px(30.))
        .px(px(8.))
        .rounded(cx.theme().radius_sm())
        .cursor_default()
        .hover(move |s| s.bg(accent))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .child(
            div()
                .flex_none()
                .w(px(track_w))
                .h(px(4.))
                .rounded(px(2.))
                .bg(muted.opacity(0.25))
                .overflow_hidden()
                .child(div().h_full().w(px(fill_w)).rounded(px(2.)).bg(tone)),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(tone)
                .child(format!("{pct}%")),
        )
        .into_any_element()
}

/// Plan-mode toggle — sits beside the attach control in the composer toolbar.
///
/// Active is a pale success wash with green label: solid green shouted from a
/// quiet toolbar, a bare 16% tint vanished into the card surface, so the fill
/// sits between the two and steps up slightly on hover.
pub(super) fn plan_mode_chip(active: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let is_dark = theme.is_dark();
    let fill = if is_dark { 0.22 } else { 0.14 };
    let (color, hover_bg) = if active {
        (theme.success, theme.success.opacity(fill + 0.08))
    } else {
        (theme.muted_foreground, theme.accent)
    };
    let mut el = div()
        .id("coder-agent-plan-mode")
        .flex()
        .items_center()
        .flex_none()
        .gap(px(3.))
        .h(px(30.))
        .px(px(6.))
        .rounded(cx.theme().radius_sm())
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(|_, _, cx| {
            let _ = Bridge::halo_agent_toggle_plan_mode(cx);
        })
        .child(
            Phosphor::new(PhosphorIcon::ListChecks)
                .size(px(15.))
                .color(color),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(color)
                .child("Plan"),
        );
    if active {
        el = el.bg(theme.success.opacity(fill));
    }
    el
}

fn approval_mode_icon(mode: AgentApprovalMode) -> PhosphorIcon {
    match mode {
        AgentApprovalMode::Ask => PhosphorIcon::Hand,
        AgentApprovalMode::ApproveEdits => PhosphorIcon::PencilSimple,
        AgentApprovalMode::ApproveSafe => PhosphorIcon::ShieldCheck,
        // Warning triangle — Full access is a danger state, not a neutral shield.
        AgentApprovalMode::FullAccess => PhosphorIcon::Warning,
    }
}

fn approval_is_danger(mode: AgentApprovalMode) -> bool {
    matches!(mode, AgentApprovalMode::FullAccess)
}

/// Per-session approval chip + flyout — sits beside Plan in the composer toolbar.
pub(super) fn approval_picker_anchor(
    state: &BridgeState,
    muted: Hsla,
    accent: Hsla,
    cx: &App,
) -> impl IntoElement {
    let open = state.coder.agent_approval_picker_open;
    let mode = state.coder.agent_approval_mode;
    let tip = format!("{} — {}", mode.title(), mode.description());
    let danger = cx.theme().danger;
    let danger_mode = approval_is_danger(mode);
    let label_color = if danger_mode { danger } else { muted };
    let trigger_radius = cx.theme().radius_sm();

    div()
        .id("coder-agent-approval-anchor")
        .relative()
        .min_w_0()
        .flex_shrink(1.)
        .child(
            div()
                .id("coder-agent-approval-trigger")
                .flex()
                .items_center()
                .gap(px(5.))
                .h(px(30.))
                .px(px(8.))
                .rounded(trigger_radius)
                .cursor_pointer()
                .hover(move |s| s.bg(accent))
                .when(open, |el| el.bg(accent))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                // Press-time toggle (see `model_picker_anchor`): keeps the sheet's
                // outside-dismiss from swallowing the trigger press.
                .capture_any_mouse_down(move |event, _, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    let _ = Bridge::halo_agent_toggle_approval_picker(cx);
                    cx.stop_propagation();
                })
                .child(
                    Phosphor::new(approval_mode_icon(mode))
                        .size(px(15.))
                        .color(label_color),
                )
                .child(
                    div()
                        .min_w_0()
                        .max_w(px(120.))
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(label_color)
                        .child(mode.title()),
                )
                .child(
                    Phosphor::new(PhosphorIcon::CaretDown)
                        .size(px(12.))
                        .color(muted),
                ),
        )
        .when(open, move |el| {
            // Top-layer paint (see `model_picker_anchor`): the sheet must sit above
            // the work-tab strip, dock, and side-panel toolbars, not just its pane.
            el.child(
                deferred(
                    div()
                        .id("coder-agent-approval-flyout")
                        .absolute()
                        .occlude()
                        .left(px(0.))
                        .bottom(px(32.))
                        .w(px(300.))
                        .overflow_hidden()
                        .rounded(cx.theme().radius)
                        .bg(model_picker::elevated_surface(
                            cx.theme().popover,
                            cx.theme().is_dark(),
                        ))
                        .shadow(picker_elevation(cx.theme().is_dark()))
                        // Dismiss on a press outside the sheet — never on the anchor,
                        // whose bounds are only the 30px trigger row (see the model picker).
                        .on_mouse_down_out(|_, _, cx| {
                            let _ = Bridge::halo_agent_close_approval_picker(cx);
                        })
                        .child(celestia_ui::motion::menu_in_from(
                            "coder-agent-approval-flyout-in",
                            6.0,
                            div().w_full().child(approval_picker_menu(
                                mode,
                                state.coder.agent_harness.clone(),
                                cx,
                            )),
                        )),
                )
                .with_priority(2),
            )
        })
}

fn approval_picker_menu(
    selected: AgentApprovalMode,
    harness: String,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    // Outer pad 4. `radius_sm` is half the base, which only matches this inset
    // when the base radius is 8.
    let row_radius = (theme.radius - px(4.)).max(px(0.));

    let mut list = div()
        .w_full()
        .flex()
        .flex_col()
        .p(px(4.))
        .gap(px(1.))
        .child(
            div()
                .px(px(8.))
                .pt(px(4.))
                .pb(px(6.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("When an agent wants to change something"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("How far this session may go without asking you first."),
                ),
        );

    for mode in AgentApprovalMode::ALL {
        list = list.child(approval_picker_option(
            mode,
            mode == selected,
            row_radius,
            cx,
        ));
    }

    if harness == "cursor" {
        list = list.child(
            div()
                .px(px(8.))
                .pt(px(4.))
                .pb(px(2.))
                .text_xs()
                .text_color(muted)
                .child("Cursor runs tools in its own runtime. This choice is saved with the chat and applied on Pi, OpenCode, and Antigravity."),
        );
    }

    list
}

fn approval_picker_option(
    mode: AgentApprovalMode,
    selected: bool,
    row_radius: Pixels,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let accent = theme.accent;
    let danger = theme.danger;
    let dangerous = approval_is_danger(mode);
    let icon = approval_mode_icon(mode);

    // Full access paints icon + copy in danger red when selected — the static
    // cue for a high-risk mode (motion is not the only signal).
    let icon_color = if selected && dangerous {
        danger
    } else if selected {
        foreground
    } else {
        muted
    };
    let title_color = if selected && dangerous {
        danger
    } else {
        foreground
    };
    let desc_color = if selected && dangerous {
        danger.opacity(0.85)
    } else {
        muted
    };
    let check_color = if dangerous { danger } else { foreground };

    div()
        .id(SharedString::from(format!(
            "coder-agent-approval-pick-{}",
            mode.id()
        )))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .px(px(8.))
        .py(px(8.))
        .rounded(row_radius)
        .bg(if selected {
            if dangerous {
                danger.opacity(0.10)
            } else {
                accent.opacity(0.55)
            }
        } else {
            transparent_black()
        })
        .cursor_pointer()
        .hover(move |s| {
            if selected {
                s
            } else {
                s.bg(accent.opacity(0.35))
            }
        })
        .on_click(move |_, _, cx| {
            let _ = Bridge::halo_agent_set_approval_mode(mode, cx);
        })
        .child(Phosphor::new(icon).size(px(16.)).color(icon_color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(title_color)
                        .child(mode.title()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(desc_color)
                        .child(mode.description()),
                ),
        )
        .when(selected, |el| {
            el.child(
                Phosphor::new(PhosphorIcon::Check)
                    .size(px(14.))
                    .color(check_color),
            )
        })
}

/// Trigger + sheet. The trigger toggles on the press (capture phase) so the
/// sheet's own outside-dismiss cannot race it; presses inside the sheet never
/// dismiss, because the dismiss listener lives on the sheet's bounds.
pub(super) fn model_picker_anchor(
    state: &BridgeState,
    running: bool,
    foreground: Hsla,
    muted: Hsla,
    accent: Hsla,
    search: Entity<InputState>,
    draft: Entity<TextareaState>,
    cx: &App,
) -> impl IntoElement {
    // A live turn locks the model — never show the sheet over a working agent.
    let open = state.coder.agent_picker_open && !running;

    // The chip may truncate; the sheet must not. A shrinkable relative parent
    // clamped the absolute flyout to the trigger width, so long model names
    // (and the search field) looked squeezed into the toolbar slot.
    div()
        .id("coder-agent-model-anchor")
        .relative()
        .flex_none()
        .child(model_trigger(
            state,
            running,
            foreground,
            muted,
            accent,
            search.clone(),
            draft.clone(),
            cx,
        ))
        .when(open, move |el| {
            // Deferred so the sheet paints in the window's top layer. As a plain
            // absolute child it only outranks its own pane, so the work-tab strip,
            // the dock, and side-panel toolbars kept painting over it (and the
            // pane's overflow clipped its top once it grew past the composer).
            el.child(
                deferred(
                    div()
                        .id("coder-agent-picker-flyout")
                        .absolute()
                        .occlude()
                        .right(px(0.))
                        .bottom(px(32.))
                        .w(model_picker::PICKER_WIDTH)
                        .min_w(model_picker::PICKER_WIDTH)
                        .max_h(model_picker::PICKER_MAX)
                        .overflow_hidden()
                        .rounded(cx.theme().radius)
                        .bg(model_picker::elevated_surface(
                            cx.theme().popover,
                            cx.theme().is_dark(),
                        ))
                        .shadow(picker_elevation(cx.theme().is_dark()))
                        // Dismiss on a press that misses the sheet. The listener must sit
                        // on the sheet, not on the anchor: the flyout overflows the 30px
                        // trigger row, so an anchor-level `mouse_down_out` fired for every
                        // press *inside* the list, closed the picker during the capture
                        // phase, and the row's click (which needs its mouse-up) never
                        // landed — picking a model silently did nothing.
                        .on_mouse_down_out(|_, _, cx| {
                            let _ = Bridge::halo_agent_close_picker(cx);
                        })
                        .child(celestia_ui::motion::menu_in_from(
                            "coder-agent-picker-flyout-in",
                            6.0,
                            div()
                                .w(model_picker::PICKER_WIDTH)
                                .min_w(model_picker::PICKER_WIDTH)
                                .max_h(model_picker::PICKER_MAX)
                                .overflow_hidden()
                                .child(model_picker::render_picker(state, &search, &draft, cx)),
                        )),
                )
                .with_priority(2),
            )
        })
}

/// Layered transparent lift on light; a thin white ring on dark.
fn picker_elevation(is_dark: bool) -> Vec<BoxShadow> {
    if is_dark {
        vec![
            BoxShadow::new(px(0.), px(0.), hsla(0., 0., 1., 0.08))
                .blur_radius(px(0.))
                .spread_radius(px(1.)),
            BoxShadow::new(px(0.), px(12.), hsla(0., 0., 0., 0.35))
                .blur_radius(px(28.))
                .spread_radius(px(-8.)),
        ]
    } else {
        vec![
            BoxShadow::new(px(0.), px(0.), hsla(0., 0., 0., 0.06))
                .blur_radius(px(0.))
                .spread_radius(px(1.)),
            BoxShadow::new(px(0.), px(4.), hsla(0., 0., 0., 0.08))
                .blur_radius(px(12.))
                .spread_radius(px(-2.)),
            BoxShadow::new(px(0.), px(12.), hsla(0., 0., 0., 0.10))
                .blur_radius(px(28.))
                .spread_radius(px(-4.)),
        ]
    }
}

/// Model trigger — harness brand mark + model name. The harness name stays on
/// the tooltip so the chip stays compact (icon only for the runtime).
fn model_trigger(
    state: &BridgeState,
    running: bool,
    foreground: Hsla,
    muted: Hsla,
    accent: Hsla,
    search: Entity<InputState>,
    draft: Entity<TextareaState>,
    cx: &App,
) -> impl IntoElement {
    let harness = super::super::harness_label(state);
    let model = Bridge::halo_agent_list_models(&state.coder.agent_harness, cx)
        .ok()
        .and_then(|models| {
            models
                .into_iter()
                .find(|m| Some(m.id.as_str()) == state.coder.agent_model.as_deref())
                .map(|m| m.display_name)
        })
        .or_else(|| state.coder.agent_model.clone())
        .unwrap_or_else(|| "Select model".to_string());
    let (mark, tint) = model_picker::model_mark(
        &state.coder.agent_harness,
        state.coder.agent_model.as_deref().unwrap_or(""),
    );
    let tip_label = format!("{harness} · {model} — click to switch harness or model");

    let label_color = if running { muted } else { foreground };
    let glyph = model_picker::render_mark(mark, tint, px(14.), PhosphorIcon::Sparkle, muted);

    div()
        .id("coder-agent-model-trigger")
        .flex()
        .items_center()
        .gap(px(5.))
        .h(px(30.))
        .px(px(8.))
        .max_w(px(260.))
        .rounded(cx.theme().radius_sm())
        .when(running, |el| el.opacity(0.55).cursor_not_allowed())
        .when(!running, |el| {
            el.cursor_pointer()
                .hover(move |s| s.bg(accent))
                .tooltip(move |window, cx| Tooltip::new(tip_label.clone()).build(window, cx))
        })
        .when(running, |el| {
            el.tooltip(|window, cx| {
                Tooltip::new("Model is locked while the agent is working").build(window, cx)
            })
        })
        .child(glyph)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(label_color)
                .child(model),
        )
        .child(
            Phosphor::new(PhosphorIcon::CaretDown)
                .size(px(12.))
                .color(muted),
        )
        // Toggle on the press, during the capture phase. The sheet's own
        // outside-dismiss is registered *after* this listener (it paints later), so
        // stopping propagation here is what keeps a trigger press from dismissing the
        // sheet and then having the click re-open it. A click handler cannot do this:
        // gpui only arms `on_click` in the bubble phase, which never runs.
        .capture_any_mouse_down(move |event, window, cx| {
            if event.button != MouseButton::Left {
                return;
            }
            if Bridge::state(cx).read(cx).coder.agent_running {
                cx.stop_propagation();
                return;
            }
            let opening = !Bridge::state(cx).read(cx).coder.agent_picker_open;
            let harness_label = {
                let state = Bridge::state(cx).read(cx);
                super::super::harness_label(&state)
            };
            // Opens immediately; catalog refresh runs off-thread.
            let _ = Bridge::halo_agent_toggle_picker(cx);
            if opening {
                let placeholder = format!("Filter {harness_label} models…");
                search.update(cx, |search, cx| {
                    search.set_value("", window, cx);
                    search.set_placeholder(placeholder, window, cx);
                    search.focus(window, cx);
                });
            } else {
                draft.update(cx, |draft, cx| draft.focus(window, cx));
            }
            cx.stop_propagation();
        })
}

/// One primary action at a time: send when idle or while a turn is live (the
/// bridge steers into that turn), stop when the draft is empty. Uses Celestia [`Button::icon`].
pub(super) fn trailing_action(
    running: bool,
    draft_empty: bool,
    input: Entity<TextareaState>,
    _primary: Hsla,
    _primary_foreground: Hsla,
    _danger: Hsla,
    _danger_foreground: Hsla,
) -> AnyElement {
    if running && draft_empty {
        return Button::icon("coder-agent-stop", PhosphorIcon::Stop)
            .variant(ButtonVariant::Destructive)
            .size(ButtonSize::Icon)
            .accessibility_label("Stop")
            .tooltip("Stop")
            .on_click(|_, _, cx| {
                let _ = Bridge::halo_agent_interrupt(cx);
            })
            .into_any_element();
    }

    let (label, tip) = if running {
        ("Send", "Send to the current turn")
    } else {
        ("Send", "Send")
    };
    Button::icon("coder-agent-send", PhosphorIcon::ArrowUp)
        .variant(ButtonVariant::Primary)
        .size(ButtonSize::Icon)
        .accessibility_label(label)
        .tooltip(tip)
        .on_click(move |_, window, cx| submit_draft(&input, window, cx))
        .into_any_element()
}

fn submit_draft(input: &Entity<TextareaState>, window: &mut Window, cx: &mut App) {
    let text = input.read(cx).value().to_string();
    if Bridge::halo_agent_submit(&text, cx).is_ok() {
        input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
    }
}

//! Error, tool-status, and interrupted cards plus attachment chips.

use super::*;

/// Failed turn: show the host error and a Resend that requeues the last user
/// bubble (queue rows for confirmed failures are already dropped).
pub(crate) fn error_card(
    id: SharedString,
    text: String,
    agent_running: bool,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let danger = theme.danger;
    let border = theme.border;
    let chip_radius = px(6.);
    let repair = needs_ownership_repair(&text);
    let pick_model = !repair && needs_another_model(&text);
    let title = if turn_died_after_send(&text) {
        "Turn failed"
    } else {
        "Send failed"
    };
    let copy_text = text.clone();

    div()
        .id(id.clone())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .px(px(12.))
        .py(px(10.))
        .rounded(theme.radius)
        .border_1()
        .border_color(border)
        .bg(theme.popover)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_none()
                        .size(px(22.))
                        .rounded(chip_radius)
                        .bg(danger.opacity(0.14))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::WarningCircle)
                                .size(px(12.))
                                .color(danger),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .line_height(CARD_TITLE_LINE)
                        .font_weight(FontWeight::NORMAL)
                        .text_color(foreground)
                        .child(title),
                )
                .child(copy_button(
                    SharedString::from(format!("{id}:copy-error")),
                    copy_text,
                    "Copy error",
                    cx,
                )),
        )
        .child(
            div()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(danger)
                .whitespace_normal()
                .child(text),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(muted)
                        .child(if repair {
                            "Fix directory ownership before retrying"
                        } else if pick_model {
                            "Selected model is unavailable on this harness"
                        } else {
                            "Retry the last prompt"
                        }),
                )
                .child(if repair {
                    Button::new(SharedString::from(format!("{id}:fix-ownership")))
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::ShieldCheck)
                        .label("Fix ownership")
                        .disabled(agent_running)
                        .on_click(|_, window, cx| {
                            crate::pages::doctor::request_fix(window, cx, "system:agent-state");
                        })
                        .into_any_element()
                } else if pick_model {
                    Button::new(SharedString::from(format!("{id}:choose-model")))
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Cube)
                        .label("Choose model")
                        .disabled(agent_running)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_open_picker(cx);
                        })
                        .into_any_element()
                } else {
                    Button::new(SharedString::from(format!("{id}:resend")))
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::PaperPlaneTilt)
                        .label("Resend")
                        .disabled(agent_running)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_retry_last(cx);
                        })
                        .into_any_element()
                }),
        )
        .into_any_element()
}

/// Host denied a tool (approval hook) rather than the tool itself failing.
pub(crate) fn is_approval_denial(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("not approved")
        || lower.contains("denied by the host")
        || (lower.contains("approval") && lower.contains("denied"))
}

/// Local denied/failed tool card — quieter title than Celestia `Tool::card`.
pub(crate) fn tool_status_card(
    id: SharedString,
    label: String,
    state: TraceState,
    expanded: bool,
    outcome: Option<String>,
    denied: bool,
    extras: Vec<AnyElement>,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let border = theme.border;
    let warning = theme.warning;
    let danger = theme.danger;
    let (badge_label, badge_tone) = match state {
        TraceState::Denied => ("Denied", warning),
        TraceState::Failed | TraceState::Error => ("Failed", danger),
        _ => ("Error", danger),
    };
    let caret = if expanded {
        PhosphorIcon::CaretDown
    } else {
        PhosphorIcon::CaretRight
    };
    let chip_radius = px(6.);

    let mut root = div()
        .id(id.clone())
        .w_full()
        .flex()
        .flex_col()
        .rounded(theme.radius)
        .border_1()
        .border_color(border)
        .bg(theme.popover)
        .overflow_hidden()
        .child(
            div()
                .id(SharedString::from(format!("{id}:header")))
                .w_full()
                .px(px(12.))
                .py(px(8.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .pr(px(18.))
                .capture_any_mouse_down(move |event, window, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    on_toggle(&ClickEvent::default(), window, cx);
                    cx.stop_propagation();
                })
                .child(
                    Phosphor::new(PhosphorIcon::Wrench)
                        .size(px(13.))
                        .color(muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .line_height(CARD_TITLE_LINE)
                        .font_weight(FontWeight::NORMAL)
                        .text_color(foreground)
                        .child(label),
                )
                .child(
                    div()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(chip_radius)
                        .bg(badge_tone.opacity(0.14))
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(badge_tone)
                        .child(badge_label),
                )
                .child(Phosphor::new(caret).size(px(12.)).color(muted)),
        );

    if expanded {
        root = root.child(
            div()
                .id(SharedString::from(format!("{id}:body")))
                .w_full()
                .flex_none()
                .px(px(12.))
                .pb(px(10.))
                .max_h(px(300.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(8.))
                .when_some(outcome, |el, detail| {
                    el.child(tool_outcome_banner(
                        SharedString::from(format!("{id}:outcome")),
                        detail,
                        denied,
                        cx,
                    ))
                })
                .children(extras),
        );
    }

    root.into_any_element()
}

/// Outcome notice inside a failed / denied tool card — chip label + muted detail.
pub(crate) fn tool_outcome_banner(
    id: SharedString,
    detail: String,
    denied: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let (tint, icon, title) = if denied {
        (theme.warning, PhosphorIcon::Prohibit, "Not approved")
    } else {
        (theme.danger, PhosphorIcon::XCircle, "Tool failed")
    };
    let chip_radius = px(6.);

    div()
        .id(id)
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_none()
                        .size(px(22.))
                        .rounded(chip_radius)
                        .bg(tint.opacity(0.14))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Phosphor::new(icon).size(px(12.)).color(tint)),
                )
                .child(
                    div()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(chip_radius)
                        .bg(tint.opacity(0.14))
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(tint)
                        .child(title),
                ),
        )
        .child(
            div()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(theme.muted_foreground)
                .whitespace_normal()
                .child(detail),
        )
}

/// Human interrupt notice — what happened, plus a way back into the turn.
pub(crate) fn interrupted_card(id: SharedString, agent_running: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let border = theme.border;
    let chip_radius = px(6.);

    div()
        .id(id.clone())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .px(px(12.))
        .py(px(10.))
        .rounded(theme.radius)
        .border_1()
        .border_color(border)
        .bg(theme.popover)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_none()
                        .size(px(22.))
                        .rounded(chip_radius)
                        .bg(muted.opacity(0.14))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Phosphor::new(PhosphorIcon::Stop).size(px(12.)).color(muted)),
                )
                .child(
                    div()
                        .text_sm()
                        .line_height(CARD_TITLE_LINE)
                        .font_weight(FontWeight::NORMAL)
                        .text_color(foreground)
                        .child("Response stopped"),
                ),
        )
        .child(
            div()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted)
                .whitespace_normal()
                .child(
                    "The turn was stopped before completion. Earlier file edits or commands may already have taken effect.",
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(muted)
                        .child("Retry whenever you're ready"),
                )
                .child(
                    Button::new(SharedString::from(format!("{id}:retry-last")))
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::ArrowClockwise)
                        .label("Try again")
                        .disabled(agent_running)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_retry_last(cx);
                        }),
                ),
        )
        .into_any_element()
}

/// Image chips for a user turn; hover shows a thumbnail. Rendered inside the
/// message bubble, so the row is intrinsic-width rather than a full-width band.
pub(crate) fn attachment_chips(paths: &[String], id: SharedString, cx: &App) -> impl IntoElement {
    let surface = cx.theme().secondary;
    let text = cx.theme().secondary_foreground;
    let muted = cx.theme().muted_foreground;
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(4.))
        .min_w_0()
        .children(paths.iter().enumerate().map(|(i, path)| {
            let name = attachment_label(path);
            let preview_path = path.clone();
            let tip_name = std::path::Path::new(path)
                .file_name()
                .and_then(|file_name| file_name.to_str())
                .unwrap_or(path)
                .to_string();
            div()
                .id(SharedString::from(format!("{id}:attachment:{i}")))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .px(px(8.))
                .py(px(3.))
                .rounded(px(6.))
                .bg(surface)
                .tooltip(move |_, cx| {
                    attachment_preview_tooltip(preview_path.clone(), tip_name.clone(), cx)
                })
                .child(
                    Phosphor::new(PhosphorIcon::Image)
                        .size(px(12.))
                        .color(muted),
                )
                .child(
                    div()
                        .max_w(px(240.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(text)
                        .child(name),
                )
        }))
}

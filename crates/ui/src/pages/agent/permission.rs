//! Human approver — question panel over the composer.
//!
//! Parks tool approvals / AskUserQuestion until the user picks an option and
//! advances (or dismisses). Binary Allow/Block tool prompts skip the option
//! list and use only the Deny / Allow footer — the radios were the same choice.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, HaloAgentInputQuestion};

use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};

pub(super) fn render_permission_panel(state: &BridgeState, cx: &App) -> AnyElement {
    let Some(pending) = state.coder.agent_pending_input.as_ref() else {
        return Empty.into_any_element();
    };
    let Some(question) = pending.questions.get(pending.page) else {
        return Empty.into_any_element();
    };

    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let border = theme.border;
    let surface = theme.popover;
    let accent = theme.accent;
    let warning = theme.warning;
    let chip_bg = warning.opacity(0.14);
    let radius = theme.radius;
    let row_radius = theme.radius_sm();
    let chip_radius = px(6.);

    let counter = if pending.questions.len() > 1 {
        Some(format!("{}/{}", pending.page + 1, pending.questions.len()))
    } else {
        None
    };
    // A child approval keeps its provenance: which delegated agent is asking.
    let subagent = pending
        .subagent
        .as_deref()
        .and_then(|session| {
            state
                .coder
                .agent_subagents
                .iter()
                .find(|child| child.session_id == session)
        })
        .map(|child| format!("Subagent · {}", child.label()));
    let last = pending.page + 1 >= pending.questions.len();
    let page = pending.page;
    let picked = pending.picked.get(page).cloned().unwrap_or_default();
    let binary = is_binary_allow_deny(question);
    // Binary tool prompts resolve via the footer alone — no prior radio pick.
    let can_advance = binary || !picked.is_empty() || question.options.is_empty();

    let options = question.options.iter().enumerate().map(|(ix, label)| {
        let is_picked = picked.contains(&ix);
        div()
            .id(("coder-agent-input-opt", ix))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.))
            .px(px(10.))
            .py(px(9.))
            .rounded(row_radius)
            .bg(if is_picked {
                accent
            } else {
                transparent_black()
            })
            .cursor_pointer()
            .hover(move |s| s.bg(accent))
            .on_click(move |_, _, cx| {
                let _ = Bridge::halo_agent_input_select(ix, cx);
            })
            .child(
                div()
                    .flex_none()
                    .size(px(16.))
                    .rounded_full()
                    .border_1()
                    .border_color(if is_picked {
                        warning
                    } else {
                        muted.opacity(0.45)
                    })
                    .bg(if is_picked {
                        warning
                    } else {
                        transparent_black()
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(is_picked, |el| {
                        el.child(
                            Phosphor::new(PhosphorIcon::Check)
                                .size(px(10.))
                                .color(theme.warning_foreground),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .line_height(CARD_TITLE_LINE)
                    .font_weight(FontWeight::NORMAL)
                    .text_color(foreground)
                    .child(label.clone()),
            )
            .when(ix < 9, |el| {
                el.child(
                    div()
                        .flex_none()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(chip_radius)
                        .bg(accent)
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{}", ix + 1)),
                )
            })
    });

    celestia_ui::motion::dialog_in(
        "coder-agent-permission-in",
        div()
            .id("coder-agent-permission")
            .w_full()
            .flex()
            .flex_col()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(surface)
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .px(px(14.))
                    .pt(px(12.))
                    .pb(px(10.))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .flex_none()
                            .mt(px(1.))
                            .size(px(26.))
                            .rounded(chip_radius)
                            .bg(chip_bg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Phosphor::new(PhosphorIcon::ShieldWarning)
                                    .size(px(13.))
                                    .color(warning),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
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
                                            .px(px(7.))
                                            .py(px(2.))
                                            .rounded(chip_radius)
                                            .bg(chip_bg)
                                            .text_xs()
                                            .font_weight(FontWeight::NORMAL)
                                            .text_color(warning)
                                            .child(question.header.to_uppercase()),
                                    )
                                    .when_some(subagent, |el, name| {
                                        el.child(
                                            div()
                                                .px(px(7.))
                                                .py(px(2.))
                                                .rounded(chip_radius)
                                                .bg(accent)
                                                .text_xs()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(muted)
                                                .child(name),
                                        )
                                    })
                                    .when_some(counter, |el, c| {
                                        el.child(
                                            div()
                                                .px(px(6.))
                                                .py(px(2.))
                                                .rounded(chip_radius)
                                                .bg(accent)
                                                .text_xs()
                                                .text_color(muted)
                                                .child(c),
                                        )
                                    }),
                            )
                            .when(binary, |el| {
                                el.child(command_preview(&question.question, cx))
                            })
                            .when(!binary, |el| {
                                el.child(
                                    div()
                                        .text_sm()
                                        .line_height(CARD_TITLE_LINE)
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(foreground)
                                        .whitespace_normal()
                                        .child(question.question.clone()),
                                )
                            }),
                    ),
            )
            .when(!binary, |el| {
                el.child(
                    div()
                        .w_full()
                        .px(px(12.))
                        .pt(px(10.))
                        .pb(px(4.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .when(question.multi_select, |el| {
                            el.child(
                                div()
                                    .text_xs()
                                    .line_height(META_LINE_HEIGHT)
                                    .text_color(muted)
                                    .child("Select one or more options."),
                            )
                        })
                        .child(div().flex().flex_col().gap(px(2.)).children(options)),
                )
            })
            .child(
                div()
                    .w_full()
                    .px(px(12.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(border)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1())
                    .child(
                        Button::new("coder-agent-input-dismiss")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .label(if binary { "Deny" } else { "Dismiss" })
                            .on_click(|_, _, cx| {
                                let _ = Bridge::halo_agent_input_dismiss(cx);
                            }),
                    )
                    .child(
                        Button::new("coder-agent-input-advance")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Small)
                            .label(if binary {
                                "Allow"
                            } else if last {
                                "Submit"
                            } else {
                                "Continue"
                            })
                            .disabled(!can_advance)
                            .on_click(move |_, _, cx| {
                                if binary {
                                    // Option 0 is always the positive choice for
                                    // harness Allow/Block prompts.
                                    let _ = Bridge::halo_agent_input_select(0, cx);
                                }
                                let _ = Bridge::halo_agent_input_advance(cx);
                            }),
                    ),
            ),
    )
    .into_any_element()
}

const COMMAND_LINES: f32 = 6.;

/// Binary tool text, scrolled, with a fade when more than six lines are hidden.
fn command_preview(text: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let inset = px(14.);
    let radius = (theme.radius - inset).max(px(0.));
    let line = px(18.);
    let max_h = line * COMMAND_LINES;
    let clipped = text.lines().count() > COMMAND_LINES as usize
        || text.chars().count() > (COMMAND_LINES as usize) * 72;
    let fill = theme.accent.opacity(0.35);
    let mono = theme.mono_font_family.clone();

    div()
        .id("coder-agent-command-preview")
        .relative()
        .w_full()
        .max_h(max_h)
        .overflow_y_scroll()
        .rounded(radius)
        .bg(fill)
        .px(px(8.))
        .py(px(6.))
        .child(
            div()
                .w_full()
                .font_family(mono)
                .text_xs()
                .line_height(line)
                .text_color(theme.foreground)
                .whitespace_normal()
                .child(text.to_string()),
        )
        .when(clipped, |el| {
            el.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(28.))
                    .rounded_b(radius)
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(fill.opacity(0.), 0.),
                        linear_color_stop(fill, 1.),
                    )),
            )
        })
        .into_any_element()
}

/// Tool-execution prompts from every harness ship as a single-select
/// `["Allow", "Block"]` pair. Showing those as radios *and* Deny/Allow buttons
/// asks the same question twice — collapse to the footer only.
fn is_binary_allow_deny(question: &HaloAgentInputQuestion) -> bool {
    if question.multi_select || question.options.len() != 2 {
        return false;
    }
    is_allow_label(&question.options[0]) && is_deny_label(&question.options[1])
}

fn is_allow_label(label: &str) -> bool {
    matches!(
        label.trim().to_ascii_lowercase().as_str(),
        "allow" | "approve" | "yes"
    )
}

fn is_deny_label(label: &str) -> bool {
    matches!(
        label.trim().to_ascii_lowercase().as_str(),
        "block" | "deny" | "reject" | "no"
    )
}

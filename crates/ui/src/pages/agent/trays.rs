//! Agent trays above the composer — todo plan and queued prompts.
//!
//! Shared chrome with the permission panel: regular outer border, no inner
//! row borders, status as a warning/danger chip.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::components::{Plan, PlanStep, PlanStepStatus};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, HaloAgentDelivery};

use super::{CARD_TITLE_LINE, LINE_HEIGHT, META_LINE_HEIGHT};

/// One truncated line and a chevron, so a collapsed tray still discloses its body.
fn summary_line(
    id: &'static str,
    label: String,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    div()
        .id(id)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .py(px(6.))
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .cursor_pointer()
        .on_click(on_click)
        .child(
            Phosphor::new(PhosphorIcon::CaretRight)
                .size(px(12.))
                .color(theme.muted_foreground),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(theme.foreground)
                .child(label),
        )
        .into_any_element()
}

pub(super) fn render_todo_tray(state: &BridgeState, summary: bool, cx: &App) -> AnyElement {
    let todos = &state.coder.agent_todos;
    if todos.is_empty() || state.coder.agent_todo_dismissed {
        return Empty.into_any_element();
    }
    if summary {
        let done = todos.iter().filter(|todo| todo.done).count();
        return summary_line(
            "coder-agent-todo-summary",
            format!("Plan · {done}/{}", todos.len()),
            |_, _, cx| {
                let _ = Bridge::halo_agent_toggle_dock_todo(cx);
            },
            cx,
        );
    }

    let theme = cx.theme();
    let border = theme.border;
    let surface = theme.popover;
    let radius = theme.radius;

    let done = todos.iter().filter(|t| t.done).count();
    let total = todos.len();
    let all_done = done == total && total > 0;
    let expanded = state.coder.agent_todo_expanded;

    let header = format!(
        "Plan · {done}/{total}{}",
        if all_done { " · Completed" } else { "" }
    );

    let mut plan = Plan::new("coder-agent-plan", header)
        .card(false)
        .open(expanded)
        .on_toggle(|_, _, cx| {
            let _ = Bridge::halo_agent_toggle_todo_expanded(cx);
        });

    if all_done {
        plan = plan.on_dismiss(|_, _, cx| {
            let _ = Bridge::halo_agent_dismiss_todos(cx);
        });
    }

    for (i, todo) in todos.iter().enumerate() {
        let status = match todo.status.as_str() {
            "completed" => PlanStepStatus::Done,
            "inProgress" => PlanStepStatus::Active,
            _ => PlanStepStatus::Pending,
        };
        plan = plan.step(PlanStep::new(("coder-agent-todo", i), todo.text.clone()).status(status));
    }

    celestia_ui::motion::menu_in_from(
        "coder-agent-todo-tray-in",
        6.0,
        div()
            .id("coder-agent-todo-tray")
            .w_full()
            .flex()
            .flex_col()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(surface)
            .overflow_hidden()
            .child(div().w_full().px(px(10.)).py(px(6.)).child(plan)),
    )
    .into_any_element()
}

pub(super) fn render_queue_tray(state: &BridgeState, summary: bool, cx: &App) -> AnyElement {
    if state.coder.agent_queue.is_empty() {
        return Empty.into_any_element();
    }
    if summary {
        return summary_line(
            "coder-agent-queue-summary",
            format!("Queue · {}", state.coder.agent_queue.len()),
            |_, _, cx| {
                let _ = Bridge::halo_agent_toggle_dock_queue(cx);
            },
            cx,
        );
    }

    let theme = cx.theme();
    let border = theme.border;
    let surface = theme.popover;
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let danger = theme.danger;
    let warning = theme.warning;
    let accent = theme.accent;
    let radius = theme.radius;
    let row_radius = theme.radius_sm();
    let chip_radius = px(6.);
    let queue_len = state.coder.agent_queue.len();

    let rows = state.coder.agent_queue.iter().enumerate().map(|(i, item)| {
        let id = item.id.clone();
        let base = if item.text.is_empty() && !item.attachments.is_empty() {
            format!(
                "{} image{}",
                item.attachments.len(),
                if item.attachments.len() == 1 { "" } else { "s" }
            )
        } else if !item.attachments.is_empty() {
            format!(
                "{} · {} image{}",
                item.text,
                item.attachments.len(),
                if item.attachments.len() == 1 { "" } else { "s" }
            )
        } else {
            item.text.clone()
        };

        let running = state.coder.agent_running;
        // The chip carries the state word and nothing else; any detail the
        // backend sent rides on its own quiet line under the prompt.
        let (status_label, status_tone, status_detail, status_live, can_send, can_remove): (
            Option<SharedString>,
            Option<Hsla>,
            Option<(String, Hsla)>,
            bool,
            bool,
            bool,
        ) = match &item.delivery {
            HaloAgentDelivery::Pending => (None, None, None, false, true, true),
            HaloAgentDelivery::Sending { .. } => (
                Some("Sending".into()),
                Some(muted),
                None,
                true,
                false,
                false,
            ),
            HaloAgentDelivery::Rejected { error } => (
                Some("Not sent".into()),
                Some(danger),
                Some((clip_line(error), danger)),
                false,
                !running,
                true,
            ),
            // Explicit retry only — never auto-drained. Send Now requeues as Pending.
            HaloAgentDelivery::Uncertain { error, .. } => (
                Some("Uncertain".into()),
                Some(warning),
                Some((clip_line(error), muted)),
                false,
                !running,
                true,
            ),
        };
        let send_id = id.clone();
        let rm_id = id.clone();
        let send_tip = match &item.delivery {
            HaloAgentDelivery::Uncertain { .. } => {
                "Retry send — check the transcript first; it may already have been applied"
            }
            _ => "Send now",
        };

        let has_detail = status_detail.is_some();
        div()
            .id(("coder-agent-q", i))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            // A two-line row hangs its chip off the prompt line, not the block.
            .when(has_detail, |row| row.items_start())
            .gap(px(8.))
            // 6px list inset + 4px row inset puts the prompt on the header's
            // 10px edge, and keeps the hover wash off the card border.
            .px(px(4.))
            .py(px(6.))
            .rounded(row_radius)
            .hover(move |s| s.bg(accent))
            .child(
                // The queued prompt is content, not chrome: body scale and
                // regular weight, so it never competes with the card header.
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .truncate()
                            .text_base()
                            .line_height(LINE_HEIGHT)
                            .text_color(foreground)
                            .child(base),
                    )
                    .when_some(status_detail, |column, (detail, tone)| {
                        column.child(
                            div()
                                .truncate()
                                .text_xs()
                                .line_height(META_LINE_HEIGHT)
                                .text_color(tone)
                                .child(detail),
                        )
                    }),
            )
            .when_some(status_label.zip(status_tone), |el, (label, tone)| {
                el.child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(4.))
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(chip_radius)
                        .bg(tone.opacity(0.14))
                        .text_xs()
                        .line_height(px(14.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(tone)
                        .whitespace_nowrap()
                        .when(status_live, |chip| {
                            chip.child(Loader::grid_shift().size(px(10.)).color(tone))
                        })
                        .child(label),
                )
            })
            .when(can_send, |el| {
                el.child(
                    Button::icon(("coder-agent-q-send", i), PhosphorIcon::PaperPlaneTilt)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .tooltip(send_tip)
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::halo_agent_queue_send_now(&send_id, cx);
                        }),
                )
            })
            .when(can_remove, |el| {
                el.child(
                    Button::icon(("coder-agent-q-rm", i), PhosphorIcon::X)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .tooltip("Remove")
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::halo_agent_queue_remove(&rm_id, cx);
                        }),
                )
            })
    });

    celestia_ui::motion::menu_in_from(
        "coder-agent-queue-in",
        6.0,
        div()
            .id("coder-agent-queue")
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
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .py(px(7.))
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .flex_none()
                            .size(px(20.))
                            .rounded(chip_radius)
                            .bg(accent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Phosphor::new(PhosphorIcon::Queue)
                                    .size(px(11.))
                                    .color(muted),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .line_height(CARD_TITLE_LINE)
                            .font_weight(FontWeight::NORMAL)
                            .text_color(foreground)
                            .child(format!("Queue · {queue_len}")),
                    )
                    .child(div().flex_1())
                    // Wayfinding copy, not a control: plain muted text, so the
                    // only filled shapes in this card are real state chips.
                    // Pending head auto-sends after a successful Completed turn.
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .text_color(muted)
                            .child("Sent automatically after current turn"),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px(px(6.))
                    .pt(px(2.))
                    .pb(px(6.))
                    .children(rows),
            ),
    )
    .into_any_element()
}

/// First non-blank line of a backend message, cut to one quiet row. The cut is
/// char-boundary safe; the layout clips it again with an ellipsis.
fn clip_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let mut end = line.len().min(140);
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    let mut clipped = line[..end].to_owned();
    if clipped.len() < line.len() {
        clipped.push('…');
    }
    clipped
}

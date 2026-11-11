//! Subagent surfaces: spawn cards in the parent transcript and the running tray.
//!
//! A delegation tool call (`task` / `subagent`) is painted as a card instead of
//! a plain tool chip. The card is the join point between the parent tool row and
//! the child session that the harness reports separately.

use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, HaloAgentSubagent, HaloAgentSubagentStatus};

use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};

/// One painted spawn row, already joined to its parent tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SpawnCard {
    pub tool_call_id: String,
    pub session_id: String,
    pub title: String,
    pub agent: Option<String>,
    pub status: HaloAgentSubagentStatus,
    pub events: usize,
    pub open: bool,
    pub waiting: bool,
}

pub(super) fn spawn_cards(children: &[HaloAgentSubagent], open: Option<&str>) -> Vec<SpawnCard> {
    children
        .iter()
        .filter_map(|child| {
            Some(SpawnCard {
                tool_call_id: child.parent_tool_call_id.clone()?,
                session_id: child.session_id.clone(),
                title: child.label().to_string(),
                agent: child.agent.clone(),
                status: child.status,
                events: child.messages.len(),
                open: open == Some(child.session_id.as_str()),
                waiting: child.pending_input.is_some(),
            })
        })
        .collect()
}

pub(super) fn status_label(status: HaloAgentSubagentStatus) -> &'static str {
    match status {
        HaloAgentSubagentStatus::Running => "RUNNING",
        HaloAgentSubagentStatus::Completed => "DONE",
        HaloAgentSubagentStatus::Interrupted => "STOPPED",
        HaloAgentSubagentStatus::Errored => "FAILED",
    }
}

fn status_color(status: HaloAgentSubagentStatus, cx: &App) -> Hsla {
    let theme = cx.theme();
    match status {
        HaloAgentSubagentStatus::Running => theme.warning,
        HaloAgentSubagentStatus::Completed => theme.success,
        HaloAgentSubagentStatus::Interrupted => theme.muted_foreground,
        HaloAgentSubagentStatus::Errored => theme.danger,
    }
}

fn meta_line(card: &SpawnCard) -> String {
    let mut parts = Vec::new();
    if let Some(agent) = card.agent.as_deref().filter(|a| !a.trim().is_empty()) {
        parts.push(agent.to_string());
    }
    if card.waiting {
        parts.push("needs approval".into());
    }
    let events = card.events;
    parts.push(match events {
        1 => "1 event".into(),
        n => format!("{n} events"),
    });
    parts.join(" · ")
}

/// Parent-transcript card for one delegated child.
pub(super) fn render_spawn_card(part_id: SharedString, card: &SpawnCard, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let border = if card.open {
        theme.accent
    } else {
        theme.border
    };
    let surface = theme.popover;
    let accent = theme.accent;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let chip_bg = status_color(card.status, cx).opacity(0.14);
    let status_color = status_color(card.status, cx);
    let session = card.session_id.clone();
    let title = card.title.clone();
    let meta = meta_line(card);
    let status = status_label(card.status);
    let caret = if card.open {
        PhosphorIcon::CaretDown
    } else {
        PhosphorIcon::CaretRight
    };

    div()
        .id(part_id.clone())
        .debug_selector(move || format!("{part_id}"))
        .w_full()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .px(px(10.))
        .py(px(8.))
        .rounded(theme.radius)
        .border_1()
        .border_color(border)
        .bg(surface)
        .cursor_pointer()
        .hover(move |s| s.bg(accent))
        .on_click(move |_, _, cx| {
            let _ = Bridge::halo_agent_toggle_subagent(&session, cx);
        })
        .child(
            div()
                .flex_none()
                .size(px(24.))
                .rounded(theme.radius_sm())
                .bg(accent)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Phosphor::new(PhosphorIcon::GitBranch)
                        .size(px(13.))
                        .color(foreground),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .w_full()
                        .truncate()
                        .text_sm()
                        .line_height(CARD_TITLE_LINE)
                        .font_weight(FontWeight::NORMAL)
                        .text_color(foreground)
                        .child(title),
                )
                .child(
                    div()
                        .w_full()
                        .truncate()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(muted)
                        .child(meta),
                ),
        )
        .child(
            div()
                .flex_none()
                .px(px(7.))
                .py(px(2.))
                .rounded(px(6.))
                .bg(chip_bg)
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(status_color)
                .child(status),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(Phosphor::new(caret).size(px(12.)).color(muted)),
        )
        .into_any_element()
}

/// Compact tray above the composer listing children that are still working.
pub(super) fn render_subagent_tray(state: &BridgeState, cx: &App) -> AnyElement {
    let children: Vec<&HaloAgentSubagent> = state
        .coder
        .agent_subagents
        .iter()
        .filter(|child| child.is_running() || child.pending_input.is_some())
        .collect();
    if children.is_empty() {
        return Empty.into_any_element();
    }

    let theme = cx.theme();
    let border = theme.border;
    let surface = theme.popover;
    let radius = theme.radius;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let accent = theme.accent;
    let open = state.coder.agent_open_subagent.clone();
    let running = state
        .coder
        .agent_subagents
        .iter()
        .filter(|c| c.is_running())
        .count();

    let mut rows = div().w_full().flex().flex_col().gap(px(2.));
    for child in &children {
        let active = open.as_deref() == Some(child.session_id.as_str());
        let row_bg = if active { accent } else { surface };
        let session = child.session_id.clone();
        let color = status_color(child.status, cx);
        let label = child.label().to_string();
        let detail = match (&child.agent, child.pending_input.is_some()) {
            (Some(agent), true) => format!("{agent} · needs approval"),
            (Some(agent), false) => agent.clone(),
            (None, true) => "needs approval".into(),
            (None, false) => status_label(child.status).to_lowercase(),
        };
        rows = rows.child(
            div()
                .id(SharedString::from(format!(
                    "coder-agent-subagent-tray-{}",
                    child.session_id
                )))
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .py(px(5.))
                .rounded(theme.radius_sm())
                .bg(row_bg)
                .cursor_pointer()
                .hover(move |s| s.bg(accent))
                .on_click(move |_, _, cx| {
                    let _ = Bridge::halo_agent_toggle_subagent(&session, cx);
                })
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::GitBranch)
                                .size(px(12.))
                                .color(color),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .line_height(CARD_TITLE_LINE)
                        .text_color(foreground)
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(if child.pending_input.is_some() {
                            theme.warning
                        } else {
                            muted
                        })
                        .child(detail),
                ),
        );
    }

    let header = if running == 0 {
        format!("Subagents · {}", children.len())
    } else {
        format!("Subagents · {running} running")
    };

    celestia_ui::motion::menu_in_from(
        "coder-agent-subagent-tray-in",
        6.0,
        div()
            .id("coder-agent-subagent-tray")
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
                    .px(px(8.))
                    .py(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .px(px(4.))
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .font_weight(FontWeight::NORMAL)
                            .text_color(muted)
                            .child(header),
                    )
                    .child(rows),
            ),
    )
    .into_any_element()
}

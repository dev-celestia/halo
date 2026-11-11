//! Landing "Continue" / "Up next" panel.
//!
//! Below the composer the empty state lists the newest chats (first prompt +
//! last activity) and the plan steps recent chats left open. Both rows resume
//! a session instead of opening a new one, so the landing page works like the
//! Sessions rail without duplicating its tree.

use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, HaloAgentSession};

use super::history::format_relative_time_ms;

/// Rows per column — the panel is a shortcut, not a second history page.
const MAX_ROWS: usize = 4;

pub(super) fn render_recent_work(state: &BridgeState, cx: &App) -> Option<AnyElement> {
    let theme = cx.theme();

    // Stored rail order is stable on purpose; this panel wants newest first.
    let mut sessions: Vec<&HaloAgentSession> = state
        .coder
        .agent_sessions
        .iter()
        .filter(|session| !session.archived && !session.messages.is_empty())
        .collect();
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let continue_rows: Vec<&HaloAgentSession> = sessions.iter().take(MAX_ROWS).copied().collect();

    // "Up next": open plan steps from the most recently active sessions. A
    // step belongs to the chat that produced it, so the row resumes that chat.
    let mut next_rows: Vec<NextItem> = Vec::new();
    for session in &sessions {
        for todo in &session.todos {
            if todo.done || todo.status == "completed" {
                continue;
            }
            if next_rows.iter().any(|row| row.text == todo.text) {
                continue;
            }
            next_rows.push(NextItem {
                session_id: session.id.clone(),
                text: todo.text.clone(),
                in_progress: todo.status == "inProgress",
                when: format_relative_time_ms(session.updated_at),
            });
            if next_rows.len() == MAX_ROWS {
                break;
            }
        }
        if next_rows.len() == MAX_ROWS {
            break;
        }
    }

    if continue_rows.is_empty() && next_rows.is_empty() {
        return None;
    }

    let muted = theme.muted_foreground;
    let mut columns = div()
        .w_full()
        .flex()
        .flex_row()
        .items_start()
        .gap(px(32.))
        .pt(px(20.));

    if !continue_rows.is_empty() {
        let mut column = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(section_title("Continue", muted));
        for (ix, session) in continue_rows.iter().enumerate() {
            column = column.child(continue_row(ix, session, theme));
        }
        columns = columns.child(column);
    }

    if !next_rows.is_empty() {
        let mut column = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(section_title("Up next", muted));
        for (ix, item) in next_rows.iter().enumerate() {
            column = column.child(next_row(ix, item, theme));
        }
        columns = columns.child(column);
    }

    Some(columns.into_any_element())
}

fn section_title(label: &'static str, muted: Hsla) -> impl IntoElement {
    // No left inset: the heading lines up with the composer's edge while the
    // rows keep their 8px chip inset, like the reference layout.
    div()
        .pb(px(6.))
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(muted)
        .child(label)
}

fn continue_row(
    ix: usize,
    session: &HaloAgentSession,
    theme: &celestia_ui::theme::Theme,
) -> AnyElement {
    let id = session.id.clone();
    let title = session.title_or_default().to_string();
    let title = match session
        .root_path
        .as_deref()
        .map(crate::pages::coder::project_display_name)
    {
        Some(project) => format!("{project} · {title}"),
        None => title,
    };
    let time = format_relative_time_ms(session.updated_at);
    div()
        .id(("agent-continue-row", ix))
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(8.))
        .py(px(6.))
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .hover(|hover| hover.bg(theme.muted.opacity(0.3)))
        .on_click(move |_, _, cx| {
            let _ = Bridge::halo_agent_select_session(&id, cx);
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.foreground)
                .child(title),
        )
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(time),
        )
        .into_any_element()
}

struct NextItem {
    session_id: String,
    text: String,
    in_progress: bool,
    when: String,
}

fn next_row(ix: usize, item: &NextItem, theme: &celestia_ui::theme::Theme) -> AnyElement {
    let session_id = item.session_id.clone();
    let (glyph, color) = if item.in_progress {
        (PhosphorIcon::CircleNotch, theme.primary)
    } else {
        (PhosphorIcon::Circle, theme.muted_foreground)
    };
    div()
        .id(("agent-next-row", ix))
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(8.))
        .py(px(6.))
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .hover(|hover| hover.bg(theme.muted.opacity(0.3)))
        .on_click(move |_, _, cx| {
            let _ = Bridge::halo_agent_select_session(&session_id, cx);
        })
        .child(Phosphor::new(glyph).size(px(13.)).color(color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.foreground)
                .child(item.text.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(item.when.clone()),
        )
        .into_any_element()
}

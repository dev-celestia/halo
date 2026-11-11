//! Message-level rows: render_message dispatch, tool traces, tool parts.

use super::*;

pub(crate) fn render_message(
    msg: &halo_bridge::HaloAgentMessage,
    chat: &ChatKey,
    scope: &PartScope,
    spawns: &[SpawnCard],
    agent_running: bool,
    active: Option<(&str, usize)>,
    fold_process: bool,
    texts: &HashMap<usize, (Entity<TextViewState>, MarkdownExtensions)>,
    _window: &mut Window,
    cx: &App,
) -> impl IntoElement {
    let alignment = match msg.role {
        HaloAgentRole::User => MessageAlignment::End,
        _ => MessageAlignment::Start,
    };
    let message_id = msg.id.clone();
    let muted = cx.theme().muted_foreground;

    let body: AnyElement = if msg.role == HaloAgentRole::User {
        user_turn(msg, chat, &message_id, cx).into_any_element()
    } else {
        let mut body = div().flex().flex_col().gap(BLOCK_GAP).w_full().min_w_0();

        for (part_index, part) in msg.parts.iter().enumerate() {
            let part_id: SharedString = chat.part_id(&message_id, part_index).into();
            let live = is_live_part(active, &message_id, part_index);
            match part {
                HaloAgentPart::Text(text) => match msg.role {
                    HaloAgentRole::System => {
                        body = body.child(notice_text(text.clone(), muted));
                    }
                    HaloAgentRole::Assistant => {
                        let (state, extensions) = &texts[&part_index];
                        body = body.child(reply_text(state, extensions.clone()));
                    }
                    HaloAgentRole::User => {}
                },
                HaloAgentPart::Attachments(paths) => {
                    body = body.child(attachment_chips(paths, part_id, cx));
                }
                HaloAgentPart::Reasoning { text, expanded } => {
                    // The active turn's thoughts fold into the work group with its
                    // tool calls; the group paints them in step order.
                    if fold_process {
                        continue;
                    }
                    let mid = message_id.clone();
                    let scope = scope.clone();
                    let streaming = live && msg.role == HaloAgentRole::Assistant;
                    body = body.child(reasoning_row(
                        part_id.clone(),
                        text,
                        streaming,
                        *expanded,
                        move |_, _, cx| match &scope {
                            PartScope::Root => {
                                let _ = Bridge::halo_agent_toggle_reasoning(&mid, part_index, cx);
                            }
                            PartScope::Subagent(session) => {
                                let _ = Bridge::halo_agent_toggle_subagent_reasoning(
                                    session, &mid, part_index, cx,
                                );
                            }
                        },
                        cx,
                    ));
                }
                HaloAgentPart::Error(text) => {
                    body = body.child(error_card(part_id, text.clone(), agent_running, cx));
                }
                HaloAgentPart::Interrupted => {
                    body = body.child(interrupted_card(part_id, agent_running, cx));
                }
                HaloAgentPart::ModelChanged { from, to } => {
                    body = body.child(model_change_divider(from, to, cx));
                }
                HaloAgentPart::ToolCall {
                    id,
                    label,
                    detail,
                    expanded,
                    is_error,
                    done,
                    sources,
                } => {
                    // A delegation call is the parent face of a child session.
                    if let Some(card) = spawns.iter().find(|card| &card.tool_call_id == id) {
                        if !fold_process {
                            body = body.child(render_spawn_card(part_id.clone(), card, cx));
                        }
                        continue;
                    }
                    // The active turn's process folds under the work-group row;
                    // the trailing group paints it when expanded.
                    if fold_process {
                        continue;
                    }
                    let (state, denied) =
                        tool_trace_state(detail.as_deref(), *is_error, *done, live);
                    let mid = message_id.clone();
                    let tid = id.clone();
                    let scope = scope.clone();
                    body = body.child(render_tool_part(
                        part_id.clone(),
                        label.clone(),
                        detail.clone(),
                        sources.clone(),
                        state,
                        *expanded,
                        denied,
                        move |_, _, cx| match &scope {
                            PartScope::Root => {
                                let _ = Bridge::halo_agent_toggle_tool_chip(&mid, &tid, cx);
                            }
                            PartScope::Subagent(session) => {
                                let _ = Bridge::halo_agent_toggle_subagent_tool_chip(
                                    session, &mid, &tid, cx,
                                );
                            }
                        },
                        cx,
                    ));
                }
            }
        }

        body.into_any_element()
    };

    let mut message = Message::new()
        .alignment(alignment)
        .content(MessageContent::new().child(body));
    // Footer: token usage under assistant replies + whole-message copy.
    // (Hover-reveal via group_hover never painted inside Message's nested tree.)
    let source = message_source(msg);
    let usage = (msg.role == HaloAgentRole::Assistant)
        .then(|| msg.usage.as_ref())
        .flatten()
        .filter(|u| u.input_tokens > 0 || u.output_tokens > 0 || u.used_tokens > 0);
    if source.is_some() || usage.is_some() {
        message = message.footer(message_actions(&message_id, source, usage.cloned(), cx));
    }
    div().w_full().min_w_0().child(message)
}

/// Trace state + denial flag for one tool call, shared by the inline body and
/// the work-group dropdown so both surfaces agree on the glyph.
pub(crate) fn tool_trace_state(
    detail: Option<&str>,
    is_error: bool,
    done: bool,
    live: bool,
) -> (TraceState, bool) {
    let denied = detail.is_some_and(is_approval_denial);
    let state = if denied {
        TraceState::Denied
    } else if is_error {
        TraceState::Failed
    } else if done {
        TraceState::Done
    } else if live {
        TraceState::Running
    } else {
        TraceState::Pending
    };
    (state, denied)
}

/// One tool-call row — the inline transcript body and the turn work group
/// paint identical chips through this helper.
pub(crate) fn render_tool_part(
    part_id: SharedString,
    label: String,
    detail: Option<String>,
    sources: Vec<HaloAgentSource>,
    state: TraceState,
    expanded: bool,
    denied: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    // Denied / failed tools use a local card so the command title can
    // sit between body (14) and meta (12) — quieter than Tool::card.
    let as_card = matches!(
        state,
        TraceState::Denied | TraceState::Failed | TraceState::Error
    );
    if as_card {
        let end = detail.as_deref().map(preview_end);
        let outcome = detail.as_deref().map(|d| {
            let end = end.unwrap_or(d.len());
            d[..end].to_owned()
        });
        let mut extras = Vec::new();
        if let (Some(d), Some(end)) = (detail.as_deref(), end) {
            if end < d.len() {
                extras.push(
                    div()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(muted)
                        .child("Large tool preview limited. Copy includes the complete output.")
                        .into_any_element(),
                );
                extras.push(
                    copy_button(
                        SharedString::from(format!("{part_id}:copy-tool")),
                        d.to_owned(),
                        "Copy tool output",
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }
        tool_status_card(
            part_id, label, state, expanded, outcome, denied, extras, on_toggle, cx,
        )
    } else {
        // Inline chips are step rows too, so they share the group's
        // capture-phase toggle instead of a passive click target.
        process_row(
            part_id, state, label, detail, &sources, true, false, expanded, on_toggle, cx,
        )
    }
}

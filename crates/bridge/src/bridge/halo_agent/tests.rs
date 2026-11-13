use super::super::*;
use super::run::{clear_submitted_draft, drop_failed_user_tail};
use crate::types::BackendEvent;
use crate::work::{
    CoderWork, HaloAgentDelivery, HaloAgentFileChangeKind, HaloAgentPart, HaloAgentPendingInput,
    HaloAgentQueuedMessage, HaloAgentSessionStatus, WorkData, WorkKind, WorkTab,
};
use halo_backend::commands::halo_agent::{AgentEvent, DoneStatus, ToolCall};

fn queued(id: &str, text: &str) -> HaloAgentQueuedMessage {
    HaloAgentQueuedMessage {
        id: id.into(),
        text: text.into(),
        attachments: vec![format!("/tmp/{id}.png")],
        delivery: HaloAgentDelivery::Pending,
    }
}

fn workspace(root: &str, run: Option<&str>, pending: bool) -> (CoderWork, String) {
    let mut work = CoderWork::default();
    work.root_path = Some(root.into());
    work.agent_open = true;
    work.agent_ensure_session();
    let chat = work.agent_active_session_id.clone();
    if let Some(run) = run {
        work.agent_sessions[0].register_run(run.into());
        work.agent_running = true;
        work.agent_run_id = Some(run.into());
    }
    if pending {
        work.agent_sessions[0].pending_input = Some(HaloAgentPendingInput {
            chat_id: chat.clone(),
            run_id: run.unwrap_or("run").into(),
            request_id: "approval".into(),
            subagent: None,
            questions: Vec::new(),
            page: 0,
            picked: Vec::new(),
        });
    }
    work.agent_sessions[0].queue.push(queued("next", "later"));
    work.agent_sessions[0].draft = "keep this edit".into();
    (work, chat)
}

fn two_workspaces() -> (BridgeState, String, String) {
    let (primary, chat_a) = workspace("/tmp/workspace-a", Some("run-a"), false);
    let (secondary, chat_b) = workspace("/tmp/workspace-b", Some("run-b"), true);
    let mut state = BridgeState::default();
    state.active_work_tab_id = Some("tab-a".into());
    state.coder = primary.clone();
    state.work_tabs = vec![
        WorkTab {
            id: "tab-a".into(),
            title: "A".into(),
            kind: WorkKind::Coder,
            session_id: state.active_activity_session_id.clone().unwrap_or_default(),
            data: WorkData::Coder(primary),
        },
        WorkTab {
            id: "tab-b".into(),
            title: "B".into(),
            kind: WorkKind::Coder,
            session_id: state.active_activity_session_id.clone().unwrap_or_default(),
            data: WorkData::Coder(secondary),
        },
    ];
    (state, chat_a, chat_b)
}

fn event(state: &mut BridgeState, run: &str, chat: &str, event: AgentEvent) {
    state.apply_events([BackendEvent::HaloAgentEvent {
        run_id: run.into(),
        chat_id: chat.into(),
        event,
    }]);
}

fn session<'a>(state: &'a BridgeState, chat: &str) -> &'a crate::work::HaloAgentSession {
    state
        .coder_work_for_chat(chat)
        .unwrap()
        .agent_sessions
        .iter()
        .find(|session| session.id == chat)
        .unwrap()
}

#[test]
fn stale_events_and_foreign_chats_do_not_cross() {
    let (mut state, chat_a, chat_b) = two_workspaces();
    let before_b = session(&state, &chat_b).messages.revision();
    event(
        &mut state,
        "run-old",
        &chat_a,
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: Some("stale".into()),
        },
    );
    assert_eq!(session(&state, &chat_a).run_id.as_deref(), Some("run-a"));
    assert!(matches!(
        session(&state, &chat_a).queue[0].delivery,
        HaloAgentDelivery::Pending
    ));
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "hello".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    assert_eq!(session(&state, &chat_b).messages.revision(), before_b);
    assert!(session(&state, &chat_b).pending_input.is_some());
    assert_eq!(session(&state, &chat_b).draft, "keep this edit");
    let text = session(&state, &chat_a)
        .messages
        .iter()
        .flat_map(|message| message.parts.iter())
        .any(|part| matches!(part, HaloAgentPart::Text(text) if text == "hello"));
    assert!(text);
}

#[test]
fn a_completed_turn_drains_the_pending_queue_head() {
    let (mut state, chat_a, chat_b) = two_workspaces();
    // Successful Completed schedules auto-drain of the Pending head. The pump
    // (not apply_events) owns the real dispatch; tests assert the drain list
    // then call prepare_agent_dispatch the same way the pump would.
    event(
        &mut state,
        "run-b",
        &chat_b,
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: Some("session-b".into()),
        },
    );
    assert!(matches!(
        session(&state, &chat_b).queue[0].delivery,
        HaloAgentDelivery::Pending
    ));
    let drains = state.take_agent_queue_drains();
    assert_eq!(drains, vec![(chat_b.clone(), "next".into(), false)]);

    // Duplicate Completed on a settled run is ignored (accepts_run fails), so
    // no second drain is queued while the head is still Pending.
    event(
        &mut state,
        "run-b",
        &chat_b,
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: None,
        },
    );
    assert!(state.take_agent_queue_drains().is_empty());

    let dispatch = state
        .prepare_agent_dispatch(&chat_b, "next", false)
        .unwrap()
        .expect("drain should prepare a run");
    assert!(matches!(dispatch, super::run::AgentDispatch::Run(_)));
    assert!(
        session(&state, &chat_b).queue.is_empty(),
        "a sent prompt leaves the queue"
    );

    // Interrupted holds the queue — no auto-drain.
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::Done {
            status: DoneStatus::Interrupted,
            result: None,
            error: None,
            session_id: None,
        },
    );
    assert!(matches!(
        session(&state, &chat_a).queue[0].delivery,
        HaloAgentDelivery::Pending
    ));
    assert!(state.take_agent_queue_drains().is_empty());

    session_mut(&mut state, &chat_a).register_run("run-a3".into());
    session_mut(&mut state, &chat_a).queue[0].delivery = HaloAgentDelivery::Sending {
        run_id: "run-a3".into(),
        command_id: "cmd".into(),
    };
    event(
        &mut state,
        "run-a3",
        &chat_a,
        AgentEvent::Done {
            status: DoneStatus::Errored,
            result: None,
            error: Some("closed".into()),
            session_id: None,
        },
    );
    // Confirmed failure drops the Sending row so Queue · N shrinks; no drain.
    assert!(session(&state, &chat_a).queue.is_empty());
    assert!(state.take_agent_queue_drains().is_empty());
    assert_eq!(session(&state, &chat_a).draft, "keep this edit");
}

#[test]
fn harness_ready_keeps_followups_queued_until_steer() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::HarnessReady,
    );
    let drains = state.take_agent_queue_drains();
    assert_eq!(drains, vec![(chat_a.clone(), "next".into(), true)]);
    assert!(session(&state, &chat_a).is_working());
    assert!(
        session(&state, &chat_a)
            .queue
            .iter()
            .any(|item| item.id == "next"),
        "the queue panel still holds the follow-up until steer dispatch"
    );
}

#[test]
fn mid_run_submit_steers_into_the_live_turn() {
    // Send while working prepares a Steer on the current run. It does not
    // settle the session or start a second run. Stop still does not drain.
    let (mut state, chat_a, _) = two_workspaces();
    assert!(session(&state, &chat_a).is_working());

    session_mut(&mut state, &chat_a)
        .queue
        .push(queued("follow-up", "write a note"));

    let dispatch = state
        .prepare_agent_dispatch(&chat_a, "follow-up", true)
        .unwrap()
        .expect("mid-run submit should steer the live turn");
    assert!(matches!(dispatch, super::run::AgentDispatch::Steer { .. }));
    assert!(session(&state, &chat_a).is_working());
    assert!(
        session(&state, &chat_a)
            .queue
            .iter()
            .all(|item| item.id != "follow-up"),
        "steered follow-up leaves the queue"
    );
    assert!(
        session(&state, &chat_a)
            .queue
            .iter()
            .any(|item| item.id == "next" && matches!(item.delivery, HaloAgentDelivery::Pending)),
        "older Pending heads stay until Completed drain or explicit Send now"
    );
    assert!(state.take_agent_queue_drains().is_empty());
}

#[test]
fn uncertain_queue_item_can_be_retried_explicitly() {
    let (mut state, chat_a, _) = two_workspaces();
    // Idle session with an Uncertain row and its orphaned optimistic bubble.
    session_mut(&mut state, &chat_a).settle_run(HaloAgentSessionStatus::Idle);
    session_mut(&mut state, &chat_a).queue[0].delivery = HaloAgentDelivery::Uncertain {
        run_id: "run-old".into(),
        command_id: "cmd-old".into(),
        error: "No command acknowledgement received".into(),
    };
    session_mut(&mut state, &chat_a)
        .messages
        .push(crate::work::HaloAgentMessage {
            id: "cmd-old".into(),
            role: crate::work::HaloAgentRole::User,
            parts: vec![HaloAgentPart::Text("later".into())],
            usage: None,
        });

    // Uncertain is never auto-drained — only an explicit retry moves it.
    assert!(matches!(
        session(&state, &chat_a).queue[0].delivery,
        HaloAgentDelivery::Uncertain { .. }
    ));
    session_mut(&mut state, &chat_a).register_run("run-done".into());
    event(
        &mut state,
        "run-done",
        &chat_a,
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: None,
        },
    );
    assert!(state.take_agent_queue_drains().is_empty());
    assert!(matches!(
        session(&state, &chat_a).queue[0].delivery,
        HaloAgentDelivery::Uncertain { .. }
    ));

    let dispatch = state
        .prepare_agent_dispatch(&chat_a, "next", false)
        .unwrap()
        .expect("explicit retry should prepare a fresh run");
    assert!(matches!(dispatch, super::run::AgentDispatch::Run(_)));
    assert!(
        session(&state, &chat_a).queue.is_empty(),
        "a retried prompt leaves the queue once it is in the chat"
    );
    // Orphan bubble removed; a new user message is queued for the retry.
    assert!(session(&state, &chat_a)
        .messages
        .iter()
        .all(|m| m.id != "cmd-old"));
    assert!(session(&state, &chat_a).messages.iter().any(|m| {
        m.role == crate::work::HaloAgentRole::User
            && m.parts
                .iter()
                .any(|p| matches!(p, HaloAgentPart::Text(t) if t == "later"))
    }));
}

#[test]
fn artifacts_wait_for_a_successful_tool_result() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ToolCall {
            id: "write-1".into(),
            call: ToolCall::WriteFile {
                path: "src/lib.rs".into(),
                content: Some("fn main() {}".into()),
            },
        },
    );
    assert!(session(&state, &chat_a).file_changes.is_empty());
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ToolResult {
            id: "write-1".into(),
            is_error: true,
            output: Some("denied".into()),
        },
    );
    assert!(session(&state, &chat_a).file_changes.is_empty());
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ToolCall {
            id: "write-2".into(),
            call: ToolCall::EditFile {
                path: "src/lib.rs".into(),
                old_string: None,
                new_string: Some("ok".into()),
            },
        },
    );
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ToolResult {
            id: "write-2".into(),
            is_error: false,
            output: Some("wrote".into()),
        },
    );
    assert_eq!(session(&state, &chat_a).file_changes.len(), 1);
    assert_eq!(
        session(&state, &chat_a).file_changes[0].kind,
        HaloAgentFileChangeKind::Edit
    );
}

#[test]
fn clear_during_a_run_rejects_later_events_for_that_identity() {
    let (mut state, chat_a, chat_b) = two_workspaces();
    session_mut(&mut state, &chat_a).clear_chat();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "late".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    assert!(session(&state, &chat_a).messages.is_empty());
    assert!(session(&state, &chat_a).run_id.is_none());
    assert_eq!(
        session(&state, &chat_b).status,
        HaloAgentSessionStatus::Working
    );
}

#[test]
fn rejected_delivery_drops_queue_and_keeps_newer_drafts() {
    let (mut state, chat_a, _) = two_workspaces();
    session_mut(&mut state, &chat_a)
        .messages
        .push(crate::work::HaloAgentMessage {
            id: "next".into(),
            role: crate::work::HaloAgentRole::User,
            parts: vec![
                HaloAgentPart::Attachments(vec!["/tmp/next.png".into()]),
                HaloAgentPart::Text("later".into()),
            ],
        usage: None,
        });
    session_mut(&mut state, &chat_a).queue[0].delivery = HaloAgentDelivery::Sending {
        run_id: "run-a".into(),
        command_id: "next".into(),
    };
    session_mut(&mut state, &chat_a).draft = "typed after submit".into();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::CommandRejected {
            command_id: "next".into(),
            error: "Pi rejected steer".into(),
        },
    );
    assert!(
        session(&state, &chat_a).queue.is_empty(),
        "rejected rows leave the queue so the count shrinks"
    );
    // Prompt + images stay on the optimistic bubble for transcript Resend.
    let user = session(&state, &chat_a)
        .messages
        .iter()
        .find(|m| m.id == "next")
        .expect("user bubble kept");
    assert!(user.parts.iter().any(|p| matches!(
        p,
        HaloAgentPart::Attachments(paths) if paths == &["/tmp/next.png".to_string()]
    )));
    assert_eq!(session(&state, &chat_a).draft, "typed after submit");
    let mut draft = "typed after submit".to_string();
    clear_submitted_draft(&mut draft, "original");
    assert_eq!(draft, "typed after submit");
    clear_submitted_draft(&mut draft, "typed after submit");
    assert!(draft.is_empty());
}

#[test]
fn usage_event_stamps_the_latest_assistant_message() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "hello".into(),
            reasoning: false,
            append: true,
            done: true,
        },
    );
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::Usage {
            used_tokens: 12_000,
            window_tokens: 200_000,
            input_tokens: 8_000,
            output_tokens: 400,
        },
    );
    let msg = session(&state, &chat_a)
        .messages
        .iter()
        .rev()
        .find(|m| m.role == crate::work::HaloAgentRole::Assistant)
        .expect("assistant bubble");
    let usage = msg.usage.as_ref().expect("usage stamped on reply");
    assert_eq!(usage.input_tokens, 8_000);
    assert_eq!(usage.output_tokens, 400);
    assert_eq!(usage.used_tokens, 12_000);
    assert_eq!(
        session(&state, &chat_a)
            .context_usage
            .as_ref()
            .map(|u| u.window_tokens),
        Some(200_000)
    );
}

#[test]
fn token_deltas_do_not_require_a_shell_snapshot() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "token".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    let previous = state.clone();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "-2".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    assert!(state.suppresses_shell_refresh(&previous));
    let previous = state.clone();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: None,
        },
    );
    assert!(!state.suppresses_shell_refresh(&previous));
}

#[test]
fn doctor_and_harness_clicks_repaint_during_token_stream() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "token".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    let previous = state.clone();
    state.doctor.confirm_armed = Some("system:agent-state".into());
    assert!(
        !state.suppresses_shell_refresh(&previous),
        "arming a Doctor row must repaint even while tokens are streaming"
    );
    let previous = state.clone();
    state.coder.agent_installing = Some("pi".into());
    assert!(
        !state.suppresses_shell_refresh(&previous),
        "Upgrade must repaint even while tokens are streaming"
    );
    let previous = state.clone();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "-2".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    assert!(state.suppresses_shell_refresh(&previous));
}

#[test]
fn visibility_toggle_paints_during_a_token_stream() {
    use celestia_ui::state::StoreCommit;

    let (mut state, chat_a, _) = two_workspaces();
    state.hold_paint = true;
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "token".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    state.on_store_commit();
    assert_eq!(state.paint_epoch, 0, "a token batch must not paint the shell");

    let shell = state.clone();
    state.hold_paint = true;
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "-2".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    state.on_store_commit();
    assert_eq!(state.paint_epoch, shell.paint_epoch);
    assert!(state.suppresses_shell_refresh(&shell));

    state.coder.agent_context_open = !state.coder.agent_context_open;
    state.on_store_commit();
    assert_ne!(state.paint_epoch, shell.paint_epoch);
    assert!(
        state.suppresses_shell_refresh(&shell),
        "the field allowlist still treats a context toggle as token-only"
    );

    let painted = state.paint_epoch;
    state.hold_paint = true;
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: None,
        },
    );
    state.on_store_commit();
    assert_eq!(
        state.paint_epoch,
        painted + 1,
        "a shell event inside a held batch still paints once"
    );
}

fn session_mut<'a>(
    state: &'a mut BridgeState,
    chat: &str,
) -> &'a mut crate::work::HaloAgentSession {
    state
        .coder_work_for_chat_mut(chat)
        .unwrap()
        .agent_sessions
        .iter_mut()
        .find(|session| session.id == chat)
        .unwrap()
}

#[test]
fn authoritative_text_replaces_a_streamed_prefix() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "draft".into(),
            reasoning: false,
            append: true,
            done: false,
        },
    );
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m".into(),
            content_index: 0,
            text: "final".into(),
            reasoning: false,
            append: false,
            done: true,
        },
    );
    let texts: Vec<_> = session(&state, &chat_a)
        .messages
        .iter()
        .flat_map(|message| message.parts.iter())
        .filter_map(|part| match part {
            HaloAgentPart::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["final"]);
}

#[test]
fn skill_and_mcp_toggles_round_trip_through_the_flat_projection() {
    let (mut work, chat) = workspace("/tmp/workspace-a", None, false);
    work.agent_enabled_skills = vec!["demo".into()];
    work.agent_enabled_mcp = vec!["files".into()];
    work.agent_flush_active_session();

    let stored = work
        .agent_sessions
        .iter()
        .find(|session| session.id == chat)
        .expect("active session");
    assert_eq!(stored.skills, vec!["demo".to_string()]);
    assert_eq!(stored.mcp_servers, vec!["files".to_string()]);

    work.agent_enabled_skills.clear();
    work.agent_enabled_mcp.clear();
    work.agent_refresh_active_session();
    assert_eq!(work.agent_enabled_skills, vec!["demo".to_string()]);
    assert_eq!(work.agent_enabled_mcp, vec!["files".to_string()]);
}

#[test]
fn web_search_results_become_sources_on_the_tool_part() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ToolCall {
            id: "search-1".into(),
            call: ToolCall::WebSearch {
                query: "tokio TcpStream".into(),
            },
        },
    );
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ToolResult {
            id: "search-1".into(),
            is_error: false,
            output: Some(
                "Title: TcpStream in tokio::net - Rust\n\
                 URL: https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html\n\
                 Published: N/A\n\
                 Author: N/A\n\
                 Highlights:\n\
                 A TCP stream between a local and a remote socket.\n"
                    .into(),
            ),
        },
    );

    let search = session(&state, &chat_a)
        .messages
        .iter()
        .flat_map(|message| message.parts.iter())
        .find_map(|part| match part {
            HaloAgentPart::ToolCall { label, sources, .. } if label.starts_with("Web search: ") => {
                Some(sources.clone())
            }
            _ => None,
        })
        .expect("web search part was not recorded");

    assert_eq!(search.len(), 1);
    assert_eq!(search[0].title, "TcpStream in tokio::net - Rust");
    assert!(search[0].url.starts_with("https://docs.rs/"));
}

// ── drop_failed_user_tail ────────────────────────────────────────────────

fn bubble(id: &str) -> crate::work::HaloAgentMessage {
    crate::work::HaloAgentMessage {
        id: id.into(),
        role: crate::work::HaloAgentRole::User,
        parts: vec![HaloAgentPart::Text("hello".into())],
        usage: None,
    }
}

fn reply(id: &str) -> crate::work::HaloAgentMessage {
    crate::work::HaloAgentMessage {
        id: id.into(),
        role: crate::work::HaloAgentRole::Assistant,
        parts: vec![HaloAgentPart::Text("reply".into())],
        usage: None,
    }
}

fn system_row(id: &str, part: HaloAgentPart) -> crate::work::HaloAgentMessage {
    crate::work::HaloAgentMessage {
        id: id.into(),
        role: crate::work::HaloAgentRole::System,
        parts: vec![part],
        usage: None,
    }
}

#[test]
fn a_failed_tail_pops_the_notice_and_the_user_bubble() {
    let mut messages: crate::HaloAgentMessages = vec![
        bubble("a"),
        system_row("e", HaloAgentPart::Error("boom".into())),
    ]
    .into();
    assert!(drop_failed_user_tail(&mut messages));
    assert!(messages.is_empty());
}

#[test]
fn an_interrupted_tail_pops() {
    let mut messages: crate::HaloAgentMessages =
        vec![bubble("a"), system_row("i", HaloAgentPart::Interrupted)].into();
    assert!(drop_failed_user_tail(&mut messages));
    assert!(messages.is_empty());
}

/// The failure came after a reply; stripping the user bubble would delete a
/// message that was answered. Resend must no-op instead.
#[test]
fn an_answered_turn_is_never_popped() {
    let mut messages: crate::HaloAgentMessages = vec![
        bubble("a"),
        reply("r"),
        system_row("e", HaloAgentPart::Error("boom".into())),
    ]
    .into();
    assert!(!drop_failed_user_tail(&mut messages));
    assert_eq!(messages.len(), 3);
}

/// A model-change divider is informational — it does not mark the turn above
/// it as failed, so the user bubble stays.
#[test]
fn a_divider_alone_does_not_mark_a_failed_turn() {
    let mut messages: crate::HaloAgentMessages = vec![
        bubble("a"),
        system_row(
            "d",
            HaloAgentPart::ModelChanged {
                from: "Auto".into(),
                to: "DeepSeek Flash".into(),
            },
        ),
    ]
    .into();
    assert!(!drop_failed_user_tail(&mut messages));
    assert_eq!(messages.len(), 2);
}

/// A divider may sit between the failed bubble and the notice (the user
/// switched models, sent, and the run failed) — the whole failed tail goes.
#[test]
fn a_divider_between_the_bubble_and_the_error_pops_the_tail() {
    let mut messages: crate::HaloAgentMessages = vec![
        bubble("old"),
        system_row(
            "d",
            HaloAgentPart::ModelChanged {
                from: "Auto".into(),
                to: "B".into(),
            },
        ),
        bubble("new"),
        system_row("e", HaloAgentPart::Error("boom".into())),
    ]
    .into();
    assert!(drop_failed_user_tail(&mut messages));
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "old");
    assert_eq!(messages[1].id, "d");
}

/// An unanswered prompt with no notice (e.g. the app died before any event)
/// is exactly what Resend is for.
#[test]
fn an_unanswered_bubble_with_no_notice_still_resends() {
    let mut messages: crate::HaloAgentMessages = vec![bubble("a")].into();
    assert!(drop_failed_user_tail(&mut messages));
    assert!(messages.is_empty());
}

/// A lone error row has no user turn under it; popping would eat the notice.
#[test]
fn a_lone_error_notice_is_left_alone() {
    let mut messages: crate::HaloAgentMessages =
        vec![system_row("e", HaloAgentPart::Error("boom".into()))].into();
    assert!(!drop_failed_user_tail(&mut messages));
    assert_eq!(messages.len(), 1);
}

// ── steer transport failure recovery ─────────────────────────────────────

/// A steer that never reached the harness must not kill the run: the bubble
/// is undone and the prompt rejoins the queue head as Pending.
#[test]
fn a_failed_steer_returns_to_the_queue_and_the_run_survives() {
    let (mut state, chat_a, _) = two_workspaces();
    // The optimistic steer bubble sits in the transcript with its queue row
    // already removed — the state a failing halo_agent_steer finds.
    session_mut(&mut state, &chat_a).messages.push(crate::work::HaloAgentMessage {
        id: "cmd-1".into(),
        role: crate::work::HaloAgentRole::User,
        parts: vec![HaloAgentPart::Text("write a note".into())],
        usage: None,
    });

    state.requeue_failed_steer(&chat_a, "cmd-1", "write a note", vec!["/tmp/note.png".into()]);

    let s = session(&state, &chat_a);
    assert!(
        s.messages.iter().all(|m| m.id != "cmd-1"),
        "the optimistic bubble is removed"
    );
    assert_eq!(s.queue.len(), 2, "the prompt rejoins the existing queue head");
    assert_eq!(s.queue[0].text, "write a note");
    assert_eq!(s.queue[0].attachments, vec!["/tmp/note.png".to_string()]);
    assert!(matches!(s.queue[0].delivery, HaloAgentDelivery::Pending));
    // The live run is untouched — no Errored settle, no Error notice.
    assert_eq!(s.run_id.as_deref(), Some("run-a"));
    assert!(s.is_working());
    assert!(s.messages.iter().all(|m| {
        !m.parts.iter().any(|p| matches!(p, HaloAgentPart::Error(_)))
    }));
    // The tray (flat projection) sees the row again.
    assert!(state
        .coder
        .agent_queue
        .iter()
        .any(|item| item.text == "write a note"));
}

/// The backend acks a steer and OpenCode/Pi ack it natively too — the
/// duplicate must retain nothing further, remove nothing else, and leave the
/// live run untouched.
#[test]
fn a_harness_side_accepted_is_a_no_op_after_the_synthesized_one() {
    let (mut state, chat_a, _) = two_workspaces();
    session_mut(&mut state, &chat_a).queue[0].delivery = HaloAgentDelivery::Sending {
        run_id: "run-a".into(),
        command_id: "cmd-9".into(),
    };
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::CommandAccepted {
            command_id: "cmd-9".into(),
        },
    );
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::CommandAccepted {
            command_id: "cmd-9".into(),
        },
    );
    assert!(
        session(&state, &chat_a).queue.is_empty(),
        "the acked row left the queue"
    );
    assert!(
        session(&state, &chat_a).is_working(),
        "the live run survives a duplicate ack"
    );
}

// ── steered turn boundary ────────────────────────────────────────────────

/// A steered follow-up is a new turn. Harnesses with session-scoped response
/// ids (OpenCode / Antigravity) keep streaming under the SAME message id, so
/// the boundary must also drop the live row maps — otherwise the reply glues
/// onto the previous turn's text and the user's message looks ignored.
#[test]
fn a_steered_followup_starts_a_new_bubble() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m1".into(),
            content_index: 0,
            text: "feel free to give me a task to run!".into(),
            reasoning: false,
            append: true,
            done: true,
        },
    );
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::CommandAccepted {
            command_id: "cmd-9".into(),
        },
    );
    event(&mut state, "run-a", &chat_a, AgentEvent::Steered);
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m1".into(),
            content_index: 0,
            text: "hellow world".into(),
            reasoning: false,
            append: true,
            done: true,
        },
    );

    let texts: Vec<_> = session(&state, &chat_a)
        .messages
        .iter()
        .flat_map(|message| message.parts.iter())
        .filter_map(|part| match part {
            HaloAgentPart::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        ["feel free to give me a task to run!", "hellow world"],
        "the steered reply must not glue onto the previous turn"
    );
}

/// The backend acks the boundary and Cursor self-reports it — a duplicate
/// Steered reuses the empty row instead of stacking a blank bubble.
#[test]
fn a_duplicate_steered_does_not_stack_empty_bubbles() {
    let (mut state, chat_a, _) = two_workspaces();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::ContentUpdated {
            message_id: "m1".into(),
            content_index: 0,
            text: "first".into(),
            reasoning: false,
            append: true,
            done: true,
        },
    );
    let before = session(&state, &chat_a).messages.len();
    event(&mut state, "run-a", &chat_a, AgentEvent::Steered);
    event(&mut state, "run-a", &chat_a, AgentEvent::Steered);
    assert_eq!(
        session(&state, &chat_a).messages.len(),
        before + 1,
        "two boundaries with no text between collapse into one row"
    );
}

#[test]
fn session_started_then_error_drops_empty_assistant_row_and_allows_resend() {
    let (mut state, chat_a, _) = two_workspaces();
    session_mut(&mut state, &chat_a).messages = vec![HaloAgentMessage {
        id: "u1".into(),
        role: HaloAgentRole::User,
        parts: vec![HaloAgentPart::Text("fix the bug".into())],
        usage: None,
    }]
    .into();
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::SessionStarted {
            harness: "opencode".into(),
            model: "anthropic/claude-sonnet-4-5".into(),
            session_id: "ses_abc123".into(),
            tools: Vec::new(),
            cwd: "/tmp".into(),
        },
    );
    event(
        &mut state,
        "run-a",
        &chat_a,
        AgentEvent::Error {
            message: "rate limited".into(),
        },
    );
    let sess = session_mut(&mut state, &chat_a);
    assert_eq!(sess.harness_session_id.as_deref(), Some("ses_abc123"));
    assert_eq!(sess.harness_session_owner.as_deref(), Some("opencode"));
    assert!(
        drop_failed_user_tail(&mut sess.messages),
        "Resend must succeed even when SessionStarted pre-allocated an empty assistant row before the error"
    );
    assert!(sess.messages.is_empty());
}

#[test]
fn switching_harnesses_mid_chat_clears_foreign_resume_and_attaches_fallback_context() {
    let (mut state, chat_a, _) = two_workspaces();
    {
        let sess = session_mut(&mut state, &chat_a);
        sess.settle_run(HaloAgentSessionStatus::Idle);
        sess.set_harness_session("opencode", "ses_opencode_99".into());
        sess.messages = vec![
            HaloAgentMessage::new(
                "u1",
                HaloAgentRole::User,
                vec![HaloAgentPart::Text("Where is main.rs?".into())],
            ),
            HaloAgentMessage::new(
                "a1",
                HaloAgentRole::Assistant,
                vec![HaloAgentPart::Text("It is in src/main.rs.".into())],
            ),
        ]
        .into();
        sess.queue.push(HaloAgentQueuedMessage {
            id: "q-next".into(),
            text: "Now add a CLI flag to it.".into(),
            attachments: Vec::new(),
            delivery: HaloAgentDelivery::Pending,
        });
    }
    // Switch the workspace harness from opencode to cursor mid-chat.
    state
        .coder_work_for_chat_mut(&chat_a)
        .unwrap()
        .agent_harness = "cursor".into();

    let Some(super::run::AgentDispatch::Run(req)) = state
        .prepare_agent_dispatch(&chat_a, "q-next", false)
        .expect("dispatch succeeds")
    else {
        panic!("expected a Run dispatch");
    };
    assert_eq!(
        req.harness,
        halo_backend::commands::halo_agent::HarnessId::Cursor
    );
    assert!(
        req.resume.is_none(),
        "foreign OpenCode session ID must not be passed to Cursor"
    );
    let ctx = req
        .fallback_context
        .expect("prior conversation context should be synthesized");
    assert!(ctx.contains("Where is main.rs?"));
    assert!(ctx.contains("It is in src/main.rs."));
    assert!(session(&state, &chat_a).harness_session_id.is_none());
}

#[test]
fn same_harness_resume_retains_session_id_and_fallback_context() {
    let (mut state, chat_a, _) = two_workspaces();
    {
        let sess = session_mut(&mut state, &chat_a);
        sess.settle_run(HaloAgentSessionStatus::Idle);
        sess.set_harness_session("opencode", "ses_opencode_99".into());
        sess.messages = vec![
            HaloAgentMessage::new(
                "u1",
                HaloAgentRole::User,
                vec![HaloAgentPart::Text("Hello".into())],
            ),
            HaloAgentMessage::new(
                "a1",
                HaloAgentRole::Assistant,
                vec![HaloAgentPart::Text("Hi there!".into())],
            ),
        ]
        .into();
        sess.queue.push(HaloAgentQueuedMessage {
            id: "q-next".into(),
            text: "Follow up".into(),
            attachments: Vec::new(),
            delivery: HaloAgentDelivery::Pending,
        });
    }
    state
        .coder_work_for_chat_mut(&chat_a)
        .unwrap()
        .agent_harness = "opencode".into();

    let Some(super::run::AgentDispatch::Run(req)) = state
        .prepare_agent_dispatch(&chat_a, "q-next", false)
        .expect("dispatch succeeds")
    else {
        panic!("expected a Run dispatch");
    };
    assert_eq!(req.resume.as_deref(), Some("ses_opencode_99"));
    assert!(req.fallback_context.is_some());
}

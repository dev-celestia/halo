//! Encode / decode coding-agent sessions for `halo.db`.

use super::super::*;
use halo_backend::{HaloAgentMessageRecord, HaloAgentSessionRecord};

use crate::work::{
    CoderWork, HaloAgentDelivery, HaloAgentFileChange, HaloAgentMessage, HaloAgentQueuedMessage,
    HaloAgentRole, HaloAgentSession, HaloAgentSessionStatus, HaloAgentSubagent,
    HaloAgentSubagentStatus, HaloAgentTodo,
};

/// One delegated child transcript inside `subagents_json`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubagentRecord {
    session_id: String,
    #[serde(default)]
    parent_tool_call_id: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    agent: Option<String>,
    /// `running` | `completed` | `interrupted` | `errored`
    #[serde(default)]
    status: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    messages: Vec<HaloAgentMessage>,
    #[serde(default)]
    todos: Vec<HaloAgentTodo>,
}

fn subagent_status_str(status: HaloAgentSubagentStatus) -> &'static str {
    match status {
        HaloAgentSubagentStatus::Running => "running",
        HaloAgentSubagentStatus::Completed => "completed",
        HaloAgentSubagentStatus::Interrupted => "interrupted",
        HaloAgentSubagentStatus::Errored => "errored",
    }
}

fn subagent_to_record(child: &HaloAgentSubagent) -> SubagentRecord {
    SubagentRecord {
        session_id: child.session_id.clone(),
        parent_tool_call_id: child.parent_tool_call_id.clone(),
        title: child.title.clone(),
        agent: child.agent.clone(),
        status: subagent_status_str(child.status).into(),
        error: child.error.clone(),
        messages: child.messages.iter().map(|m| (**m).clone()).collect(),
        todos: child.todos.clone(),
    }
}

fn sanitize_restored_messages(messages: &mut Vec<HaloAgentMessage>) {
    for message in messages.iter_mut() {
        for part in message.parts.iter_mut() {
            if let HaloAgentPart::ToolCall { done, is_error, .. } = part {
                if !*done {
                    *done = true;
                    *is_error = true;
                }
            }
        }
    }
    while messages
        .last()
        .is_some_and(|m| super::reducer::is_empty_assistant_message(m))
    {
        messages.pop();
    }
}

fn encode_harness_session(owner: Option<&str>, id: Option<&str>) -> Option<String> {
    let id = id?.trim();
    if id.is_empty() {
        return None;
    }
    match owner.map(str::trim).filter(|o| !o.is_empty()) {
        Some(owner) => Some(format!("{owner}::{id}")),
        None => Some(id.to_string()),
    }
}

fn decode_harness_session(raw: Option<String>) -> (Option<String>, Option<String>) {
    let Some(raw) = raw else {
        return (None, None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    if let Some((owner, id)) = trimmed.split_once("::") {
        let owner = owner.trim().to_ascii_lowercase();
        let id = id.trim();
        if matches!(owner.as_str(), "pi" | "opencode" | "antigravity" | "cursor")
            && !id.is_empty()
        {
            return (Some(owner), Some(id.to_string()));
        }
    }
    (None, Some(trimmed.to_string()))
}

fn record_to_subagent(mut record: SubagentRecord) -> HaloAgentSubagent {
    let mut child = HaloAgentSubagent::new(record.session_id, record.title, record.agent);
    child.parent_tool_call_id = record.parent_tool_call_id;
    // A restored chat has no live process; Running children can never resume.
    child.status = match record.status.as_str() {
        "completed" => HaloAgentSubagentStatus::Completed,
        "errored" => HaloAgentSubagentStatus::Errored,
        _ => HaloAgentSubagentStatus::Interrupted,
    };
    child.error = record.error;
    sanitize_restored_messages(&mut record.messages);
    child.messages = record.messages.into();
    child.todos = record.todos;
    child
}

fn json_or_empty<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[]".into())
}

fn parse_json_vec<T: serde::de::DeserializeOwned>(raw: &str) -> Vec<T> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str(raw).unwrap_or_default()
}

fn role_to_str(role: HaloAgentRole) -> &'static str {
    match role {
        HaloAgentRole::User => "user",
        HaloAgentRole::Assistant => "assistant",
        HaloAgentRole::System => "system",
    }
}

fn role_from_str(role: &str) -> Option<HaloAgentRole> {
    match role {
        "user" => Some(HaloAgentRole::User),
        "assistant" => Some(HaloAgentRole::Assistant),
        "system" => Some(HaloAgentRole::System),
        _ => None,
    }
}

/// Skip brand-new empty slots so accidental New Chat does not fill the DB.
pub(crate) fn session_worth_persisting(session: &HaloAgentSession) -> bool {
    session.archived
        || !session.messages.is_empty()
        || !session.draft.trim().is_empty()
        || !session.attachments.is_empty()
        || !session.todos.is_empty()
        || !session.file_changes.is_empty()
        || !session.queue.is_empty()
        || !session.subagents.is_empty()
        || !session.skills.is_empty()
        || !session.mcp_servers.is_empty()
        || session.harness_session_id.is_some()
        || !session.title.trim().is_empty()
}

pub(crate) fn session_to_record(session: &HaloAgentSession) -> Option<HaloAgentSessionRecord> {
    if !session_worth_persisting(session) {
        return None;
    }
    let messages = session
        .messages
        .iter()
        .map(|message| HaloAgentMessageRecord {
            id: message.id.clone(),
            role: role_to_str(message.role).into(),
            parts_json: json_or_empty(&message.parts),
            usage_json: message
                .usage
                .as_ref()
                .map(json_or_empty)
                .unwrap_or_default(),
        })
        .collect();
    Some(HaloAgentSessionRecord {
        id: session.id.clone(),
        title: session.title.clone(),
        preview: session.preview.clone(),
        updated_at: session.updated_at,
        archived: session.archived,
        root_path: session.root_path.clone(),
        harness_session_id: encode_harness_session(
            session.harness_session_owner.as_deref(),
            session.harness_session_id.as_deref(),
        ),
        model: session.model.clone(),
        plan_mode: session.plan_mode,
        approval_mode: session.approval_mode.id().into(),
        draft: session.draft.clone(),
        attachments_json: json_or_empty(&session.attachments),
        todos_json: json_or_empty(&session.todos),
        file_changes_json: json_or_empty(&session.file_changes),
        queue_json: json_or_empty(&session.queue),
        todo_expanded: session.todo_expanded,
        todo_dismissed: session.todo_dismissed,
        subagents_json: json_or_empty(
            &session
                .subagents
                .iter()
                .map(subagent_to_record)
                .collect::<Vec<_>>(),
        ),
        skills_json: json_or_empty(&session.skills),
        mcp_servers_json: json_or_empty(&session.mcp_servers),
        messages,
    })
}

pub(crate) fn record_to_session(record: HaloAgentSessionRecord) -> HaloAgentSession {
    let mut session = HaloAgentSession::new_empty();
    session.id = record.id;
    session.title = record.title;
    session.preview = record.preview;
    session.updated_at = record.updated_at;
    session.archived = record.archived;
    session.root_path = record.root_path;
    let (harness_session_owner, harness_session_id) =
        decode_harness_session(record.harness_session_id);
    session.harness_session_owner = harness_session_owner;
    session.harness_session_id = harness_session_id;
    session.model = record.model;
    session.plan_mode = record.plan_mode;
    session.approval_mode = crate::AgentApprovalMode::parse(&record.approval_mode)
        .unwrap_or(crate::AgentApprovalMode::Ask);
    session.draft = record.draft;
    session.attachments = parse_json_vec(&record.attachments_json);
    session.todos = parse_json_vec::<HaloAgentTodo>(&record.todos_json);
    session.file_changes = parse_json_vec::<HaloAgentFileChange>(&record.file_changes_json);
    session.todo_expanded = record.todo_expanded;
    session.todo_dismissed = record.todo_dismissed;
    session.subagents = parse_json_vec::<SubagentRecord>(&record.subagents_json)
        .into_iter()
        .map(record_to_subagent)
        .collect();
    session.skills = parse_json_vec(&record.skills_json);
    session.mcp_servers = parse_json_vec(&record.mcp_servers_json);
    // Never restore a live run — process is gone after restart.
    session.status = HaloAgentSessionStatus::Idle;
    session.run_id = None;
    session.streaming_part = None;
    session.pending_input = None;
    session.completion = None;
    session.live = Default::default();

    let mut queue: Vec<HaloAgentQueuedMessage> = parse_json_vec(&record.queue_json);
    for item in &mut queue {
        if let HaloAgentDelivery::Sending { run_id, command_id } = &item.delivery {
            item.delivery = HaloAgentDelivery::Uncertain {
                run_id: run_id.clone(),
                command_id: command_id.clone(),
                error: "App restarted before delivery was acknowledged.".into(),
            };
        }
    }
    session.queue = queue;

    let mut messages = Vec::with_capacity(record.messages.len());
    for message in record.messages {
        let Some(role) = role_from_str(&message.role) else {
            continue;
        };
        let parts = parse_json_vec(&message.parts_json);
        let usage = if message.usage_json.trim().is_empty() {
            None
        } else {
            serde_json::from_str(&message.usage_json).ok()
        };
        messages.push(HaloAgentMessage {
            id: message.id,
            role,
            parts,
            usage,
        });
    }
    sanitize_restored_messages(&mut messages);
    session.messages = messages.into();
    session
}

pub(crate) fn apply_loaded_sessions(work: &mut CoderWork, records: Vec<HaloAgentSessionRecord>) {
    if records.is_empty() {
        return;
    }
    let sessions: Vec<HaloAgentSession> = records.into_iter().map(record_to_session).collect();
    // Prefer the newest non-archived chat as the active panel.
    let active = sessions
        .iter()
        .filter(|s| !s.archived)
        .max_by_key(|s| s.updated_at)
        .map(|s| s.id.clone())
        .or_else(|| sessions.first().map(|s| s.id.clone()));
    if let Some(id) = active {
        work.agent_sessions = sessions;
        work.agent_active_session_id = id;
        work.agent_load_active_session();
    } else {
        work.agent_sessions = sessions;
    }
}

impl Bridge {
    /// Load durable chats from SQLite into the flat coder projection (boot).
    pub(crate) fn halo_agent_load_persisted(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let Ok(records) = backend.halo_agent_list_sessions() else {
            return;
        };
        if records.is_empty() {
            return;
        }
        Self::state(cx).update(cx, |s| {
            apply_loaded_sessions(&mut s.coder, records);
            s.sync_agent_into_coder_tabs();
        });
    }

    /// Write one in-memory session to SQLite (no-op for empty brand-new slots).
    #[allow(dead_code)] // sync path kept for tests / emergency flushes
    pub(crate) fn halo_agent_persist_session(session: &HaloAgentSession, cx: &App) {
        let Some(record) = session_to_record(session) else {
            return;
        };
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let _ = backend.halo_agent_upsert_session(&record);
    }

    /// Persist one session without blocking the UI on serde + SQLite.
    pub(crate) fn halo_agent_persist_session_async(session: HaloAgentSession, cx: &App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        Self::run_blocking(
            cx,
            move || {
                if let Some(record) = session_to_record(&session) {
                    let _ = backend.halo_agent_upsert_session(&record);
                }
                Ok(())
            },
            |_, _| {},
        );
    }

    /// Persist specific chat ids. `idle_only` skips in-flight Working chats
    /// (streaming batches); pass false after submit so the user turn is durable.
    /// Prefer [`Self::halo_agent_persist_chats_async`] from UI paths — this sync
    /// form is for tests and shutdown flushes that already own the thread.
    #[allow(dead_code)]
    pub(crate) fn halo_agent_persist_chats(chat_ids: &[String], idle_only: bool, cx: &App) {
        if chat_ids.is_empty() {
            return;
        }
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let records: Vec<_> = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .filter(|s| chat_ids.iter().any(|id| id == &s.id))
            .filter(|s| !idle_only || s.status != HaloAgentSessionStatus::Working)
            .filter_map(session_to_record)
            .collect();
        for record in records {
            let _ = backend.halo_agent_upsert_session(&record);
        }
    }

    /// Clone matching sessions on the UI thread (Arc message rows), then
    /// serialize + SQLite upsert on a worker so agent Done / submit never
    /// stalls clicks, theme toggle, or layout.
    pub(crate) fn halo_agent_persist_chats_async(chat_ids: &[String], idle_only: bool, cx: &App) {
        if chat_ids.is_empty() {
            return;
        }
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let sessions: Vec<HaloAgentSession> = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .filter(|s| chat_ids.iter().any(|id| id == &s.id))
            .filter(|s| !idle_only || s.status != HaloAgentSessionStatus::Working)
            .cloned()
            .collect();
        if sessions.is_empty() {
            return;
        }
        Self::run_blocking(
            cx,
            move || {
                for session in sessions {
                    if let Some(record) = session_to_record(&session) {
                        let _ = backend.halo_agent_upsert_session(&record);
                    }
                }
                Ok(())
            },
            |_, _| {},
        );
    }

    /// Flush the live panel and persist every durable chat (quit / project switch).
    pub(crate) fn halo_agent_persist_all(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_flush_active_session();
        });
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let records: Vec<_> = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .filter_map(session_to_record)
            .collect();
        for record in records {
            let _ = backend.halo_agent_upsert_session(&record);
        }
    }

    pub(crate) fn halo_agent_delete_persisted(id: &str, cx: &App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let _ = backend.halo_agent_delete_persisted_session(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::{HaloAgentMessage, HaloAgentPart, HaloAgentRole};

    #[test]
    fn session_round_trip_keeps_transcript_and_drops_live_run() {
        let mut session = HaloAgentSession::new_for_project(Some("/tmp/proj".into()));
        session.title = "Hello".into();
        session.preview = "hi".into();
        session.updated_at = 42;
        session.harness_session_id = Some("/tmp/pi.jsonl".into());
        session.status = HaloAgentSessionStatus::Working;
        session.run_id = Some("run-1".into());
        session.messages = vec![HaloAgentMessage {
            id: "m1".into(),
            role: HaloAgentRole::User,
            parts: vec![HaloAgentPart::Text("hi".into())],
            usage: None,
        }]
        .into();
        session.queue.push(HaloAgentQueuedMessage {
            id: "q1".into(),
            text: "later".into(),
            attachments: Vec::new(),
            delivery: HaloAgentDelivery::Sending {
                run_id: "run-1".into(),
                command_id: "cmd-1".into(),
            },
        });
        session.skills = vec!["demo".into()];
        session.mcp_servers = vec!["files".into()];

        let record = session_to_record(&session).expect("worth persisting");
        let restored = record_to_session(record);
        assert_eq!(restored.title, "Hello");
        assert_eq!(restored.skills, vec!["demo".to_string()]);
        assert_eq!(restored.mcp_servers, vec!["files".to_string()]);
        assert_eq!(
            restored.harness_session_id.as_deref(),
            Some("/tmp/pi.jsonl")
        );
        assert_eq!(restored.status, HaloAgentSessionStatus::Idle);
        assert!(restored.run_id.is_none());
        assert_eq!(restored.messages.len(), 1);
        assert!(matches!(
            restored.messages[0].parts[0],
            HaloAgentPart::Text(ref t) if t == "hi"
        ));
        assert!(matches!(
            restored.queue[0].delivery,
            HaloAgentDelivery::Uncertain { .. }
        ));
    }

    #[test]
    fn empty_new_session_is_not_persisted() {
        let session = HaloAgentSession::new_empty();
        assert!(session_to_record(&session).is_none());
    }

    /// A model-change divider has to survive a restart, or the transcript
    /// would quietly lose the record of which model answered which turn.
    #[test]
    fn a_model_change_notice_survives_persistence() {
        let mut session = HaloAgentSession::new_for_project(Some("/tmp/proj".into()));
        session.messages = vec![HaloAgentMessage {
            id: "m1".into(),
            role: HaloAgentRole::System,
            parts: vec![HaloAgentPart::ModelChanged {
                from: "Auto".into(),
                to: "DeepSeek Flash".into(),
            }],
            usage: None,
        }]
        .into();

        let record = session_to_record(&session).expect("worth persisting");
        let restored = record_to_session(record);
        assert_eq!(
            restored.messages[0].parts,
            vec![HaloAgentPart::ModelChanged {
                from: "Auto".into(),
                to: "DeepSeek Flash".into(),
            }]
        );
    }

    #[test]
    fn toggle_only_session_is_persisted() {
        let mut session = HaloAgentSession::new_empty();
        session.skills = vec!["demo".into()];
        assert!(session_to_record(&session).is_some());
    }

    #[test]
    fn restored_session_preserves_harness_owner_and_marks_unfinished_tools_done() {
        let mut session = HaloAgentSession::new_for_project(Some("/tmp/proj".into()));
        session.set_harness_session("antigravity", "agy-thread-7".into());
        session.messages = vec![
            HaloAgentMessage {
                id: "m1".into(),
                role: HaloAgentRole::User,
                parts: vec![HaloAgentPart::Text("run tests".into())],
                usage: None,
            },
            HaloAgentMessage {
                id: "m2".into(),
                role: HaloAgentRole::Assistant,
                parts: vec![HaloAgentPart::ToolCall {
                    id: "tc1".into(),
                    label: "cargo test".into(),
                    detail: None,
                    expanded: false,
                    is_error: false,
                    done: false,
                    sources: Vec::new(),
                }],
                usage: None,
            },
            HaloAgentMessage {
                id: "m3".into(),
                role: HaloAgentRole::Assistant,
                parts: Vec::new(),
                usage: None,
            },
        ]
        .into();

        let record = session_to_record(&session).expect("worth persisting");
        assert_eq!(
            record.harness_session_id.as_deref(),
            Some("antigravity::agy-thread-7")
        );
        let restored = record_to_session(record);
        assert_eq!(
            restored.harness_session_owner.as_deref(),
            Some("antigravity")
        );
        assert_eq!(
            restored.harness_session_id.as_deref(),
            Some("agy-thread-7")
        );
        assert_eq!(
            restored.messages.len(),
            2,
            "trailing empty assistant row is stripped on restore"
        );
        assert!(matches!(
            restored.messages[1].parts[0],
            HaloAgentPart::ToolCall {
                done: true,
                is_error: true,
                ..
            }
        ));
    }
}

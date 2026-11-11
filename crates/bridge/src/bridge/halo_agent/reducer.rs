//! Session-local reduction. No foreground swapping and no unregistered events.
use super::super::*;
use crate::work::{
    HaloAgentContextUsage, HaloAgentDelivery, HaloAgentFileChange, HaloAgentFileChangeKind,
    HaloAgentInputQuestion, HaloAgentMessage, HaloAgentPart, HaloAgentPendingInput, HaloAgentRole,
    HaloAgentRunState, HaloAgentSession, HaloAgentSubagent, HaloAgentSubagentStatus, HaloAgentTodo,
};

impl BridgeState {
    /// Chat UUIDs are globally unique; an inactive flat projection is NOT an owner.
    /// Flat `coder` projection owns agent sessions while Agent (or Coder) is
    /// the active work tab — Agent leaves that projection in place on load.
    fn agent_flat_is_live(&self) -> bool {
        matches!(
            self.active_work_kind(),
            Some(WorkKind::Agent | WorkKind::Coder)
        )
    }

    pub(crate) fn coder_work_for_chat(&self, chat_id: &str) -> Option<&CoderWork> {
        if self.agent_flat_is_live() && self.coder.agent_sessions.iter().any(|s| s.id == chat_id) {
            return Some(&self.coder);
        }
        self.work_tabs
            .iter()
            .filter(|t| Some(&t.id) != self.active_work_tab_id.as_ref())
            .find_map(|tab| match &tab.data {
                WorkData::Coder(work) if work.agent_sessions.iter().any(|s| s.id == chat_id) => {
                    Some(work)
                }
                _ => None,
            })
    }

    pub(crate) fn coder_work_for_chat_mut(&mut self, chat_id: &str) -> Option<&mut CoderWork> {
        if self.agent_flat_is_live() && self.coder.agent_sessions.iter().any(|s| s.id == chat_id) {
            return Some(&mut self.coder);
        }
        self.work_tabs
            .iter_mut()
            .filter(|t| Some(&t.id) != self.active_work_tab_id.as_ref())
            .find_map(|tab| match &mut tab.data {
                WorkData::Coder(work) if work.agent_sessions.iter().any(|s| s.id == chat_id) => {
                    Some(work)
                }
                _ => None,
            })
    }

    pub(crate) fn reduce_agent_event(
        &mut self,
        run_id: &str,
        chat_id: &str,
        event: AgentEvent,
        first: bool,
    ) -> bool {
        let (drain, antigravity_session_started, steer_ids) = {
            let Some(work) = self.coder_work_for_chat_mut(chat_id) else {
                return false;
            };
            let Some(session) = work.agent_sessions.iter_mut().find(|s| s.id == chat_id) else {
                return false;
            };
            // Check identity BEFORE title/model/session caches, input, tools, or Done.
            if !session.accepts_run(run_id) {
                return false;
            }
            if first {
                session.messages.begin_batch();
            }
            // Only a clean Completed turn may auto-send the next queued prompt.
            // Errored / Interrupted leave the queue held for an explicit Send now.
            let consider_drain = matches!(
                &event,
                AgentEvent::Done {
                    status: DoneStatus::Completed,
                    error: None,
                    ..
                }
            );
            let harness_ready = matches!(&event, AgentEvent::HarnessReady);
            let antigravity_session_started = work.agent_harness == "antigravity"
                && matches!(&event, AgentEvent::SessionStarted { .. });
            session.reduce(run_id, event);
            let drain = if consider_drain
                && !session.archived
                && session.status == crate::work::HaloAgentSessionStatus::Idle
            {
                session.queue.first().and_then(|item| {
                    matches!(item.delivery, HaloAgentDelivery::Pending)
                        .then(|| (chat_id.to_string(), item.id.clone(), false))
                })
            } else if harness_ready && session.is_working() {
                None
            } else {
                None
            };
            let steer_ids = if harness_ready && session.is_working() {
                session
                    .queue
                    .iter()
                    .filter(|item| matches!(item.delivery, HaloAgentDelivery::Pending))
                    .map(|item| item.id.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            (drain, antigravity_session_started, steer_ids)
        };
        // Session start means Google auth already succeeded for this process —
        // clear the stale "Sign in" card without waiting for a keychain re-probe.
        if antigravity_session_started {
            self.antigravity_acp_signed_in = true;
            self.antigravity_acp_login = crate::work::AntigravityLoginStatus::Idle;
            if self.coder.agent_harness == "antigravity" && self.coder.agent_harness_installed {
                self.coder.agent_provider_sheet_open = false;
                self.coder.agent_provider_sheet_dismissed = false;
            }
        }
        if let Some((chat, item, steer)) = drain {
            self.push_agent_queue_drain(chat, item, steer);
        }
        for item in steer_ids {
            self.push_agent_queue_drain(chat_id.to_string(), item, true);
        }
        true
    }

    /// Queue a post-Done auto-dispatch. Dedupes so a duplicate Completed frame
    /// cannot schedule the same Pending row twice before the pump takes it.
    pub(crate) fn push_agent_queue_drain(
        &mut self,
        chat_id: String,
        item_id: String,
        steer: bool,
    ) {
        if self
            .agent_queue_drains
            .iter()
            .any(|(c, i, _)| c == &chat_id && i == &item_id)
        {
            return;
        }
        self.agent_queue_drains.push((chat_id, item_id, steer));
    }

    /// Drain candidates recorded during the last apply batch. The event pump
    /// dispatches each pair via `halo_agent_dispatch_queue_item`.
    /// The bool is true when the item must steer the live turn.
    pub(crate) fn take_agent_queue_drains(&mut self) -> Vec<(String, String, bool)> {
        std::mem::take(&mut self.agent_queue_drains)
    }

    /// Refresh live flat fields for `chats`. When `sync_tabs` is true (shell /
    /// structural changes), also mirror sessions into matching Coder work tabs.
    /// Token deltas pass `sync_tabs = false` to avoid cloning every session on
    /// every streamed chunk.
    pub(crate) fn finish_agent_batch(&mut self, chats: &[String], sync_tabs: bool) {
        let mut live_changed = false;
        for chat in chats {
            let live = self.agent_flat_is_live()
                && self.coder.agent_sessions.iter().any(|s| &s.id == chat);
            if live {
                live_changed = true;
            } else if let Some(work) = self.coder_work_for_chat_mut(chat) {
                work.agent_refresh_active_session();
            }
        }
        if live_changed {
            self.coder.agent_refresh_active_session();
            // Only the agent slice is committed on streaming batches, not open
            // editor buffers, terminals, the tree, and the rest of CoderWork.
            if sync_tabs {
                self.sync_agent_into_coder_tabs();
            }
        }
    }

    /// Push agent session fields from the flat `coder` projection into matching
    /// Coder work tabs (same project root). Used while Agent is the active page.
    ///
    /// Callers that mutate flat fields (`agent_queue`, messages, …) must
    /// [`CoderWork::agent_flush_active_session`] first so sessions are current.
    pub(crate) fn sync_agent_into_coder_tabs(&mut self) {
        let root = self.coder.root_path.clone();
        let sessions = self.coder.agent_sessions.clone();
        let active_id = self.coder.agent_active_session_id.clone();
        for tab in &mut self.work_tabs {
            if let WorkData::Coder(work) = &mut tab.data {
                if work.root_path == root {
                    work.agent_sessions = sessions.clone();
                    work.agent_active_session_id = active_id.clone();
                    work.agent_refresh_active_session();
                }
            }
        }
    }

    /// A narrow subscription key; read the transcript by borrow when this changes.
    pub fn agent_transcript_revision(&self, chat_id: &str) -> Option<u64> {
        let work = self.coder_work_for_chat(chat_id)?;
        Some(
            work.agent_sessions
                .iter()
                .find(|s| s.id == chat_id)?
                .messages
                .revision(),
        )
    }

    /// Narrow revision key for one child transcript (drawer scrolling).
    pub fn agent_subagent_revision(&self, chat_id: &str, session_id: &str) -> Option<u64> {
        let work = self.coder_work_for_chat(chat_id)?;
        Some(
            work.agent_sessions
                .iter()
                .find(|s| s.id == chat_id)?
                .subagents
                .iter()
                .find(|c| c.session_id == session_id)?
                .messages
                .revision(),
        )
    }
}

impl HaloAgentSession {
    pub(crate) fn accepts_run(&self, run_id: &str) -> bool {
        !run_id.is_empty()
            && !self.archived
            && self.run_id.as_deref() == Some(run_id)
            && self.status == HaloAgentSessionStatus::Working
    }

    pub(crate) fn register_run(&mut self, run_id: String) {
        self.settle_run(HaloAgentSessionStatus::Idle);
        self.live = Default::default();
        self.run_id = Some(run_id);
        self.status = HaloAgentSessionStatus::Working;
        self.run_started_at = Some(crate::work::agent_now_ms());
        self.last_worked_secs = None;
    }

    pub(crate) fn settle_run(&mut self, status: HaloAgentSessionStatus) {
        if let Some(started) = self.run_started_at.take() {
            let elapsed_ms = crate::work::agent_now_ms().saturating_sub(started);
            self.last_worked_secs = Some((elapsed_ms / 1000).max(1));
        }
        self.status = status;
        self.run_id = None;
        self.streaming_part = None;
        self.pending_input = None;
        // Unfinished tools must not look live after stop/error (or a missing result).
        for (_, (row, part, _)) in self.live.tools.drain() {
            if let Some(HaloAgentPart::ToolCall { done, is_error, .. }) = self
                .messages
                .get_mut(row)
                .and_then(|m| m.parts.get_mut(part))
            {
                *done = true;
                *is_error = true;
            }
        }
        self.live.content.clear();
        self.live.responses.clear();
        // Delegated children die with the root run; never leave them spinning.
        for child in &mut self.subagents {
            if child.is_running() {
                child.finish(DoneStatus::Interrupted, None);
            }
        }
        for item in &mut self.queue {
            if let HaloAgentDelivery::Sending { run_id, command_id } = &item.delivery {
                item.delivery = HaloAgentDelivery::Uncertain {
                    run_id: run_id.clone(), command_id: command_id.clone(),
                    error: "No command acknowledgement received; delivery is uncertain. Do not resend without checking the transcript.".into(),
                };
            }
        }
    }

    pub(crate) fn clear_chat(&mut self) {
        self.settle_run(HaloAgentSessionStatus::Idle);
        self.messages.clear();
        self.subagents.clear();
        self.todos.clear();
        self.queue.clear();
        self.file_changes.clear();
        self.skills.clear();
        self.mcp_servers.clear();
        self.attachments.clear();
        self.draft.clear();
        self.completion = None;
        self.title.clear();
        self.preview.clear();
        self.harness_session_id = None;
        self.todo_dismissed = false;
        self.todo_expanded = true;
        self.last_worked_secs = None;
        self.run_started_at = None;
    }

    pub(crate) fn approval_matches(&self, run_id: &str, request_id: &str) -> bool {
        self.accepts_run(run_id)
            && (self.pending_input.as_ref().is_some_and(|p| {
                p.chat_id == self.id && p.run_id == run_id && p.request_id == request_id
            }) || self.subagents.iter().any(|child| {
                child.pending_input.as_ref().is_some_and(|p| {
                    p.chat_id == self.id && p.run_id == run_id && p.request_id == request_id
                })
            }))
    }

    fn assistant_row(&mut self) -> usize {
        tx_assistant_row(&mut self.messages)
    }

    pub(crate) fn system_part(&mut self, part: HaloAgentPart) {
        tx_system_part(&mut self.messages, part);
    }

    fn legacy_text(&mut self, text: String, reasoning: bool) {
        tx_legacy_text(
            &mut self.messages,
            &mut self.streaming_part,
            text,
            reasoning,
        );
    }

    fn content_updated(
        &mut self,
        message_id: String,
        content_index: usize,
        text: String,
        reasoning: bool,
        append: bool,
        done: bool,
    ) {
        tx_content_updated(
            &mut self.messages,
            &mut self.live,
            &mut self.streaming_part,
            message_id,
            content_index,
            text,
            reasoning,
            append,
            done,
        );
    }

    fn tool_call(&mut self, id: String, call: ToolCall) {
        let todos = tx_tool_call(
            &mut self.messages,
            &mut self.live,
            &mut self.streaming_part,
            &mut self.todos,
            id,
            call,
        );
        if let Some(any_pending) = todos {
            self.todo_dismissed = false;
            if any_pending {
                self.todo_expanded = true;
            }
        }
    }

    fn tool_result(&mut self, id: String, is_error: bool, output: Option<String>) {
        tx_tool_result(
            &mut self.messages,
            &mut self.live,
            &mut self.streaming_part,
            &mut self.file_changes,
            id,
            is_error,
            output,
        );
    }

    fn reduce(&mut self, run_id: &str, event: AgentEvent) {
        match event {
            AgentEvent::SessionStarted {
                model, session_id, ..
            } => {
                if !model.is_empty() {
                    // Cursor SDK may report `default` for Auto — normalize so the
                    // composer chip does not flip off the catalog selection.
                    self.model = Some(halo_backend::commands::halo_agent::migrate_model_id(
                        &model,
                    ));
                }
                if !session_id.is_empty() {
                    self.harness_session_id = Some(session_id);
                }
                self.assistant_row();
            }
            AgentEvent::TextDelta { text } => self.legacy_text(text, false),
            AgentEvent::ReasoningDelta { text } => self.legacy_text(text, true),
            AgentEvent::ContentUpdated {
                message_id,
                content_index,
                text,
                reasoning,
                append,
                done,
            } => self.content_updated(message_id, content_index, text, reasoning, append, done),
            AgentEvent::ToolCall { id, call } => self.tool_call(id, call),
            AgentEvent::ToolResult {
                id,
                is_error,
                output,
            } => self.tool_result(id, is_error, output),
            AgentEvent::HarnessReady => {}
            AgentEvent::CommandAccepted { command_id } => {
                self.queue.retain(|item| !matches!(&item.delivery, HaloAgentDelivery::Sending { run_id: r, command_id: c } if r == run_id && c == &command_id));
            }
            AgentEvent::CommandRejected { command_id, error } => {
                // Confirmed negative ack — drop the queue row so the tray count
                // shrinks; the optimistic user bubble + Error notice keep the
                // prompt for an explicit Resend from the transcript.
                // Do not settle here: a following Error/Done carries the
                // authoritative failure message and must still be admitted.
                let matched = self.queue.iter().any(|item| {
                    matches!(
                        &item.delivery,
                        HaloAgentDelivery::Sending { run_id: r, command_id: c }
                            if r == run_id && c == &command_id
                    )
                });
                if matched {
                    self.queue.retain(|item| {
                        !matches!(
                            &item.delivery,
                            HaloAgentDelivery::Sending { run_id: r, command_id: c }
                                if r == run_id && c == &command_id
                        )
                    });
                    // Primary-run rejects are followed by Error/Done with the
                    // real failure text; only steer rejects need a notice here.
                    if command_id != run_id {
                        self.system_part(HaloAgentPart::Error(error));
                    }
                }
            }
            AgentEvent::InputRequested {
                request_id,
                questions,
            } => {
                self.streaming_part = None;
                let n = questions.len();
                self.pending_input = Some(HaloAgentPendingInput {
                    chat_id: self.id.clone(),
                    run_id: run_id.to_string(),
                    request_id,
                    subagent: None,
                    questions: questions
                        .into_iter()
                        .map(|q| HaloAgentInputQuestion {
                            id: q.id,
                            header: q.header,
                            question: q.question,
                            options: q.options,
                            multi_select: q.multi_select,
                        })
                        .collect(),
                    page: 0,
                    picked: vec![Vec::new(); n],
                });
            }
            AgentEvent::InputResolved { request_id } => {
                if self
                    .pending_input
                    .as_ref()
                    .is_some_and(|p| p.request_id == request_id)
                {
                    self.pending_input = None;
                }
                for child in &mut self.subagents {
                    if child
                        .pending_input
                        .as_ref()
                        .is_some_and(|p| p.request_id == request_id)
                    {
                        child.pending_input = None;
                    }
                }
            }
            AgentEvent::SubagentStarted {
                parent_tool_call_id,
                session_id,
                title,
                agent,
            } => self.subagent_started(parent_tool_call_id, session_id, title, agent),
            AgentEvent::SubagentBound {
                session_id,
                parent_tool_call_id,
            } => {
                if let Some(child) = self
                    .subagents
                    .iter_mut()
                    .find(|s| s.session_id == session_id)
                {
                    if child.parent_tool_call_id.is_none() {
                        child.parent_tool_call_id = Some(parent_tool_call_id);
                    }
                }
            }
            AgentEvent::SubagentEvent { session_id, event } => {
                let chat_id = self.id.clone();
                let child = self.subagent_or_start(&session_id);
                child.reduce(run_id, *event);
                if let Some(pending) = &mut child.pending_input {
                    pending.chat_id = chat_id;
                }
                // Child file writes surface in the parent Artifacts rail.
                let changes = std::mem::take(&mut child.file_changes);
                for change in changes {
                    if let Some(old) = self.file_changes.iter_mut().find(|c| c.path == change.path)
                    {
                        *old = change;
                    } else {
                        self.file_changes.push(change);
                    }
                }
            }
            AgentEvent::SubagentDone {
                session_id,
                status,
                error,
            } => {
                if let Some(child) = self
                    .subagents
                    .iter_mut()
                    .find(|s| s.session_id == session_id)
                {
                    child.finish(status, error);
                }
            }
            AgentEvent::Steered => {
                self.streaming_part = None;
                // The steered reply is a new response block. Drop the live
                // text maps so a harness that keeps streaming under the
                // previous message id (session-scoped response ids) starts a
                // fresh row instead of appending into the pre-steer text.
                // In-flight tools stay mapped: their results still belong to
                // rows from before the boundary.
                self.live.content.clear();
                self.live.responses.clear();
                // One boundary per steer — a duplicate (Cursor self-reports
                // and the backend acks) reuses the empty row instead of
                // stacking a blank bubble.
                if !self
                    .messages
                    .last()
                    .is_some_and(|m| m.role == HaloAgentRole::Assistant && m.parts.is_empty())
                {
                    self.messages.push(HaloAgentMessage::new(
                        uuid::Uuid::new_v4().to_string(),
                        HaloAgentRole::Assistant,
                        Vec::new(),
                    ));
                }
            }
            AgentEvent::Usage {
                used_tokens,
                window_tokens,
                input_tokens,
                output_tokens,
            } => {
                let usage = HaloAgentContextUsage {
                    used_tokens,
                    window_tokens,
                    input_tokens,
                    output_tokens,
                };
                if window_tokens > 0 {
                    self.context_usage = Some(usage.clone());
                }
                // Stamp the turn onto the latest assistant bubble so the
                // transcript can show tokens under that response.
                if usage.input_tokens > 0 || usage.output_tokens > 0 || usage.used_tokens > 0 {
                    if let Some(row) = self
                        .messages
                        .iter()
                        .rposition(|m| m.role == HaloAgentRole::Assistant)
                    {
                        if let Some(msg) = self.messages.get_mut(row) {
                            msg.usage = Some(usage);
                        }
                    }
                }
            }
            AgentEvent::Done {
                status,
                error,
                session_id,
                ..
            } => {
                let failed = status == DoneStatus::Errored || error.is_some();
                // Confirmed failure: drop this run's Sending rows so Queue · N
                // shrinks. (Error may already have removed them.) Uncertain is
                // reserved for Interrupted / missing-ack when failure is unknown.
                if failed {
                    self.queue.retain(|item| {
                        !matches!(
                            &item.delivery,
                            HaloAgentDelivery::Sending { run_id: r, .. } if r == run_id
                        )
                    });
                }
                let next = if failed {
                    HaloAgentSessionStatus::Errored
                } else {
                    HaloAgentSessionStatus::Idle
                };
                self.settle_run(next);
                if let Some(sid) = session_id.filter(|s| !s.is_empty()) {
                    self.harness_session_id = Some(sid);
                }
                if let Some(error) = error {
                    self.system_part(HaloAgentPart::Error(error));
                } else if status == DoneStatus::Interrupted {
                    self.system_part(HaloAgentPart::Interrupted);
                }
                // Auto-drain is scheduled by BridgeState::reduce_agent_event
                // only for Completed + Pending head; Errored / Interrupted hold.
            }
            AgentEvent::Error { message } => {
                // Transport / host failures before CommandAccepted did not
                // deliver the prompt. Drop Sending rows so the queue count
                // shrinks; transcript Resend requeues from the user bubble.
                // Do not promote to Uncertain — delivery did not happen.
                self.queue
                    .retain(|item| !matches!(&item.delivery, HaloAgentDelivery::Sending { .. }));
                self.settle_run(HaloAgentSessionStatus::Errored);
                self.system_part(HaloAgentPart::Error(message));
            }
        }
        if self.title.is_empty() {
            self.title = crate::work::agent_session_title_from_messages(&self.messages);
        }
        self.preview = crate::work::agent_session_preview_from_messages(&self.messages);
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
    }
}

/// Shared transcript primitives: the root session and every child agent reduce
/// through these, so children can never grow a divergent message model.
fn tx_assistant_row(messages: &mut crate::HaloAgentMessages) -> usize {
    if messages
        .last()
        .is_none_or(|m| m.role != HaloAgentRole::Assistant)
    {
        messages.push(HaloAgentMessage::new(
            uuid::Uuid::new_v4().to_string(),
            HaloAgentRole::Assistant,
            Vec::new(),
        ));
    }
    messages.len() - 1
}

fn tx_system_part(messages: &mut crate::HaloAgentMessages, part: HaloAgentPart) {
    if messages
        .last()
        .is_some_and(|m| m.role == HaloAgentRole::System && m.parts.as_slice() == [part.clone()])
    {
        return;
    }
    messages.push(HaloAgentMessage::new(
        uuid::Uuid::new_v4().to_string(),
        HaloAgentRole::System,
        vec![part],
    ));
}

fn tx_legacy_text(
    messages: &mut crate::HaloAgentMessages,
    streaming: &mut Option<(String, usize)>,
    text: String,
    reasoning: bool,
) {
    if text.is_empty() {
        return;
    }
    let row = tx_assistant_row(messages);
    let msg = messages.get_mut(row).unwrap();
    match (reasoning, msg.parts.last_mut()) {
        (true, Some(HaloAgentPart::Reasoning { text: old, .. }))
        | (false, Some(HaloAgentPart::Text(old))) => old.push_str(&text),
        (true, _) => msg.parts.push(HaloAgentPart::Reasoning {
            text,
            expanded: false,
        }),
        (false, _) => msg.parts.push(HaloAgentPart::Text(text)),
    }
    *streaming = Some((msg.id.clone(), msg.parts.len() - 1));
}

#[allow(clippy::too_many_arguments)]
fn tx_content_updated(
    messages: &mut crate::HaloAgentMessages,
    live: &mut HaloAgentRunState,
    streaming: &mut Option<(String, usize)>,
    message_id: String,
    content_index: usize,
    text: String,
    reasoning: bool,
    append: bool,
    done: bool,
) {
    let key = (message_id.clone(), content_index);
    let (row, part) = if let Some(position) = live.content.get(&key) {
        *position
    } else {
        let row = if let Some(&row) = live.responses.get(&message_id) {
            row
        } else {
            // SessionStarted may have allocated the empty first response.
            let row = if messages
                .last()
                .is_some_and(|m| m.role == HaloAgentRole::Assistant && m.parts.is_empty())
            {
                messages.len() - 1
            } else {
                messages.push(HaloAgentMessage::new(
                    uuid::Uuid::new_v4().to_string(),
                    HaloAgentRole::Assistant,
                    Vec::new(),
                ));
                messages.len() - 1
            };
            live.responses.insert(message_id, row);
            row
        };
        let msg = messages.get_mut(row).unwrap();
        let part = msg.parts.len();
        msg.parts.push(if reasoning {
            HaloAgentPart::Reasoning {
                text: String::new(),
                expanded: false,
            }
        } else {
            HaloAgentPart::Text(String::new())
        });
        live.content.insert(key, (row, part));
        (row, part)
    };
    let msg = messages.get_mut(row).unwrap();
    match &mut msg.parts[part] {
        HaloAgentPart::Text(old) | HaloAgentPart::Reasoning { text: old, .. } => {
            if append {
                old.push_str(&text);
            } else {
                *old = text;
            }
        }
        _ => return,
    }
    let marker = (msg.id.clone(), part);
    if !done {
        *streaming = Some(marker);
    } else if streaming.as_ref() == Some(&marker) {
        *streaming = None;
    }
}

/// Returns `Some(has_unfinished_items)` when this call replaced the todo list.
#[allow(clippy::too_many_arguments)]
fn tx_tool_call(
    messages: &mut crate::HaloAgentMessages,
    live: &mut HaloAgentRunState,
    streaming: &mut Option<(String, usize)>,
    todos: &mut Vec<HaloAgentTodo>,
    id: String,
    call: ToolCall,
) -> Option<bool> {
    *streaming = None;
    if live.tools.contains_key(&id) {
        return None;
    }
    let mut installed_todos = None;
    if let ToolCall::Todo { items } = &call {
        *todos = items
            .iter()
            .map(|t| HaloAgentTodo {
                text: t.text.clone(),
                done: t.done,
                status: t.effective_status().to_string(),
            })
            .collect();
        installed_todos = Some(todos.iter().any(|t| !t.done));
    }
    let change = call.file_change().map(|(path, kind)| HaloAgentFileChange {
        path: path.to_string(),
        kind: match kind {
            halo_backend::commands::halo_agent::FileChangeKind::Write => {
                HaloAgentFileChangeKind::Write
            }
            halo_backend::commands::halo_agent::FileChangeKind::Edit => {
                HaloAgentFileChangeKind::Edit
            }
        },
    });
    let detail = match &call {
        ToolCall::Exec { command } => Some(command.clone()),
        ToolCall::WriteFile { content, .. } => content.clone(),
        ToolCall::EditFile {
            old_string,
            new_string,
            ..
        } => {
            let mut detail = String::new();
            if let Some(old) = old_string {
                detail.push_str("- ");
                detail.push_str(old);
                detail.push('\n');
            }
            if let Some(new) = new_string {
                detail.push_str("+ ");
                detail.push_str(new);
            }
            if detail.is_empty() {
                None
            } else {
                Some(detail)
            }
        }
        ToolCall::Question {
            question, options, ..
        } => {
            // Readable prompt + choices. The harness fails these calls anyway
            // (no answer channel), so the text is the whole story.
            let mut detail = question.trim().to_string();
            for (ix, option) in options.iter().enumerate() {
                if !detail.is_empty() {
                    detail.push('\n');
                }
                detail.push_str(&format!("{}. {option}", ix + 1));
            }
            if detail.is_empty() {
                None
            } else {
                Some(detail)
            }
        }
        ToolCall::Unknown { input_json, .. } => input_json.clone(),
        _ => None,
    };
    let row = tx_assistant_row(messages);
    let msg = messages.get_mut(row).unwrap();
    let part = msg.parts.len();
    msg.parts.push(HaloAgentPart::ToolCall {
        id: id.clone(),
        label: call.label(),
        detail,
        expanded: false,
        is_error: false,
        done: false,
        sources: Vec::new(),
    });
    live.tools.insert(id, (row, part, change));
    installed_todos
}

fn tx_tool_result(
    messages: &mut crate::HaloAgentMessages,
    live: &mut HaloAgentRunState,
    streaming: &mut Option<(String, usize)>,
    file_changes: &mut Vec<HaloAgentFileChange>,
    id: String,
    is_error: bool,
    output: Option<String>,
) {
    *streaming = None;
    let Some((row, part, change)) = live.tools.remove(&id) else {
        return;
    };
    if let Some(HaloAgentPart::ToolCall {
        done,
        is_error: err,
        detail,
        expanded,
        sources,
        ..
    }) = messages.get_mut(row).and_then(|m| m.parts.get_mut(part))
    {
        *done = true;
        *err = is_error;
        if let Some(output) = output {
            *sources = super::sources::parse_search_sources(&output);
            *detail = Some(output);
        }
        // Open failed / denied tools so the outcome card is visible without a click.
        if is_error {
            *expanded = true;
        }
    }
    if !is_error {
        if let Some(change) = change {
            if let Some(old) = file_changes.iter_mut().find(|c| c.path == change.path) {
                *old = change;
            } else {
                file_changes.push(change);
            }
        }
    }
}

impl HaloAgentSession {
    fn subagent_started(
        &mut self,
        parent_tool_call_id: Option<String>,
        session_id: String,
        title: String,
        agent: Option<String>,
    ) {
        if let Some(existing) = self
            .subagents
            .iter_mut()
            .find(|s| s.session_id == session_id)
        {
            if !title.trim().is_empty()
                && (existing.title.trim().is_empty() || existing.title == "Subagent")
            {
                existing.title = title;
            }
            if existing.agent.is_none() {
                existing.agent = agent;
            }
            if existing.parent_tool_call_id.is_none() {
                existing.parent_tool_call_id = parent_tool_call_id;
            }
            return;
        }
        let mut child = HaloAgentSubagent::new(session_id, title, agent);
        child.parent_tool_call_id = parent_tool_call_id;
        self.subagents.push(child);
    }

    /// Find a child by session id, creating a placeholder when a child event
    /// arrives before `SubagentStarted`.
    fn subagent_or_start(&mut self, session_id: &str) -> &mut HaloAgentSubagent {
        if let Some(ix) = self
            .subagents
            .iter()
            .position(|s| s.session_id == session_id)
        {
            return &mut self.subagents[ix];
        }
        self.subagents.push(HaloAgentSubagent::new(
            session_id.to_string(),
            String::new(),
            None,
        ));
        self.subagents.last_mut().unwrap()
    }
}

impl HaloAgentSubagent {
    fn reduce(&mut self, run_id: &str, event: AgentEvent) {
        match event {
            AgentEvent::TextDelta { text } => {
                tx_legacy_text(&mut self.messages, &mut self.streaming_part, text, false)
            }
            AgentEvent::ReasoningDelta { text } => {
                tx_legacy_text(&mut self.messages, &mut self.streaming_part, text, true)
            }
            AgentEvent::ContentUpdated {
                message_id,
                content_index,
                text,
                reasoning,
                append,
                done,
            } => tx_content_updated(
                &mut self.messages,
                &mut self.live,
                &mut self.streaming_part,
                message_id,
                content_index,
                text,
                reasoning,
                append,
                done,
            ),
            AgentEvent::ToolCall { id, call } => {
                tx_tool_call(
                    &mut self.messages,
                    &mut self.live,
                    &mut self.streaming_part,
                    &mut self.todos,
                    id,
                    call,
                );
            }
            AgentEvent::ToolResult {
                id,
                is_error,
                output,
            } => tx_tool_result(
                &mut self.messages,
                &mut self.live,
                &mut self.streaming_part,
                &mut self.file_changes,
                id,
                is_error,
                output,
            ),
            AgentEvent::InputRequested {
                request_id,
                questions,
            } => {
                self.streaming_part = None;
                let n = questions.len();
                let label = self.label().to_string();
                self.pending_input = Some(HaloAgentPendingInput {
                    chat_id: String::new(),
                    run_id: run_id.to_string(),
                    request_id,
                    subagent: Some(label),
                    questions: questions
                        .into_iter()
                        .map(|q| HaloAgentInputQuestion {
                            id: q.id,
                            header: q.header,
                            question: q.question,
                            options: q.options,
                            multi_select: q.multi_select,
                        })
                        .collect(),
                    page: 0,
                    picked: vec![Vec::new(); n],
                });
            }
            AgentEvent::InputResolved { request_id } => {
                if self
                    .pending_input
                    .as_ref()
                    .is_some_and(|p| p.request_id == request_id)
                {
                    self.pending_input = None;
                }
            }
            AgentEvent::Error { message } => {
                self.streaming_part = None;
                tx_system_part(&mut self.messages, HaloAgentPart::Error(message));
            }
            // Defensive: the harness reports child completion as SubagentDone.
            AgentEvent::Done { status, error, .. } => self.finish(status, error),
            _ => {}
        }
    }

    /// Close this child's live run and never leave a tool chip spinning.
    fn finish(&mut self, status: DoneStatus, error: Option<String>) {
        self.status = match status {
            DoneStatus::Completed => HaloAgentSubagentStatus::Completed,
            DoneStatus::Interrupted => HaloAgentSubagentStatus::Interrupted,
            DoneStatus::Errored => HaloAgentSubagentStatus::Errored,
        };
        self.streaming_part = None;
        self.pending_input = None;
        for (_, (row, part, _)) in self.live.tools.drain() {
            if let Some(HaloAgentPart::ToolCall { done, is_error, .. }) = self
                .messages
                .get_mut(row)
                .and_then(|m| m.parts.get_mut(part))
            {
                *done = true;
                *is_error = true;
            }
        }
        self.live.content.clear();
        self.live.responses.clear();
        if let Some(err) = error {
            self.error = Some(err.clone());
            tx_system_part(&mut self.messages, HaloAgentPart::Error(err));
        }
    }
}

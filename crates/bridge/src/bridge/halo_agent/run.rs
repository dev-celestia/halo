//! Agent run lifecycle. Identity and delivery records precede backend dispatch.
use super::super::*;
use crate::work::HaloAgentDelivery;
use halo_backend::commands::halo_agent::{AgentLaunchContext, HarnessId, RunRequest};

/// Drop a draft only when it is still the text that was just submitted.
pub(crate) fn clear_submitted_draft(draft: &mut String, prompt: &str) {
    if draft.trim() == prompt {
        draft.clear();
    }
}

/// Pop trailing system Error/Interrupted notices and the user turn under them
/// so an explicit Resend does not stack duplicate bubbles. Returns `false`
/// without touching anything when the tail is not a failed user turn — the
/// last user message was already answered (`[User][Assistant][Error]`), or
/// only a ModelChanged divider trails it.
///
/// A model-change notice counts as part of the tail: it is a system row that
/// can sit between the failed turn and the user bubble (the user switched
/// models, sent, and the run failed). Leaving it in place would stop the
/// walk before the user row, and the Resend would stack a second bubble.
pub(crate) fn drop_failed_user_tail(messages: &mut crate::HaloAgentMessages) -> bool {
    let Some(mut walk) = messages.len().checked_sub(1) else {
        return false;
    };
    let mut saw_failure = false;
    let mut saw_system = false;
    while messages[walk].role == HaloAgentRole::System {
        let row = &messages[walk];
        let is_tail_notice = row.parts.iter().all(|p| {
            matches!(
                p,
                HaloAgentPart::Error(_)
                    | HaloAgentPart::Interrupted
                    | HaloAgentPart::ModelChanged { .. }
            )
        });
        if !is_tail_notice {
            return false;
        }
        saw_failure |= row
            .parts
            .iter()
            .any(|p| matches!(p, HaloAgentPart::Error(_) | HaloAgentPart::Interrupted));
        saw_system = true;
        if walk == 0 {
            return false;
        }
        walk -= 1;
    }
    if messages[walk].role != HaloAgentRole::User {
        return false;
    }
    // Notices without a failure (a bare ModelChanged divider) do not mark an
    // unanswered turn; stripping the user row then would be theft, not cleanup.
    if saw_system && !saw_failure {
        return false;
    }
    while messages.len() > walk {
        messages.pop();
    }
    true
}

fn user_prompt_parts(msg: &HaloAgentMessage) -> (String, Vec<String>) {
    let mut prompt = String::new();
    let mut attachments = Vec::new();
    for part in &msg.parts {
        match part {
            HaloAgentPart::Text(text) => {
                if !prompt.is_empty() {
                    prompt.push('\n');
                }
                prompt.push_str(text);
            }
            HaloAgentPart::Attachments(paths) => attachments.extend(paths.iter().cloned()),
            _ => {}
        }
    }
    (prompt, attachments)
}

pub(super) enum AgentDispatch {
    Run(RunRequest),
    Steer {
        chat_id: String,
        run_id: String,
        command_id: String,
        text: String,
        attachments: Vec<String>,
    },
}

impl BridgeState {
    /// Prepare a command transaction without doing I/O. Used by foreground and
    /// background queue sends alike; never changes the selected chat/workspace.
    pub(super) fn prepare_agent_dispatch(
        &mut self,
        chat_id: &str,
        item_id: &str,
        steer: bool,
    ) -> BridgeResult<Option<AgentDispatch>> {
        let mcp_servers = self.coder_settings.mcp_servers.clone();
        let computer_use = self.coder_settings.computer_use;
        let computer_use_headed = self.coder_settings.computer_use_headed;
        let Some(work) = self.coder_work_for_chat_mut(chat_id) else {
            return Ok(None);
        };
        let root = work
            .root_path
            .clone()
            .ok_or_else(|| BridgeError::message("Open a folder before using the agent"))?;
        let harness = HarnessId::parse(&work.agent_harness)
            .ok_or_else(|| BridgeError::message("Unsupported agent harness"))?;
        let model = work
            .agent_model
            .as_deref()
            .map(halo_backend::commands::halo_agent::migrate_model_id);
        if model.as_ref() != work.agent_model.as_ref() {
            work.agent_model = model.clone();
        }
        // Resolve per-chat tool selections before borrowing the session: the
        // catalog lives on the work tab and MCP servers on global settings.
        let run_skills =
            super::skills::resolve_run_skills(&work.agent_skills, &work.agent_enabled_skills);
        let run_mcp = if harness == HarnessId::OpenCode {
            super::skills::resolve_run_mcp(&mcp_servers, &work.agent_enabled_mcp)
        } else {
            Vec::new()
        };
        let session = work
            .agent_sessions
            .iter_mut()
            .find(|s| s.id == chat_id)
            .unwrap();
        if let Some(migrated) = model.as_ref() {
            if session.model.as_ref() != Some(migrated) {
                session.model = Some(migrated.clone());
            }
        }
        if session.archived || (session.is_working() && !steer) {
            return Ok(None);
        }
        let Some(index) = session.queue.iter().position(|q| q.id == item_id) else {
            return Ok(None);
        };
        // Never double-fire an in-flight Sending row. Uncertain / Rejected are
        // different: acknowledgement was never correlated (or was denied), so
        // an explicit Send Now may retry after dropping the optimistic bubble.
        if matches!(
            session.queue[index].delivery,
            HaloAgentDelivery::Sending { .. }
        ) {
            return Ok(None);
        }
        match &session.queue[index].delivery {
            HaloAgentDelivery::Uncertain { command_id, .. } => {
                let orphan = command_id.clone();
                if let Some(row) = session.messages.iter().position(|m| m.id == orphan) {
                    let kept: Vec<_> = session
                        .messages
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != row)
                        .map(|(_, m)| (**m).clone())
                        .collect();
                    session.messages = kept.into();
                }
                session.queue[index].delivery = HaloAgentDelivery::Pending;
            }
            HaloAgentDelivery::Rejected { .. } => {
                // Persisted Rejected rows have no command_id — drop the failed
                // user tail so Resend does not stack duplicate bubbles. The
                // bool is ignored on purpose: whether or not the tail matched,
                // the explicit Send now proceeds.
                let _ = drop_failed_user_tail(&mut session.messages);
                session.queue[index].delivery = HaloAgentDelivery::Pending;
            }
            HaloAgentDelivery::Pending | HaloAgentDelivery::Sending { .. } => {}
        }
        let item = session.queue[index].clone();
        let active_run = session.run_id.clone().filter(|r| session.accepts_run(r));
        let run_id = active_run
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let command_id = if active_run.is_some() {
            uuid::Uuid::new_v4().to_string()
        } else {
            run_id.clone()
        };
        if active_run.is_none() {
            session.register_run(run_id.clone());
        }
        let mut parts = Vec::new();
        if !item.attachments.is_empty() {
            parts.push(HaloAgentPart::Attachments(item.attachments.clone()));
        }
        if !item.text.is_empty() {
            parts.push(HaloAgentPart::Text(item.text.clone()));
        }
        session
            .messages
            .push(HaloAgentMessage::new(command_id.clone(), HaloAgentRole::User, parts));
        // The prompt is in the transcript now — drop it from the queue immediately
        // instead of leaving a Sending row until the harness acks.
        session.queue.remove(index);
        let dispatch = if active_run.is_some() {
            AgentDispatch::Steer {
                chat_id: chat_id.into(),
                run_id,
                command_id,
                text: item.text,
                attachments: item.attachments,
            }
        } else {
            AgentDispatch::Run(RunRequest {
                run_id,
                chat_id: chat_id.into(),
                prompt: item.text,
                harness,
                model: session.model.clone().or(model),
                cwd: root,
                approval_mode: session.approval_mode.id().to_string(),
                plan_mode: session.plan_mode,
                resume: session.harness_session_id.clone(),
                attachments: item.attachments,
                skills: run_skills,
                mcp_servers: run_mcp,
                computer_use,
                computer_use_headed,
            })
        };
        self.finish_agent_batch(&[chat_id.to_string()], true);
        Ok(Some(dispatch))
    }

    fn agent_launch_context(&self) -> AgentLaunchContext {
        AgentLaunchContext {
            custom_executable: self.coder_settings.pi_executable_path.clone(),
            opencode_executable: self.coder_settings.opencode_executable_path.clone(),
            antigravity_executable: self.coder_settings.antigravity_executable_path.clone(),
            anthropic_api_key: self.coder_settings.anthropic_api_key.clone(),
            openai_api_key: self.coder_settings.openai_api_key.clone(),
            gemini_api_key: self.coder_settings.gemini_api_key.clone(),
            deepseek_api_key: self.coder_settings.deepseek_api_key.clone(),
            ollama_base_url: self.coder_settings.ollama_base_url.clone(),
        }
    }

    /// Undo an optimistic steer whose transport failed. Delivery never
    /// happened — every harness refuses before queueing — so drop the user
    /// bubble the dispatch pushed and put the prompt back at the head of the
    /// queue as Pending. The run itself is untouched: a live turn keeps going,
    /// and the next flush, drain, or explicit Send now retries the message.
    pub(crate) fn requeue_failed_steer(
        &mut self,
        chat_id: &str,
        command_id: &str,
        text: &str,
        attachments: Vec<String>,
    ) {
        let Some(work) = self.coder_work_for_chat_mut(chat_id) else {
            return;
        };
        let Some(session) = work.agent_sessions.iter_mut().find(|s| s.id == chat_id) else {
            return;
        };
        if let Some(row) = session.messages.iter().position(|m| m.id == command_id) {
            let kept: Vec<_> = session
                .messages
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != row)
                .map(|(_, m)| (**m).clone())
                .collect();
            session.messages = kept.into();
        }
        session.queue.insert(
            0,
            HaloAgentQueuedMessage {
                id: uuid::Uuid::new_v4().to_string(),
                text: text.to_string(),
                attachments,
                delivery: HaloAgentDelivery::Pending,
            },
        );
        self.finish_agent_batch(&[chat_id.to_string()], true);
    }
}

impl Bridge {
    pub fn halo_agent_toggle_plan_mode(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_plan_mode = !s.coder.agent_plan_mode;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Set approval autonomy for the active session only (not global settings).
    pub fn halo_agent_set_approval_mode(
        mode: crate::AgentApprovalMode,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let live = Self::state(cx).update(cx, |s| {
            s.coder.agent_approval_mode = mode;
            s.coder.agent_approval_picker_open = false;
            s.commit_active_tab();
            s.coder.agent_run_id.as_ref().map(|run_id| {
                (
                    s.coder.agent_active_session_id.clone(),
                    run_id.clone(),
                    mode.id().to_string(),
                )
            })
        });
        // The running harness captured its policy at spawn. Push the new mode
        // so a turn already in progress stops asking (and the card on screen
        // is answered Allow). Idle chats have nothing to update.
        if let Some((chat_id, run_id, mode_id)) = live {
            if let Ok(backend) = Self::backend(cx) {
                let _ = backend.halo_agent_set_live_approval(&chat_id, &run_id, &mode_id);
            }
        }
        Ok(())
    }

    pub fn halo_agent_toggle_approval_picker(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            let opening = !s.coder.agent_approval_picker_open;
            s.coder.agent_approval_picker_open = opening;
            if opening {
                s.coder.agent_picker_open = false;
            }
        });
        Ok(())
    }

    pub fn halo_agent_close_approval_picker(cx: &mut App) -> BridgeResult<()> {
        // Same guard as the model picker: an outside press on every click must not
        // notify observers (and repaint the shell) once the flyout is already gone.
        if !Self::state(cx).read(cx).coder.agent_approval_picker_open {
            return Ok(());
        }
        Self::state(cx).update(cx, |s| {
            s.coder.agent_approval_picker_open = false;
        });
        Ok(())
    }

    /// Store composer text in the owning session, not a global cross-chat draft.
    pub fn halo_agent_set_draft(text: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_draft = text.to_string();
            s.commit_active_tab();
        });
    }

    pub fn halo_agent_submit(prompt: &str, cx: &mut App) -> BridgeResult<()> {
        let attachments =
            Self::state(cx).update(cx, |s| std::mem::take(&mut s.coder.agent_attachments));
        Self::halo_agent_submit_inner(prompt.trim(), attachments, cx)
    }

    pub(crate) fn halo_agent_submit_inner(
        prompt: &str,
        attachments: Vec<String>,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::halo_agent_enqueue_submission(prompt, attachments, false, cx)
    }

    fn halo_agent_enqueue_submission(
        prompt: &str,
        attachments: Vec<String>,
        steer: bool,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if prompt.is_empty() && attachments.is_empty() {
            return Ok(());
        }
        let (chat_id, item_id, running) = Self::state(cx).update(cx, |s| {
            s.coder.agent_ensure_session();
            let id = uuid::Uuid::new_v4().to_string();
            s.coder.agent_queue.push(HaloAgentQueuedMessage {
                id: id.clone(),
                text: prompt.to_string(),
                attachments,
                delivery: HaloAgentDelivery::Pending,
            });
            clear_submitted_draft(&mut s.coder.agent_draft, prompt);
            if s.coder.agent_running || steer {
                // Follow-up stays visible in the queue until the live turn can merge it.
                s.coder.agent_dock_queue_open = true;
            }
            s.commit_active_tab();
            (
                s.coder.agent_active_session_id.clone(),
                id,
                s.coder.agent_running,
            )
        });
        if running || steer {
            // Leave the row in the queue panel. Steer once the harness handle
            // exists — immediately if it already does, otherwise on HarnessReady.
            // Stop never reaches here, so an interrupt still does not start the queue.
            Self::halo_agent_schedule_steer(&chat_id, cx);
        } else {
            Self::halo_agent_dispatch_queue_item(&chat_id, &item_id, false, cx)?;
        }
        Ok(())
    }

    /// Merge every Pending follow-up into the live turn once the harness can steer.
    fn halo_agent_schedule_steer(chat_id: &str, cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        if !backend.halo_agent_can_steer(chat_id).unwrap_or(false) {
            return;
        }
        let chat_id = chat_id.to_string();
        cx.spawn(async move |cx: &mut gpui::AsyncApp| {
            // One frame so the queue panel paints before the row is steered out.
            let executor = cx.background_executor();
            let _ = executor.timer(std::time::Duration::from_millis(32)).await;
            let _ = cx.update(|cx| Self::halo_agent_flush_steers(&chat_id, cx));
        })
        .detach();
    }

    fn halo_agent_flush_steers(chat_id: &str, cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        if !backend.halo_agent_can_steer(chat_id).unwrap_or(false) {
            return;
        }
        let ids = Self::state(cx).read(cx).coder.agent_queue.iter().filter_map(|item| {
            matches!(item.delivery, HaloAgentDelivery::Pending).then(|| item.id.clone())
        }).collect::<Vec<_>>();
        for id in ids {
            let _ = Self::halo_agent_dispatch_queue_item(chat_id, &id, true, cx);
        }
    }

    pub fn halo_agent_steer_now(prompt: &str, cx: &mut App) -> BridgeResult<()> {
        let attachments =
            Self::state(cx).update(cx, |s| std::mem::take(&mut s.coder.agent_attachments));
        Self::halo_agent_enqueue_submission(prompt.trim(), attachments, true, cx)
    }

    pub(crate) fn halo_agent_dispatch_queue_item(
        chat_id: &str,
        item_id: &str,
        steer: bool,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let launch = Self::state(cx).read(cx).agent_launch_context();
        // A steer before the harness handle exists must stay in the queue panel,
        // not become a Send failed card.
        if steer && !backend.halo_agent_can_steer(chat_id).unwrap_or(false) {
            return Ok(());
        }
        let dispatch =
            Self::state(cx).update(cx, |s| s.prepare_agent_dispatch(chat_id, item_id, steer))?;
        let Some(dispatch) = dispatch else {
            return Ok(());
        };
        // Snapshot + SQLite upsert off the UI thread so submit stays snappy.
        Self::halo_agent_persist_chats_async(&[chat_id.to_string()], false, cx);
        let (run_id, result, failed_steer) = match dispatch {
            AgentDispatch::Run(request) => {
                let run_id = request.run_id.clone();
                let result = backend
                    .halo_agent_run(&request, &launch)
                    .and_then(|returned| {
                        if returned == run_id {
                            Ok(())
                        } else {
                            Err(BridgeError::message(
                                "Agent returned a different run identity",
                            ))
                        }
                    });
                (run_id, result, None)
            }
            AgentDispatch::Steer {
                chat_id,
                run_id,
                command_id,
                text,
                attachments,
            } => {
                let result =
                    backend.halo_agent_steer(&chat_id, &run_id, &command_id, &text, &attachments);
                let failure = result
                    .is_err()
                    .then(|| (chat_id, command_id, text, attachments));
                (run_id, result, failure)
            }
        };
        let mut failed_steer = failed_steer;
        if let Err(err) = &result {
            match failed_steer.take() {
                Some((chat_id, command_id, text, attachments)) => {
                    // The steer never reached the harness, and AgentEvent::Error
                    // would settle a run that may still be perfectly healthy
                    // (can_steer raced the handle going away, queue full). Undo
                    // the optimistic dispatch instead: the prompt returns to the
                    // tray head as Pending for the next flush or drain.
                    Self::state(cx).update(cx, |s| {
                        s.push_debug_log(DebugLogEntry::new(
                            DebugLogLevel::Error,
                            "halo-agent",
                            err.to_string(),
                        ));
                        s.requeue_failed_steer(&chat_id, &command_id, &text, attachments);
                        s.apply_notice("Delivery failed — your message is back in the queue.".into());
                    });
                    Self::halo_agent_persist_chats_async(&[chat_id], false, cx);
                }
                None => {
                    Self::state(cx).update(cx, |s| {
                        // A transport failure is not a negative acknowledgement. Preserve
                        // images/text and mark uncertain rather than silently replaying.
                        s.apply_event(BackendEvent::HaloAgentEvent {
                            run_id,
                            chat_id: chat_id.to_string(),
                            event: AgentEvent::Error {
                                message: err.to_string(),
                            },
                        });
                    });
                }
            }
        }
        result
    }

    pub fn halo_agent_interrupt(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let chat_id = Self::state(cx)
            .read(cx)
            .coder
            .agent_active_session_id
            .clone();
        // Revoke admission before sending abort; late callbacks are harmless.
        Self::state(cx).update(cx, |s| {
            if let Some(work) = s.coder_work_for_chat_mut(&chat_id) {
                if let Some(session) = work.agent_sessions.iter_mut().find(|x| x.id == chat_id) {
                    if session.run_id.is_some() {
                        session.settle_run(HaloAgentSessionStatus::Idle);
                        session.system_part(HaloAgentPart::Interrupted);
                    }
                }
            }
            s.finish_agent_batch(&[chat_id.clone()], true);
        });
        backend.halo_agent_interrupt_chat(&chat_id)
    }

    pub fn halo_agent_retry_last(cx: &mut App) -> BridgeResult<()> {
        let (prompt, attachments) = Self::state(cx).update(cx, |s| {
            if s.coder.agent_running {
                return Ok((String::new(), Vec::new()));
            }
            // Never silently retry a command whose delivery is unknown.
            if s.coder.agent_queue.iter().any(|q| {
                matches!(
                    q.delivery,
                    HaloAgentDelivery::Uncertain { .. } | HaloAgentDelivery::Sending { .. }
                )
            }) {
                return Err(BridgeError::message(
                    "Check and remove the uncertain queued submission before retrying; it may already have been applied.",
                ));
            }
            let Some(msg) = s
                .coder
                .agent_messages
                .iter()
                .rev()
                .find(|m| m.role == HaloAgentRole::User)
                .map(|m| (**m).clone())
            else {
                s.apply_notice("Nothing to resend — this chat has no sent message.".into());
                return Ok((String::new(), Vec::new()));
            };
            let (prompt, attachments) = user_prompt_parts(&msg);
            // Only a genuinely failed tail may be popped. Resending an
            // answered turn would duplicate the user bubble under the old
            // reply, so a healthy tail is a no-op with a notice.
            if !drop_failed_user_tail(&mut s.coder.agent_messages) {
                s.apply_notice("Nothing to resend — the last message already got a reply.".into());
                return Ok((String::new(), Vec::new()));
            }
            // Drop leftover Rejected rows — Resend requeues from the bubble.
            s.coder
                .agent_queue
                .retain(|q| !matches!(q.delivery, HaloAgentDelivery::Rejected { .. }));
            s.commit_active_tab();
            Ok((prompt, attachments))
        })?;
        if prompt.is_empty() && attachments.is_empty() {
            return Ok(());
        }
        Self::halo_agent_submit_inner(&prompt, attachments, cx)
    }
}

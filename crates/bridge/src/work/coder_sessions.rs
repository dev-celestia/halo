//! Agent-session lifecycle methods on [`CoderWork`].

use super::*;

impl CoderWork {
    fn agent_new_session_slot(&self) -> HaloAgentSession {
        let mut session = HaloAgentSession::new_for_project(self.root_path.clone());
        // Seed from the live projection so Bridge can pre-load the global default
        // before the first empty slot is created.
        session.approval_mode = self.agent_approval_mode;
        session.model = self.agent_model.clone();
        session.harness_session_owner = Some(self.agent_harness.clone());
        session
    }

    /// Ensure there is always one active chat session and live fields mirror it.
    pub fn agent_ensure_session(&mut self) {
        if self.agent_sessions.is_empty() {
            let session = self.agent_new_session_slot();
            self.agent_active_session_id = session.id.clone();
            self.agent_sessions.push(session);
            self.agent_load_active_session();
            return;
        }
        if !self
            .agent_sessions
            .iter()
            .any(|s| s.id == self.agent_active_session_id)
        {
            self.agent_active_session_id = self.agent_sessions[0].id.clone();
            self.agent_load_active_session();
        }
    }

    /// Whether the Agent rail accordion section for `key` is expanded.
    ///
    /// `key` is a project path, or `""` for the "No project" group.
    pub fn agent_rail_section_open(&self, key: &str) -> bool {
        !self.agent_rail_collapsed.iter().any(|k| k == key)
    }

    /// Toggle an Agent rail accordion section open/collapsed.
    pub fn agent_toggle_rail_section(&mut self, key: &str) {
        if let Some(i) = self.agent_rail_collapsed.iter().position(|k| k == key) {
            self.agent_rail_collapsed.remove(i);
        } else {
            self.agent_rail_collapsed.push(key.to_string());
        }
    }

    /// Whether a Context rail section (`session`, `subagents`, …) is expanded.
    pub fn agent_context_section_open(&self, key: &str) -> bool {
        self.agent_context_expanded.iter().any(|k| k == key)
    }

    /// Fold or expand a Context rail section.
    pub fn agent_toggle_context_section(&mut self, key: &str) {
        if let Some(i) = self.agent_context_expanded.iter().position(|k| k == key) {
            self.agent_context_expanded.remove(i);
        } else {
            self.agent_context_expanded.push(key.to_string());
        }
    }

    /// Replace collapsed keys from an Accordion `on_toggle_click` open-index set.
    pub fn agent_set_rail_open_sections(&mut self, open_ixs: &[usize], keys: &[String]) {
        let open: std::collections::HashSet<usize> = open_ixs.iter().copied().collect();
        self.agent_rail_collapsed = keys
            .iter()
            .enumerate()
            .filter(|(i, _)| !open.contains(i))
            .map(|(_, k)| k.clone())
            .collect();
    }

    /// Bind the active UI session to a harness thread id for later resume.
    pub fn agent_set_harness_session_id(&mut self, session_id: Option<String>) {
        self.agent_ensure_session();
        let id = self.agent_active_session_id.clone();
        let harness = self.agent_harness.clone();
        if let Some(session) = self.agent_sessions.iter_mut().find(|s| s.id == id) {
            match session_id {
                Some(sid) => session.set_harness_session(&harness, sid),
                None => session.clear_harness_session(),
            }
        }
    }

    /// Write the live transcript/plan/queue into the active session slot.
    pub fn agent_flush_active_session(&mut self) {
        self.agent_ensure_session();
        let id = self.agent_active_session_id.clone();
        let title = self
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .filter(|s| !s.title.is_empty())
            .map(|s| s.title.clone())
            .unwrap_or_else(|| agent_session_title_from_messages(&self.agent_messages));
        let preview = agent_session_preview_from_messages(&self.agent_messages);
        if let Some(session) = self.agent_sessions.iter_mut().find(|s| s.id == id) {
            let live_status = if self.agent_running {
                HaloAgentSessionStatus::Working
            } else if session.status == HaloAgentSessionStatus::Errored {
                HaloAgentSessionStatus::Errored
            } else {
                HaloAgentSessionStatus::Idle
            };
            let changed = session.messages != self.agent_messages
                || session.subagents != self.agent_subagents
                || session.todos != self.agent_todos
                || session.queue != self.agent_queue
                || session.file_changes != self.agent_file_changes
                || session.skills != self.agent_enabled_skills
                || session.mcp_servers != self.agent_enabled_mcp
                || session.context_usage != self.agent_context_usage
                || session.todo_expanded != self.agent_todo_expanded
                || session.todo_dismissed != self.agent_todo_dismissed
                || session.status != live_status
                || session.run_id != self.agent_run_id;
            session.messages = self.agent_messages.clone();
            session.subagents = self.agent_subagents.clone();
            session.streaming_part = self.agent_streaming_part.clone();
            // The flat panel may be showing a child's request; keep it with its
            // owner so a child approval never overwrites the root's request.
            match self.agent_pending_input.clone() {
                Some(pending) => {
                    let owner = session.subagents.iter_mut().find(|child| {
                        child
                            .pending_input
                            .as_ref()
                            .is_some_and(|q| q.request_id == pending.request_id)
                    });
                    match owner {
                        Some(child) => child.pending_input = Some(pending),
                        None => session.pending_input = Some(pending),
                    }
                }
                None => session.pending_input = None,
            }
            session.attachments = self.agent_attachments.clone();
            session.draft = self.agent_draft.clone();
            let session_owner = session.effective_harness();
            let matches_harness =
                session_owner.is_none() || session_owner == Some(self.agent_harness.as_str());
            if matches_harness {
                session.model = self.agent_model.clone();
                if session.harness_session_id.is_none() {
                    session.harness_session_owner = Some(self.agent_harness.clone());
                }
            }
            session.plan_mode = self.agent_plan_mode;
            session.approval_mode = self.agent_approval_mode;
            session.completion = self.agent_completion.clone();
            session.todos = self.agent_todos.clone();
            session.todo_expanded = self.agent_todo_expanded;
            session.todo_dismissed = self.agent_todo_dismissed;
            session.queue = self.agent_queue.clone();
            session.file_changes = self.agent_file_changes.clone();
            session.skills = self.agent_enabled_skills.clone();
            session.mcp_servers = self.agent_enabled_mcp.clone();
            session.context_usage = self.agent_context_usage.clone();
            session.status = live_status;
            session.run_id = self.agent_run_id.clone();
            session.preview = preview;
            if !title.is_empty() {
                session.title = title;
            }
            if changed {
                session.updated_at = agent_now_ms();
            }
        }
    }

    pub(crate) fn agent_load_active_session(&mut self) {
        let id = self.agent_active_session_id.clone();
        if let Some(owner) = self
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.effective_harness())
            .map(str::to_string)
        {
            if self.agent_harness != owner {
                let previous = std::mem::replace(&mut self.agent_harness, owner.clone());
                if let Some(model) = self.agent_model.clone() {
                    self.agent_model_by_harness.insert(previous, model);
                }
                let catalog = self
                    .agent_model_cache
                    .get(&owner)
                    .cloned()
                    .unwrap_or_default();
                let session_model = self
                    .agent_sessions
                    .iter()
                    .find(|s| s.id == id)
                    .and_then(|s| s.model.clone());
                self.agent_model = session_model
                    .or_else(|| self.agent_model_by_harness.get(&owner).cloned())
                    .or_else(|| catalog.first().map(|m| m.id.clone()));
                self.agent_model_catalog = catalog;
                if let Some(row) = self
                    .agent_harness_readiness
                    .iter()
                    .find(|row| row.id == owner)
                {
                    if row.id == "cursor" {
                        let ready = row.installed && row.auth_ready;
                        self.agent_harness_installed = ready;
                        self.agent_pi_needs_upgrade = false;
                        self.agent_state_blocked = false;
                        self.agent_pi_setup_hint = if ready {
                            None
                        } else {
                            Some(row.subtitle.clone())
                        };
                    } else {
                        let ready = row.installed && !row.needs_upgrade && !row.state_blocked;
                        self.agent_harness_installed = ready;
                        self.agent_pi_needs_upgrade = row.needs_upgrade;
                        self.agent_state_blocked = row.state_blocked;
                        self.agent_pi_setup_hint = if ready {
                            None
                        } else {
                            Some(row.subtitle.clone())
                        };
                    }
                } else if let Some(info) = self
                    .agent_harness_catalog
                    .iter()
                    .find(|h| h.id == owner)
                {
                    self.agent_harness_installed = info.installed;
                    self.agent_pi_needs_upgrade = false;
                    self.agent_state_blocked = false;
                    self.agent_pi_setup_hint = None;
                }
            }
        }
        self.agent_refresh_active_session();
        self.agent_picker_open = false;
        self.agent_approval_picker_open = false;
    }

    /// Refresh only the panel projection; never change selection or another chat's draft.
    pub(crate) fn agent_refresh_active_session(&mut self) {
        let id = self.agent_active_session_id.clone();
        let Some(session) = self.agent_sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        let working = session.is_working();
        let pending_input = session.visible_pending_input();
        self.agent_messages = session.messages;
        self.agent_subagents = session.subagents.clone();
        let open = self.agent_open_subagent.take();
        self.agent_open_subagent =
            open.filter(|id| self.agent_subagents.iter().any(|s| &s.session_id == id));
        self.agent_todos = session.todos;
        self.agent_todo_expanded = session.todo_expanded;
        self.agent_todo_dismissed = session.todo_dismissed;
        self.agent_queue = session.queue;
        self.agent_file_changes = session.file_changes;
        self.agent_enabled_skills = session.skills;
        self.agent_enabled_mcp = session.mcp_servers;
        self.agent_context_usage = session.context_usage;
        self.agent_running = working;
        // A live turn owns the model — close the picker so a half-open sheet
        // cannot change it mid-reply.
        if working {
            self.agent_picker_open = false;
        }
        self.agent_run_id = session.run_id;
        self.agent_streaming_part = session.streaming_part;
        self.agent_pending_input = pending_input;
        let pending_id = self
            .agent_pending_input
            .as_ref()
            .map(|pending| pending.request_id.clone());
        if pending_id != self.agent_dock_pending_id {
            self.agent_dock_todo_open = false;
            self.agent_dock_queue_open = false;
            self.agent_dock_pending_id = pending_id;
        }
        self.agent_attachments = session.attachments;
        self.agent_draft = session.draft;
        if let Some(model) = session.model {
            // Normalize SDK aliases (e.g. Cursor `default` → `auto`) so a live
            // refresh cannot put a non-catalog id onto the composer chip.
            self.agent_model = Some(halo_backend::commands::halo_agent::migrate_model_id(&model));
        }
        self.agent_plan_mode = session.plan_mode;
        self.agent_approval_mode = session.approval_mode;
        self.agent_completion = session.completion;
    }

    pub fn agent_set_session_status(&mut self, chat_id: &str, status: HaloAgentSessionStatus) {
        if let Some(session) = self.agent_sessions.iter_mut().find(|s| s.id == chat_id) {
            session.status = status;
            if status != HaloAgentSessionStatus::Working {
                session.settle_run(status);
            }
        }
        if self.agent_active_session_id == chat_id {
            self.agent_running = status == HaloAgentSessionStatus::Working;
            if status != HaloAgentSessionStatus::Working {
                self.agent_refresh_active_session();
            }
        }
    }

    /// Archive the current chat (if it has content) and start a blank session.
    pub fn agent_new_chat(&mut self) {
        self.agent_flush_active_session();
        let current_empty = self.agent_messages.is_empty();
        if current_empty {
            // Keep the empty slot bound to the open project when the root changes.
            if let Some(session) = self
                .agent_sessions
                .iter_mut()
                .find(|s| s.id == self.agent_active_session_id)
            {
                session.root_path = self.root_path.clone();
            }
            return;
        }
        let session = self.agent_new_session_slot();
        self.agent_active_session_id = session.id.clone();
        self.agent_sessions.insert(0, session);
        self.agent_load_active_session();
    }

    /// After a workspace root change, show a chat that belongs to that project.
    ///
    /// Keeps the active session when it already matches `root_path`; otherwise
    /// activates the latest non-archived chat for the project, or starts a blank
    /// one. Session history is preserved — only the visible transcript switches.
    pub fn agent_sync_chat_to_root(&mut self) {
        let Some(path) = self.root_path.clone() else {
            return;
        };
        let active_matches = self
            .agent_sessions
            .iter()
            .find(|s| s.id == self.agent_active_session_id)
            .is_some_and(|s| s.root_path.as_deref() == Some(path.as_str()));
        if active_matches {
            return;
        }
        let best = self
            .agent_sessions
            .iter()
            .filter(|sess| !sess.archived && sess.root_path.as_deref() == Some(path.as_str()))
            .max_by_key(|sess| sess.updated_at)
            .map(|sess| sess.id.clone());
        if let Some(id) = best {
            self.agent_unarchive_session(&id);
            let _ = self.agent_select_session(&id);
        } else {
            self.agent_flush_active_session();
            self.agent_new_chat();
        }
    }

    /// Switch the panel to another saved session (other chats keep running).
    pub fn agent_select_session(&mut self, id: &str) -> bool {
        if id == self.agent_active_session_id {
            return true;
        }
        if !self.agent_sessions.iter().any(|s| s.id == id) {
            return false;
        }
        self.agent_flush_active_session();
        self.agent_active_session_id = id.to_string();
        self.agent_load_active_session();
        true
    }

    /// Soft-archive a session. Active → switch to another.
    pub fn agent_archive_session(&mut self, id: &str) {
        let was_active = self.agent_active_session_id == id;
        if let Some(session) = self.agent_sessions.iter_mut().find(|s| s.id == id) {
            session.archived = true;
            session.settle_run(HaloAgentSessionStatus::Idle);
            session.clear_harness_session();
            session.updated_at = agent_now_ms();
        }
        if was_active {
            self.agent_activate_fallback_session();
        }
        self.agent_ensure_session();
    }

    pub fn agent_unarchive_session(&mut self, id: &str) {
        if let Some(session) = self.agent_sessions.iter_mut().find(|s| s.id == id) {
            if !session.archived {
                return;
            }
            session.archived = false;
            // Do not bump `updated_at` — selecting / restoring must keep list position.
        }
    }

    /// Permanently drop a session from the Archived shelf.
    pub fn agent_delete_session(&mut self, id: &str) {
        let was_active = self.agent_active_session_id == id;
        self.agent_sessions.retain(|s| s.id != id);
        if was_active {
            self.agent_activate_fallback_session();
        }
        self.agent_ensure_session();
    }

    fn agent_activate_fallback_session(&mut self) {
        let next = self
            .agent_sessions
            .iter()
            .filter(|s| !s.archived)
            .max_by_key(|s| s.updated_at)
            .map(|s| s.id.clone());
        if let Some(next) = next {
            self.agent_active_session_id = next;
            self.agent_load_active_session();
        } else {
            let session = self.agent_new_session_slot();
            self.agent_active_session_id = session.id.clone();
            self.agent_sessions.insert(0, session);
            self.agent_load_active_session();
        }
    }

    /// Active Sessions list (non-archived), stable stored order.
    /// Selecting a chat must not reshuffle the rail — keep `agent_sessions` order.
    pub fn agent_sessions_sorted(&self) -> Vec<HaloAgentSession> {
        self.agent_sessions
            .iter()
            .filter(|s| !s.archived)
            .cloned()
            .collect()
    }

    /// Archived Sessions shelf, newest first.
    pub fn agent_archived_sessions_sorted(&self) -> Vec<HaloAgentSession> {
        let mut sessions: Vec<_> = self
            .agent_sessions
            .iter()
            .filter(|s| s.archived)
            .cloned()
            .collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }
}

//! Work-tab editor handoff and terminal surface retention.

use super::*;

impl HaloApp {
    pub(super) fn sync_work_tab_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.follows_work_tabs {
            return;
        }
        let current_id = self.bridge.active_work_tab_id.clone();
        let current_kind = self.bridge.active_work_kind();
        if self.last_work_tab_id == current_id {
            return;
        }
        if let (Some(prev_id), Some(prev_kind)) =
            (self.last_work_tab_id.clone(), self.last_work_kind)
        {
            let draft = self.capture_editor_draft(prev_kind, cx);
            self.editor_drafts.insert(prev_id, draft);
        }
        self.last_work_tab_id = current_id.clone();
        self.last_work_kind = current_kind;
        let Some(kind) = current_kind else {
            return;
        };
        if let Some(id) = current_id {
            if let Some(draft) = self.editor_drafts.get(&id).cloned() {
                self.apply_editor_draft(&draft, window, cx);
                self.lock_editor_cursors();
                return;
            }
        }
        self.apply_editor_defaults(kind, window, cx);
        self.lock_editor_cursors();
        if kind == WorkKind::Intercept && self.bridge.selected_paused_id.is_some() {
            self.last_intercept_paused_id = Some("__reload__".into());
        }
    }

    /// Keep only surfaces whose session still belongs to a live work tab.
    ///
    /// Reads *every* tab, not just the active one: the Coder dock's tabs live
    /// in `WorkData` while another page is on screen, and dropping their grids
    /// would rebuild scrollback from the store on return.
    pub(super) fn sync_terminal_surfaces(&mut self) {
        let mut live: Vec<String> = self
            .bridge
            .coder
            .terminals
            .iter()
            .filter(|t| t.kind == CoderTerminalKind::Shell)
            .map(|t| t.id.clone())
            .collect();
        for tab in &self.bridge.work_tabs {
            if let WorkData::Coder(work) = &tab.data {
                for term in &work.terminals {
                    if term.kind == CoderTerminalKind::Shell && !live.contains(&term.id) {
                        live.push(term.id.clone());
                    }
                }
            }
        }
        self.editors.terminal_surfaces.retain(&live);
    }

    fn stash_current_editor_draft(&mut self, cx: &mut Context<Self>) {
        if let (Some(prev_id), Some(prev_kind)) =
            (self.last_work_tab_id.clone(), self.last_work_kind)
        {
            let draft = self.capture_editor_draft(prev_kind, cx);
            self.editor_drafts.insert(prev_id, draft);
        }
    }

    fn lock_editor_cursors(&mut self) {
        self.last_history_detail_id = self.bridge.selected_detail.as_ref().map(|d| d.id.clone());
        self.last_forge_endpoint_id = self.bridge.active_request.endpoint_id.clone();
        self.last_forge_context_id = self.bridge.active_context_id.clone();
        self.last_intercept_paused_id = self.bridge.selected_paused_id.clone();
    }

    pub(super) fn apply_pending_transfers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.follows_work_tabs {
            return;
        }
        if let Some(req) = Bridge::take_pending_repeater(cx) {
            self.stash_current_editor_draft(cx);
            self.editors.set_repeater_method(&req.method, window, cx);
            self.editors.repeater_url.update(cx, |s, cx| {
                VariableInput::set_value(s, req.url.clone(), window, cx);
            });
            self.editors.repeater_headers.update(cx, |form, cx| {
                form.set_from_map(&req.headers, window, cx);
            });
            self.editors.repeater_request.update(cx, |e, cx| {
                e.set_text(req.body.clone(), window, cx);
            });
            let _ = Bridge::open_work_tab(WorkKind::Repeater, cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
            if let Some(id) = self.bridge.active_work_tab_id.clone() {
                let draft = self.capture_editor_draft(WorkKind::Repeater, cx);
                self.editor_drafts.insert(id.clone(), draft);
                self.last_work_tab_id = Some(id);
                self.last_work_kind = Some(WorkKind::Repeater);
            }
            self.lock_editor_cursors();
            self.page = Page::Repeater;
            self.launcher_open = false;
        }

        if let Some(draft) = Bridge::take_pending_intruder(cx) {
            self.stash_current_editor_draft(cx);
            self.editors.intruder_method.update(cx, |s, cx| {
                s.set_value(draft.method.clone(), window, cx);
            });
            self.editors.intruder_url.update(cx, |s, cx| {
                s.set_value(draft.url.clone(), window, cx);
            });
            self.editors.intruder_headers.update(cx, |s, cx| {
                s.set_value(draft.headers_text.clone(), window, cx);
            });
            self.editors.intruder_body.update(cx, |s, cx| {
                s.set_value(draft.body.clone(), window, cx);
            });
            let _ = Bridge::open_work_tab(WorkKind::Intruder, cx);
            self.bridge = Bridge::state(cx).read(cx).clone();
            if let Some(id) = self.bridge.active_work_tab_id.clone() {
                let draft = self.capture_editor_draft(WorkKind::Intruder, cx);
                self.editor_drafts.insert(id.clone(), draft);
                self.last_work_tab_id = Some(id);
                self.last_work_kind = Some(WorkKind::Intruder);
            }
            self.lock_editor_cursors();
            self.page = Page::Intruder;
            self.launcher_open = false;
        }

        if let Some(prompt) = Bridge::take_pending_agent_prompt(cx) {
            self.stash_current_editor_draft(cx);
            let _ = Bridge::focus_or_open_work_tab(WorkKind::Agent, cx);
            Bridge::state(cx).update(cx, |s| {
                s.coder.agent_ensure_session();
            });
            self.bridge = Bridge::state(cx).read(cx).clone();
            if let Some(id) = self.bridge.active_work_tab_id.clone() {
                let draft = self.capture_editor_draft(WorkKind::Agent, cx);
                self.editor_drafts.insert(id.clone(), draft);
                self.last_work_tab_id = Some(id);
                self.last_work_kind = Some(WorkKind::Agent);
            }
            self.page = Page::Agent;
            self.launcher_open = false;
            self.editors.halo_agent_input.update(cx, |input, cx| {
                input.set_value(prompt, window, cx);
                input.focus(window, cx);
            });
        }
    }
}

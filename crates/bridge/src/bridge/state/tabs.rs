//! Work-tab, session, and history helpers on [`BridgeState`].

use super::*;

impl BridgeState {
    pub const DEBUG_LOG_LIMIT: usize = 500;

    /// Failures kept in the Source Control Git Output dock. Older entries drop
    /// off the front so a repo that fails on every refresh cannot grow the
    /// snapshot without bound.
    pub const GIT_OUTPUT_LIMIT: usize = 200;

    /// Live history stream is frozen while a row is selected or its context menu is open.
    pub fn history_stream_paused(&self) -> bool {
        self.selected_log_id.is_some() || self.history_context_menu_open
    }

    pub fn push_debug_log(&mut self, entry: DebugLogEntry) {
        self.debug_logs.push(entry);
        if self.debug_logs.len() > Self::DEBUG_LOG_LIMIT {
            let excess = self.debug_logs.len() - Self::DEBUG_LOG_LIMIT;
            self.debug_logs.drain(0..excess);
        }
    }

    /// Record a failed git command for the Source Control output dock.
    ///
    /// Opening the dock is part of the record: the panes no longer print raw
    /// stderr inline, so the console is where a failure has to become visible.
    pub fn push_git_output(&mut self, label: impl Into<String>, detail: impl Into<String>) {
        let entry = GitOutputEntry::new(label, detail);
        // Auto-refreshes can retry the same failing command (a non-repo folder
        // re-lists every graph section on each visit). Keep one row per
        // distinct failure and just move its timestamp forward.
        if let Some(last) = self.git_output.last_mut() {
            if last.label == entry.label && last.detail == entry.detail {
                *last = entry;
                self.git_output_open = true;
                return;
            }
        }
        self.git_output.push(entry);
        if self.git_output.len() > Self::GIT_OUTPUT_LIMIT {
            let excess = self.git_output.len() - Self::GIT_OUTPUT_LIMIT;
            self.git_output.drain(0..excess);
        }
        self.git_output_open = true;
    }

    pub fn apply_ok_msg(&mut self, message: String) {
        self.push_debug_log(DebugLogEntry::new(
            DebugLogLevel::Info,
            "bridge",
            message.clone(),
        ));
        self.last_message = Some(message);
        self.last_error = None;
        self.last_error_info = false;
        self.coder_lsp_offer_install = None;
        self.coder_lsp_offer_install_label = None;
    }

    pub(in crate::bridge) fn apply_ok_proxy(&mut self, proxy: ProxyStatus, message: String) {
        self.proxy = proxy;
        self.apply_ok_msg(message);
    }

    pub fn apply_err(&mut self, err: BridgeError) {
        let msg = err.to_string();
        self.push_debug_log(DebugLogEntry::new(
            DebugLogLevel::Error,
            "bridge",
            msg.clone(),
        ));
        self.last_error = Some(msg);
        self.last_error_info = false;
    }

    /// Record a failed git command: raw output to the Git Output dock and the
    /// debug log, an info-level toast that points at the dock.
    ///
    /// Toasts are for telling the user where to look; multi-line stderr in a
    /// red toast is unreadable and disappears before it can be read anyway.
    pub fn apply_git_err(&mut self, op: &str, detail: String, message: String) {
        self.push_git_output(op, detail.clone());
        self.push_debug_log(DebugLogEntry::new(DebugLogLevel::Error, "bridge", detail));
        self.last_error = Some(message);
        self.last_error_info = true;
    }

    /// Record a recoverable failure as an info toast, not the red error toast.
    ///
    /// For failures that already carry their recovery path — a message
    /// returned to the queue, a resend that had nothing to replay. The user
    /// must never learn about a problem from silence, but a red toast would
    /// overstate a failure the bridge just repaired.
    pub fn apply_notice(&mut self, message: String) {
        self.push_debug_log(DebugLogEntry::new(
            DebugLogLevel::Warn,
            "bridge",
            message.clone(),
        ));
        self.last_error = Some(message);
        self.last_error_info = true;
    }

    pub fn active_work_kind(&self) -> Option<WorkKind> {
        let id = self.active_work_tab_id.as_deref()?;
        self.work_tabs.iter().find(|t| t.id == id).map(|t| t.kind)
    }

    pub fn active_work_tab(&self) -> Option<&WorkTab> {
        let id = self.active_work_tab_id.as_deref()?;
        self.work_tabs.iter().find(|t| t.id == id)
    }

    pub fn active_activity_session(&self) -> Option<&ActivitySession> {
        let id = self.active_activity_session_id.as_deref()?;
        self.activity_sessions
            .iter()
            .find(|session| session.id == id)
    }

    /// Tabs whose captured activity belongs to the session in front.
    pub fn session_work_tabs(&self) -> Vec<&WorkTab> {
        let Some(id) = self.active_activity_session_id.as_deref() else {
            return self.work_tabs.iter().collect();
        };
        self.work_tabs
            .iter()
            .filter(|tab| tab.session_id == id)
            .collect()
    }

    pub fn session_paused_len(&self) -> usize {
        let ids: Vec<&str> = self
            .session_work_tabs()
            .iter()
            .map(|tab| tab.id.as_str())
            .collect();
        self.paused
            .iter()
            .filter(|row| row.tab_id.as_deref().is_some_and(|id| ids.contains(&id)))
            .count()
    }

    pub(in crate::bridge) fn tab_in_active_session(&self, tab: &WorkTab) -> bool {
        match self.active_activity_session_id.as_deref() {
            Some(id) => tab.session_id == id,
            None => true,
        }
    }

    pub(in crate::bridge) fn ensure_activity_session(&mut self) -> String {
        if self.activity_sessions.is_empty() {
            let id = uuid::Uuid::new_v4().to_string();
            self.activity_sessions.push(ActivitySession {
                id: id.clone(),
                title: "Session".into(),
                active_work_tab_id: None,
                history: Vec::new(),
            });
            self.active_activity_session_id = Some(id);
        }
        if self.active_activity_session_id.is_none() {
            self.active_activity_session_id = self
                .activity_sessions
                .first()
                .map(|session| session.id.clone());
        }
        self.active_activity_session_id.clone().unwrap_or_default()
    }

    pub(in crate::bridge) fn shell_layout_snapshot(
        &self,
    ) -> halo_backend::commands::shell::ShellLayout {
        use halo_backend::commands::shell::{ShellLayout, ShellSession, ShellTab};
        let mut sessions: Vec<ShellSession> = self
            .activity_sessions
            .iter()
            .map(|session| ShellSession {
                id: session.id.clone(),
                title: session.title.clone(),
                active_work_tab_id: session.active_work_tab_id.clone(),
            })
            .collect();
        // Keep the live active tab on the current session so relaunch restores it.
        if let Some(active_sid) = &self.active_activity_session_id {
            if let Some(session) = sessions.iter_mut().find(|s| &s.id == active_sid) {
                session.active_work_tab_id = self.active_work_tab_id.clone();
            }
        }
        ShellLayout {
            sessions,
            active_session_id: self.active_activity_session_id.clone(),
            tabs: self
                .work_tabs
                .iter()
                .map(|tab| ShellTab {
                    id: tab.id.clone(),
                    title: tab.title.clone(),
                    kind: tab.kind.as_str().to_string(),
                    session_id: tab.session_id.clone(),
                })
                .collect(),
            active_tab_id: self.active_work_tab_id.clone(),
            appearance: self.appearance.clone(),
            pinned_apps: Some(
                self.pinned_apps
                    .iter()
                    .map(|kind| kind.as_str().to_string())
                    .collect(),
            ),
            onboarding_completed: self.onboarding_completed,
            onboarding_step: self.onboarding_step,
        }
    }

    /// Remember the live tabs and captured rows on the session that is leaving.
    pub(in crate::bridge) fn park_active_session(&mut self) {
        self.commit_active_tab();
        let Some(id) = self.active_activity_session_id.clone() else {
            return;
        };
        let history = self.history.clone();
        let active_tab = self.active_work_tab_id.clone();
        if let Some(session) = self
            .activity_sessions
            .iter_mut()
            .find(|session| session.id == id)
        {
            session.history = history;
            session.active_work_tab_id = active_tab;
        }
    }

    pub(in crate::bridge) fn show_activity_session(&mut self, id: &str) {
        let (tab_id, history) = self
            .activity_sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| (session.active_work_tab_id.clone(), session.history.clone()))
            .unwrap_or((None, Vec::new()));
        self.active_activity_session_id = Some(id.to_string());
        self.active_work_tab_id = tab_id;
        self.history = history;
        if self.active_work_tab_id.is_some() {
            self.load_active_tab();
        }
        if self.active_work_kind() != Some(WorkKind::HttpHistory) {
            self.history_filter.session_id = Some(format!("__activity_{id}"));
            self.selected_log_id = None;
            self.selected_detail = None;
        }
    }
}

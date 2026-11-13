//! Backend-event reduction: apply [`BackendEvent`] streams to state.

use celestia_ui::state::StoreCommit;

use super::*;

impl StoreCommit for BridgeState {
    fn on_store_commit(&mut self) {
        let paint = !self.hold_paint || self.needs_paint;
        self.hold_paint = false;
        self.needs_paint = false;
        if paint {
            self.paint_epoch = self.paint_epoch.wrapping_add(1);
        }
    }
}

impl BridgeState {
    /// False while an event batch has set [`Self::hold_paint`] and not committed.
    /// The shell must not copy that half-applied value.
    pub fn shell_snapshot_ready(&self) -> bool {
        !self.hold_paint
    }

    /// Apply a batch of backend-pushed events, coalescing `commit_active_tab()` to
    /// run at most once at the end of the batch.
    pub fn apply_events<I: IntoIterator<Item = BackendEvent>>(&mut self, events: I) {
        let mut needs_commit = false;
        let mut shell = false;
        let mut chats = Vec::new();
        for event in events {
            if let BackendEvent::HaloAgentEvent {
                run_id,
                chat_id,
                event,
            } = event
            {
                if !is_agent_token(&event) {
                    shell = true;
                }
                let first = !chats.contains(&chat_id);
                if self.reduce_agent_event(&run_id, &chat_id, event, first) && first {
                    chats.push(chat_id);
                }
            } else {
                shell = true;
                needs_commit |= self.apply_event_deferred(event);
            }
        }
        // Token-only batches refresh the live flat projection; tab mirrors wait
        // for shell-relevant events so we do not deep-clone every session/token.
        self.finish_agent_batch(&chats, shell);
        if shell {
            self.note_shell();
        }
        if needs_commit {
            self.commit_active_tab();
        }
    }

    pub(crate) fn note_shell(&mut self) {
        self.shell_epoch = self.shell_epoch.wrapping_add(1);
        self.needs_paint = true;
    }

    /// True when `self` differs from `previous` only by agent token text.
    /// Callers can keep the previous snapshot instead of cloning this state.
    pub fn suppresses_shell_refresh(&self, previous: &Self) -> bool {
        self.shell_epoch == previous.shell_epoch
            && self.agent_revision_fingerprint() != previous.agent_revision_fingerprint()
            && self.doctor == previous.doctor
            && harness_shell_eq(&self.coder, &previous.coder)
            && coder_chrome_eq(&self.coder, &previous.coder)
            && self.work_tabs.len() == previous.work_tabs.len()
            && self
                .work_tabs
                .iter()
                .zip(&previous.work_tabs)
                .all(|(next, prev)| {
                    next.id == prev.id
                        && next.kind == prev.kind
                        && match (&next.data, &prev.data) {
                            (WorkData::Coder(next), WorkData::Coder(prev)) => {
                                coder_chrome_eq(next, prev) && harness_shell_eq(next, prev)
                            }
                            _ => true,
                        }
                })
    }

    pub(in crate::bridge) fn agent_revision_fingerprint(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        let mut mix = |revision: u64| {
            hash ^= revision;
            hash = hash.wrapping_mul(0x100000001b3);
        };
        mix(self.coder.agent_messages.revision());
        for session in &self.coder.agent_sessions {
            mix(session.messages.revision());
        }
        for tab in &self.work_tabs {
            if let WorkData::Coder(work) = &tab.data {
                mix(work.agent_messages.revision());
                for session in &work.agent_sessions {
                    mix(session.messages.revision());
                }
            }
        }
        hash
    }

    /// Apply a backend-pushed event (proxy worker / intercept / invoker).
    pub fn apply_event(&mut self, event: BackendEvent) {
        self.apply_events(std::iter::once(event));
    }

    pub(in crate::bridge) fn apply_event_deferred(&mut self, event: BackendEvent) -> bool {
        let mut needs_commit = false;
        match event {
            BackendEvent::ProxyStatusChanged(status) => {
                self.proxy = status;
            }
            BackendEvent::ProxyRecord(row) => {
                if self.row_matches_filter(&row) {
                    // Keep the visible list stable while the operator inspects a
                    // row or holds a context menu open — queue a refresh instead.
                    if self.history_stream_paused() {
                        self.history_pending_count = self.history_pending_count.saturating_add(1);
                    } else {
                        self.history.retain(|r| r.id != row.id);
                        self.history.insert(0, row);
                        if self.history.len() > HISTORY_LIMIT as usize {
                            self.history.truncate(HISTORY_LIMIT as usize);
                        }
                    }
                }
            }
            BackendEvent::InvokerResult(row) => {
                needs_commit |= self.update_intruder(|intruder| {
                    if intruder.selected_result_id.is_none() {
                        intruder.selected_result_id = Some(row.id.clone());
                    }
                    intruder.results.push(row);
                    if intruder.results.len() > 1_000 {
                        intruder.results.drain(0..intruder.results.len() - 1_000);
                    }
                });
            }
            BackendEvent::InvokerProgress { current, total } => {
                needs_commit |= self.update_intruder(|intruder| {
                    intruder.progress_current = current;
                    intruder.progress_total = total;
                    intruder.message = Some(format!("{current} / {total}"));
                });
            }
            BackendEvent::InvokerComplete => {
                needs_commit |= self.update_intruder(|intruder| {
                    intruder.running = false;
                    intruder.message =
                        Some(format!("Complete — {} results", intruder.results.len()));
                });
            }
            BackendEvent::CrawlerSession(session) => {
                let id = session.id.clone();
                needs_commit |= self.update_crawler(Some(&id), |browser| {
                    browser.session = Some(session);
                });
            }
            BackendEvent::CrawlerPage(page) => {
                let session_id = page.session_id.clone();
                needs_commit |= self.update_crawler(Some(&session_id), |browser| {
                    if let Some(existing) = browser
                        .pages
                        .iter_mut()
                        .find(|p| p.id == page.id || p.url == page.url)
                    {
                        *existing = page;
                    } else {
                        browser.pages.push(page);
                        if browser.pages.len() > 2_000 {
                            let drain = browser.pages.len() - 2_000;
                            browser.pages.drain(0..drain);
                        }
                    }
                });
            }
            BackendEvent::CrawlerInsight(insight) => {
                let session_id = insight.session_id.clone();
                needs_commit |= self.update_crawler(Some(&session_id), |browser| {
                    if !browser.insights.iter().any(|i| i.id == insight.id) {
                        browser.insights.insert(0, insight);
                        if browser.insights.len() > 500 {
                            browser.insights.truncate(500);
                        }
                    }
                });
            }
            BackendEvent::CrawlerLog(log) => {
                let session_id = log.session_id.clone();
                needs_commit |= self.update_crawler(Some(&session_id), |browser| {
                    if !browser.logs.iter().any(|l| l.id == log.id) {
                        browser.logs.push(log);
                        if browser.logs.len() > 1_000 {
                            let drain = browser.logs.len() - 1_000;
                            browser.logs.drain(0..drain);
                        }
                    }
                });
            }
            BackendEvent::CrawlerSessionFailed {
                session_id,
                message,
            } => {
                needs_commit |= self.update_crawler(Some(&session_id), |browser| {
                    if let Some(session) = browser.session.as_mut() {
                        session.status = "failed".into();
                    }
                });
                self.last_error = Some(message);
                self.last_error_info = false;
            }
            BackendEvent::CoderPtyOutput { session_id, data } => {
                // Runtime bytes go straight to the store; the work-tab
                // projection carries layout only. `shell` already bumps the
                // repaint epoch, so no commit is needed here.
                self.terminal_sessions.push(&session_id, &data);
            }
            BackendEvent::CoderPtyExit {
                session_id,
                generation,
            } => {
                // A stale reader (previous process of a restarted tab) must not
                // mark the new shell dead; the store ignores generation mismatches.
                self.terminal_sessions.mark_exit(&session_id, generation);
            }
            BackendEvent::HaloAgentEvent { .. } => {
                unreachable!("agent events reduce in apply_events")
            }
            BackendEvent::PortScanResult { scan_id, row } => {
                Bridge::apply_port_scan_result(self, scan_id, row);
            }
            BackendEvent::PortScanProgress {
                scan_id,
                kind,
                current,
                total,
            } => {
                Bridge::apply_port_scan_progress(self, scan_id, &kind, current, total);
            }
            BackendEvent::MockLog(log) => {
                Bridge::apply_mock_log(self, log);
            }
            BackendEvent::MockServerStatus(status) => {
                self.mock.server = status;
            }
            BackendEvent::Log(msg) => {
                self.push_debug_log(DebugLogEntry::new(
                    DebugLogLevel::Info,
                    "backend",
                    msg.clone(),
                ));
                self.last_message = Some(msg);
            }
            BackendEvent::R2UploadProgress(progress) => {
                match &mut self.r2.upload {
                    Some(current) if current.file_name == progress.file_name => {
                        current.progress = progress.progress;
                    }
                    _ => self.r2.upload = Some(progress),
                }
                needs_commit = true;
            }
            BackendEvent::HistoryUpdated => {}
            BackendEvent::InterceptUpdated => {}
        }
        needs_commit
    }

    pub(in crate::bridge) fn row_matches_filter(&self, row: &HttpLogRow) -> bool {
        let f = &self.history_filter;
        if let Some(search) = f.search.as_ref().map(|s| s.trim().to_ascii_lowercase()) {
            if !search.is_empty() {
                let url_ok = row.url.to_ascii_lowercase().contains(&search);
                let host_ok = row
                    .host
                    .as_ref()
                    .map(|h| h.to_ascii_lowercase().contains(&search))
                    .unwrap_or(false);
                if !url_ok && !host_ok {
                    return false;
                }
            }
        }
        if !f.methods.is_empty()
            && !f
                .methods
                .iter()
                .any(|m| m.eq_ignore_ascii_case(&row.method))
        {
            return false;
        }
        if let Some(session_id) = &f.session_id {
            if &row.session_id != session_id {
                return false;
            }
        }
        if f.hide_304 && row.status == Some(304) {
            return false;
        }
        if f.errors_only {
            match row.status {
                Some(s) if (400..600).contains(&s) => {}
                _ => return false,
            }
        }
        if !f.status_classes.is_empty() {
            let Some(status) = row.status else {
                return false;
            };
            let class = status / 100;
            if !f.status_classes.contains(&class) {
                return false;
            }
        }
        true
    }
}

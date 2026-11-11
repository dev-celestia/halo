//! Active-tab projection: commit live state onto [`BridgeState`] flat fields.

use super::*;

impl BridgeState {
    /// Copy the live projection into the active tab.
    pub fn commit_active_tab(&mut self) {
        let Some(id) = self.active_work_tab_id.clone() else {
            return;
        };
        let Some(tab) = self.work_tabs.iter_mut().find(|t| t.id == id) else {
            return;
        };
        match &mut tab.data {
            WorkData::Http(work) => {
                work.filter = self.history_filter.clone();
                work.selected_log_id = self.selected_log_id.clone();
                work.selected_detail = self.selected_detail.clone();
                work.detail_tab = self.detail_tab;
                work.detail_side = self.detail_side;
            }
            WorkData::Intercept(work) => {
                work.capture_hosts = self.intercept.capture_patterns.clone();
                work.selected_paused_id = self.selected_paused_id.clone();
            }
            WorkData::Mock(work) | WorkData::Override(work) => {
                let _ = work; // filter lives in the page's editor entity
            }
            WorkData::Jwt(_work) => {
                // JWT state lives in the shared `JwtView` on this struct.
            }
            WorkData::PortScanner(_work) => {
                // Port Scanner state lives in the shared `PortScannerView`.
            }
            WorkData::Comparer(_work) => {
                // Comparer state lives in the shared `ComparerView`.
            }
            WorkData::FileExplorer(_work) => {
                // File Explorer state lives in the shared `FileExplorerView`.
            }
            WorkData::R2Storage(_work) => {
                // R2 state lives in the shared `R2View`.
            }
            WorkData::Wordlists(_work) => {
                // Wordlists state lives in the shared `WordlistsView`.
            }
            WorkData::SplitView(_work) => {
                // Layout lives in the shared `split_view` field on this struct.
            }
            WorkData::Agent(_work) => {
                // Same flush Coder uses: push flat agent_queue / draft / messages
                // into the active session before syncing out to Coder work tabs.
                // Without this, submit enqueues on `agent_queue` but dispatch
                // reads `session.queue` and finds nothing to send.
                self.coder.agent_flush_active_session();
                self.sync_agent_into_coder_tabs();
            }
            WorkData::Repeater(work) => {
                work.response = self.repeater_response.clone();
                work.last_sent_request = self.last_sent_request.clone();
                work.pending_repeater = self.pending_repeater.clone();
                work.selected_node_id = self.selected_node_id.clone();
                work.active_context_id = self.active_context_id.clone();
                work.active_request = self.active_request.clone();
                work.forge_view = self.forge_view;
                work.forge_request_tab = self.forge_request_tab;
                work.forge_response_tab = self.forge_response_tab;
                work.tree_filter = self.tree_filter.clone();
                work.collapsed_stashes = self.collapsed_stashes.clone();
            }
            WorkData::Intruder(work) => {
                work.status = self.intruder.clone();
                work.pending = self.pending_intruder.clone();
            }
            WorkData::Crawler(work) => *work = self.crawler.clone(),
            WorkData::Coder(work) => {
                self.coder.agent_flush_active_session();
                *work = self.coder.clone();
            }
            WorkData::Settings(tab) => *tab = self.settings_tab,
            WorkData::Notes(work) => {
                work.open_note_ids = self.notes_open_tab_ids.clone();
                work.active_note_id = self.notes_active_id.clone();
                work.search_query = self.notes_search_query.clone();
                work.view_mode = self.notes_view_mode;
            }
            WorkData::Diagram(work) => {
                *work = self.diagram.clone();
            }
            WorkData::Doctor(_work) => {
                // Doctor state lives in the shared `doctor` view on this struct.
            }
            WorkData::SourceControl(_work) => {
                // Git snapshot lives on BridgeState::git.
            }
        }
    }

    /// Project the active tab into the flat fields the pages read.
    pub fn load_active_tab(&mut self) {
        let Some(id) = self.active_work_tab_id.clone() else {
            return;
        };
        let Some(data) = self
            .work_tabs
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.data.clone())
        else {
            return;
        };
        match data {
            WorkData::Http(work) => {
                self.history_filter = work.filter;
                self.selected_log_id = work.selected_log_id;
                self.selected_detail = work.selected_detail;
                self.detail_tab = work.detail_tab;
                self.detail_side = work.detail_side;
            }
            WorkData::Intercept(work) => {
                self.intercept.capture_patterns = work.capture_hosts;
                self.selected_paused_id = work.selected_paused_id;
            }
            WorkData::Mock(work) | WorkData::Override(work) => {
                let _ = work; // filter lives in the page's editor entity
            }
            WorkData::Jwt(_work) => {
                // JWT state lives in the shared `JwtView` on this struct.
            }
            WorkData::PortScanner(_work) => {
                // Port Scanner state lives in the shared `PortScannerView`.
            }
            WorkData::Comparer(_work) => {
                // Comparer state lives in the shared `ComparerView`.
            }
            WorkData::FileExplorer(_work) => {
                // File Explorer state lives in the shared `FileExplorerView`.
            }
            WorkData::R2Storage(_work) => {
                // R2 state lives in the shared `R2View`.
            }
            WorkData::Wordlists(_work) => {
                // Wordlists state lives in the shared `WordlistsView`.
            }
            WorkData::SplitView(_work) => {
                // Layout lives in the shared `split_view` field on this struct.
            }
            WorkData::Agent(_work) => {
                // Leave `coder` (and its agent fields) alone so the Agent page
                // keeps the last projected project / session.
                self.coder.agent_ensure_session();
            }
            WorkData::Repeater(work) => {
                self.repeater_response = work.response;
                self.last_sent_request = work.last_sent_request;
                self.pending_repeater = work.pending_repeater;
                self.selected_node_id = work.selected_node_id;
                self.active_context_id = work.active_context_id;
                self.active_request = work.active_request;
                self.forge_view = work.forge_view;
                self.forge_request_tab = work.forge_request_tab;
                self.forge_response_tab = work.forge_response_tab;
                self.tree_filter = work.tree_filter;
                self.collapsed_stashes = work.collapsed_stashes;
            }
            WorkData::Intruder(work) => {
                self.intruder = work.status;
                self.pending_intruder = work.pending;
            }
            WorkData::Crawler(work) => self.crawler = work,
            WorkData::Coder(work) => {
                self.coder = work;
                self.coder.agent_ensure_session();
            }
            WorkData::Settings(tab) => self.settings_tab = tab,
            WorkData::Notes(work) => {
                self.notes_open_tab_ids = work.open_note_ids;
                self.notes_active_id = work.active_note_id;
                self.notes_search_query = work.search_query;
                self.notes_view_mode = work.view_mode;
            }
            WorkData::Diagram(work) => {
                self.diagram = work;
            }
            WorkData::Doctor(_work) => {
                // Doctor state lives in the shared `doctor` view on this struct.
            }
            WorkData::SourceControl(_work) => {
                // Git snapshot lives on BridgeState::git.
            }
        }
    }

    pub(in crate::bridge) fn update_intruder(
        &mut self,
        f: impl FnOnce(&mut IntruderStatus),
    ) -> bool {
        if self.active_work_kind() == Some(WorkKind::Intruder) {
            f(&mut self.intruder);
            return true;
        }
        if let Some(WorkData::Intruder(work)) = self
            .work_tabs
            .iter_mut()
            .rev()
            .find(|tab| {
                matches!(
                    &tab.data,
                    WorkData::Intruder(work)
                        if work.status.running || work.status.attack_id.is_some()
                )
            })
            .map(|tab| &mut tab.data)
        {
            f(&mut work.status);
            return false;
        }
        f(&mut self.intruder);
        false
    }

    pub(in crate::bridge) fn update_crawler(
        &mut self,
        session_id: Option<&str>,
        f: impl FnOnce(&mut CrawlerStatus),
    ) -> bool {
        let active_crawler = self.active_work_kind() == Some(WorkKind::Crawler);
        let active_matches = active_crawler
            && session_id.is_none_or(|id| {
                self.crawler
                    .session
                    .as_ref()
                    .is_none_or(|session| session.id == id)
            });
        if active_matches {
            f(&mut self.crawler);
            return true;
        }
        if let Some(id) = session_id {
            if let Some(WorkData::Crawler(work)) = self
                .work_tabs
                .iter_mut()
                .find(|tab| {
                    matches!(
                        &tab.data,
                        WorkData::Crawler(work)
                            if work.session.as_ref().is_some_and(|session| session.id == id)
                    )
                })
                .map(|tab| &mut tab.data)
            {
                f(work);
                return false;
            }
        }
        if active_crawler {
            f(&mut self.crawler);
            return true;
        }
        false
    }
}

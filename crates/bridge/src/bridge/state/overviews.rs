//! Cross-tool overview snapshots (intruder, crawler, repeater).

use super::*;

impl BridgeState {
    /// Running attack across every intruder tab, including the live projection.
    pub fn intruder_overview(&self) -> IntruderStatus {
        if self.active_work_kind() == Some(WorkKind::Intruder) && self.intruder.running {
            return self.intruder.clone();
        }
        if let Some(status) = self.work_tabs.iter().find_map(|tab| {
            if self.active_work_tab_id.as_deref() == Some(tab.id.as_str())
                || !self.tab_in_active_session(tab)
            {
                return None;
            }
            match &tab.data {
                WorkData::Intruder(work) if work.status.running => Some(work.status.clone()),
                _ => None,
            }
        }) {
            return status;
        }
        if self.active_work_kind() == Some(WorkKind::Intruder) {
            return self.intruder.clone();
        }
        self.work_tabs
            .iter()
            .rev()
            .filter(|tab| self.tab_in_active_session(tab))
            .find_map(|tab| match &tab.data {
                WorkData::Intruder(work) => Some(work.status.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn crawler_overview(&self) -> CrawlerStatus {
        if self.active_work_kind() == Some(WorkKind::Crawler) && self.crawler.session.is_some() {
            return self.crawler.clone();
        }
        if let Some(status) = self.work_tabs.iter().find_map(|tab| {
            if self.active_work_tab_id.as_deref() == Some(tab.id.as_str())
                || !self.tab_in_active_session(tab)
            {
                return None;
            }
            match &tab.data {
                WorkData::Crawler(work) if work.session.is_some() => Some(work.clone()),
                _ => None,
            }
        }) {
            return status;
        }
        if self.active_work_kind() == Some(WorkKind::Crawler) {
            return self.crawler.clone();
        }
        self.work_tabs
            .iter()
            .rev()
            .filter(|tab| self.tab_in_active_session(tab))
            .find_map(|tab| match &tab.data {
                WorkData::Crawler(work) => Some(work.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn any_repeater_response(&self) -> bool {
        if self.active_work_kind() == Some(WorkKind::Repeater) && self.repeater_response.is_some() {
            return true;
        }
        self.work_tabs.iter().any(|tab| {
            if self.active_work_tab_id.as_deref() == Some(tab.id.as_str())
                || !self.tab_in_active_session(tab)
            {
                return false;
            }
            matches!(&tab.data, WorkData::Repeater(work) if work.response.is_some())
        })
    }

    pub(in crate::bridge) fn push_work_tab(&mut self, kind: WorkKind) -> String {
        let session_id = self.ensure_activity_session();
        let existing = self
            .work_tabs
            .iter()
            .filter(|tab| tab.kind == kind && tab.session_id == session_id)
            .count();
        let id = uuid::Uuid::new_v4().to_string();
        self.work_tabs.push(WorkTab {
            id: id.clone(),
            title: work_title(kind, existing),
            kind,
            session_id,
            data: WorkData::for_kind(kind),
        });
        self.active_work_tab_id = Some(id.clone());
        self.load_active_tab();
        id
    }
}

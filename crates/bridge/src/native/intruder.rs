//! Intruder attacks and browser-automation crawler accessors.

use super::dto::*;
use super::*;

impl BackendIntruder for NativeBackend {
    fn intruder_start(&self, draft: IntruderDraft) -> BridgeResult<IntruderStatus> {
        let config = Self::draft_to_attack(&draft)?;
        let _enter = self.tokio.enter();
        let attack_id =
            begin_invoker_attack(self.runtime.app.clone(), config).map_err(BridgeError::message)?;
        *self.active_attack.lock() = Some(attack_id.clone());
        let mode_label = draft.mode.label();
        Ok(IntruderStatus {
            running: true,
            attack_id: Some(attack_id),
            message: Some(format!("Attack started ({mode_label})")),
            ..IntruderStatus::default()
        })
    }

    fn intruder_stop(&self) -> BridgeResult<IntruderStatus> {
        let attack_id = self.active_attack.lock().take();
        if let Some(attack_id) = attack_id {
            let state = self.runtime.app.state::<InvokerState>();
            cancel_invoker_attack(&state, &attack_id);
            Ok(IntruderStatus {
                running: false,
                attack_id: Some(attack_id),
                message: Some("Attack cancelled".into()),
                ..IntruderStatus::default()
            })
        } else {
            Ok(IntruderStatus {
                running: false,
                attack_id: None,
                message: Some("No active attack".into()),
                ..IntruderStatus::default()
            })
        }
    }

    fn crawler_start_crawl(&self, config: CrawlerCrawlConfig) -> BridgeResult<CrawlerSessionDto> {
        let _enter = self.tokio.enter();
        let crawl = to_backend_crawl_config(config);
        let state = self.runtime.app.state::<CrawlerState>();
        let session =
            crawler_start_crawl(self.runtime.app.clone(), state.inner(), crawl, None, None)
                .map_err(BridgeError::message)?;
        Ok(session_to_dto(session))
    }

    fn crawler_pause_crawl(&self, session_id: &str) -> BridgeResult<()> {
        let state = self.runtime.app.state::<CrawlerState>();
        crawler_pause_crawl(&self.runtime.app, state.inner(), session_id)
            .map_err(BridgeError::message)
    }

    fn crawler_resume_crawl(&self, session_id: &str) -> BridgeResult<()> {
        let state = self.runtime.app.state::<CrawlerState>();
        crawler_resume_crawl(&self.runtime.app, state.inner(), session_id)
            .map_err(BridgeError::message)
    }

    fn crawler_stop_crawl(&self, session_id: &str) -> BridgeResult<()> {
        let state = self.runtime.app.state::<CrawlerState>();
        crawler_stop_crawl(&self.runtime.app, state.inner(), session_id)
            .map_err(BridgeError::message)
    }

    fn crawler_delete_session(&self, session_id: &str) -> BridgeResult<()> {
        let state = self.runtime.app.state::<CrawlerState>();
        delete_crawler_session(&self.runtime.app, state.inner(), session_id)
            .map_err(BridgeError::message)
    }

    fn crawler_list_pages(&self, session_id: &str) -> BridgeResult<Vec<CrawlerPageDto>> {
        let state = self.runtime.app.state::<CrawlerState>();
        list_crawler_pages(&self.runtime.app, state.inner(), session_id)
            .map(|pages| pages.into_iter().map(page_to_dto).collect())
            .map_err(BridgeError::message)
    }

    fn crawler_list_insights(&self, session_id: &str) -> BridgeResult<Vec<CrawlerInsightDto>> {
        let state = self.runtime.app.state::<CrawlerState>();
        list_crawler_insights(&self.runtime.app, state.inner(), session_id)
            .map(|items| items.into_iter().map(insight_to_dto).collect())
            .map_err(BridgeError::message)
    }

    fn crawler_list_logs(&self, session_id: &str) -> BridgeResult<Vec<CrawlerLogDto>> {
        let state = self.runtime.app.state::<CrawlerState>();
        list_crawler_logs(&self.runtime.app, state.inner(), session_id)
            .map(|items| items.into_iter().map(log_to_dto).collect())
            .map_err(BridgeError::message)
    }
}

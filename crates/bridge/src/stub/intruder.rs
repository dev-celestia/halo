//! Stub intruder attacks and crawler state.

use super::*;

impl BackendIntruder for StubBackend {
    fn intruder_start(&self, draft: IntruderDraft) -> BridgeResult<IntruderStatus> {
        if !draft.url.contains('§')
            && !draft.headers_text.contains('§')
            && !draft.body.contains('§')
        {
            return Err(BridgeError::message(
                "Add at least one payload position with § markers",
            ));
        }
        let id = format!("stub-attack-{}", now_stamp());
        *lock_err(self.attack.lock()) = Some(id.clone());
        Ok(IntruderStatus {
            running: true,
            attack_id: Some(id),
            message: Some(format!("Stub attack started ({})", draft.mode.label())),
            ..IntruderStatus::default()
        })
    }

    fn intruder_stop(&self) -> BridgeResult<IntruderStatus> {
        let id = lock_err(self.attack.lock()).take();
        Ok(IntruderStatus {
            running: false,
            attack_id: id,
            message: Some("Stub attack stopped".into()),
            ..IntruderStatus::default()
        })
    }

    fn crawler_start_crawl(&self, config: CrawlerCrawlConfig) -> BridgeResult<CrawlerSessionDto> {
        Ok(CrawlerSessionDto {
            id: format!("stub-crawl-{}", now_stamp()),
            target_url: config.target_url,
            status: "running".into(),
            strategy: config.strategy,
            max_depth: config.max_depth,
            max_pages: config.max_pages,
            started_at: Some(now_stamp()),
            finished_at: None,
        })
    }

    fn crawler_pause_crawl(&self, _session_id: &str) -> BridgeResult<()> {
        Ok(())
    }

    fn crawler_resume_crawl(&self, _session_id: &str) -> BridgeResult<()> {
        Ok(())
    }

    fn crawler_stop_crawl(&self, _session_id: &str) -> BridgeResult<()> {
        Ok(())
    }

    fn crawler_delete_session(&self, _session_id: &str) -> BridgeResult<()> {
        Ok(())
    }

    fn crawler_list_pages(&self, _session_id: &str) -> BridgeResult<Vec<CrawlerPageDto>> {
        Ok(Vec::new())
    }

    fn crawler_list_insights(&self, _session_id: &str) -> BridgeResult<Vec<CrawlerInsightDto>> {
        Ok(Vec::new())
    }

    fn crawler_list_logs(&self, _session_id: &str) -> BridgeResult<Vec<CrawlerLogDto>> {
        Ok(Vec::new())
    }
}

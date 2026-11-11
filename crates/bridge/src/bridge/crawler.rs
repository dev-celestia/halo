//! Browser-automation crawl control, insights and console filters.

use super::*;

impl Bridge {
    // ── Browser automation ─────────────────────────────────────────────────

    pub fn set_crawler_config(config: CrawlerCrawlConfig, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.crawler.config = config);
    }

    pub fn set_crawler_view_tab(tab: CrawlerViewTab, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.crawler.view_tab = tab);
    }

    pub fn set_crawler_config_open(open: bool, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.crawler.config_open = open);
    }

    pub fn set_crawler_severity_filter(filter: String, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.crawler.severity_filter = filter);
    }

    pub fn set_crawler_log_filter(filter: String, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.crawler.log_filter = filter);
    }

    pub fn select_crawler_insight(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.crawler.selected_insight_id = id);
    }

    pub fn toggle_crawler_insight_reviewed(id: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if let Some(insight) = s.crawler.insights.iter_mut().find(|i| i.id == id) {
                insight.reviewed = !insight.reviewed;
            }
        });
    }

    pub fn clear_crawler_console(cx: &mut App) {
        Self::state(cx).update(cx, |s| s.crawler.logs.clear());
    }

    pub fn start_crawler_crawl(config: CrawlerCrawlConfig, cx: &mut App) -> BridgeResult<()> {
        if !Self::state(cx).read(cx).proxy.running {
            let err = BridgeError::message(
                "The Halo proxy is not running. Start the proxy first, then retry the crawl.",
            );
            Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
            return Err(err);
        }
        let backend = Self::backend(cx)?;
        match backend.crawler_start_crawl(config.clone()) {
            Ok(session) => {
                Self::state(cx).update(cx, |s| {
                    s.crawler.config = config;
                    s.crawler.session = Some(session.clone());
                    s.crawler.pages.clear();
                    s.crawler.insights.clear();
                    s.crawler.logs.clear();
                    s.apply_ok_msg(format!("Crawl started for {}", session.target_url));
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn pause_crawler_crawl(cx: &mut App) -> BridgeResult<()> {
        let session_id = Self::state(cx)
            .read(cx)
            .crawler
            .session
            .as_ref()
            .map(|s| s.id.clone())
            .ok_or_else(|| BridgeError::message("No active crawl session"))?;
        let backend = Self::backend(cx)?;
        match backend.crawler_pause_crawl(&session_id) {
            Ok(()) => {
                Self::state(cx).update(cx, |s| {
                    if let Some(session) = s.crawler.session.as_mut() {
                        session.status = "paused".into();
                    }
                    s.apply_ok_msg("Crawl paused".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn resume_crawler_crawl(cx: &mut App) -> BridgeResult<()> {
        let session_id = Self::state(cx)
            .read(cx)
            .crawler
            .session
            .as_ref()
            .map(|s| s.id.clone())
            .ok_or_else(|| BridgeError::message("No active crawl session"))?;
        let backend = Self::backend(cx)?;
        match backend.crawler_resume_crawl(&session_id) {
            Ok(()) => {
                Self::state(cx).update(cx, |s| {
                    if let Some(session) = s.crawler.session.as_mut() {
                        session.status = "running".into();
                    }
                    s.apply_ok_msg("Crawl resumed".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn stop_crawler_crawl(cx: &mut App) -> BridgeResult<()> {
        let session_id = Self::state(cx)
            .read(cx)
            .crawler
            .session
            .as_ref()
            .map(|s| s.id.clone())
            .ok_or_else(|| BridgeError::message("No active crawl session"))?;
        let backend = Self::backend(cx)?;
        match backend.crawler_stop_crawl(&session_id) {
            Ok(()) => {
                Self::state(cx).update(cx, |s| {
                    if let Some(session) = s.crawler.session.as_mut() {
                        session.status = "stopped".into();
                    }
                    s.apply_ok_msg("Crawl stopped".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    /// Push a backend event into UI state (for Track 2 worker threads).
    pub fn emit(event: BackendEvent, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.apply_event(event));
    }

    /// Refresh proxy snapshot from the backend without starting/stopping.
    pub fn refresh(cx: &mut App) -> BridgeResult<()> {
        Self::sync_proxy_from_backend(cx)?;
        let _ = Self::refresh_history(cx);
        let _ = Self::refresh_sessions(cx);
        let _ = Self::refresh_intercept(cx);
        let _ = Self::refresh_folders(cx);
        let _ = Self::refresh_storage(cx);
        Ok(())
    }
}

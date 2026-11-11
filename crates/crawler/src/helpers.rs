use crate::types::*;
use chrono::Utc;
use halo_core::tauri::{AppHandle, Emitter, Manager};
use halo_storage::HistoryBridge;

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn normalize_strategy(strategy: Option<String>) -> String {
    match strategy.as_deref().map(str::trim).map(str::to_lowercase) {
        Some(value) if value == "dfs" || value == "depth-first" || value == "depthfirst" => {
            "dfs".to_string()
        }
        _ => "bfs".to_string(),
    }
}

pub(crate) fn add_log(app: &AppHandle, state: &CrawlerState, log: ActivityLog) {
    {
        let mut logs = state.logs.lock();
        let session_logs = logs.entry(log.session_id.clone()).or_default();
        session_logs.push(log.clone());
        if session_logs.len() > 1_000 {
            let drain = session_logs.len() - 1_000;
            session_logs.drain(0..drain);
        }
    }
    if let Err(error) = app.state::<HistoryBridge>().insert_crawler_log(&log) {
        eprintln!("[crawler] failed to persist log: {error}");
    }
    let _ = app.emit("crawler:log-created", log);
}

pub(crate) fn persist_session(app: &AppHandle, session: &CrawlSession) {
    if let Err(error) = app.state::<HistoryBridge>().upsert_crawler_session(session) {
        eprintln!("[crawler] failed to persist session: {error}");
    }
}

pub(crate) fn persist_page(app: &AppHandle, page: &CrawlPage) {
    if let Err(error) = app.state::<HistoryBridge>().upsert_crawler_page(page) {
        eprintln!("[crawler] failed to persist page: {error}");
    }
}

pub(crate) fn persist_insight(app: &AppHandle, insight: &AIInsight) {
    if let Err(error) = app.state::<HistoryBridge>().insert_crawler_insight(insight) {
        eprintln!("[crawler] failed to persist insight: {error}");
    }
}

pub(crate) fn update_session(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
    status: &str,
    finished_at: Option<String>,
) -> Result<CrawlSession, String> {
    let updated = {
        let mut sessions = state.sessions.lock();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| "Automation session not found".to_string())?;

        session.status = status.to_string();
        if finished_at.is_some() {
            session.finished_at = finished_at;
        }
        session.clone()
    };

    persist_session(app, &updated);
    let _ = app.emit("crawler:session-updated", &updated);
    Ok(updated)
}

pub(crate) fn session_status(state: &CrawlerState, session_id: &str) -> Option<String> {
    state
        .sessions
        .lock()
        .get(session_id)
        .map(|session| session.status.clone())
}

pub(crate) fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "stopped")
}

pub(crate) fn upsert_page_memory(state: &CrawlerState, page: CrawlPage) {
    let mut pages = state.pages.lock();
    let session_pages = pages.entry(page.session_id.clone()).or_default();
    if let Some(existing) = session_pages.iter_mut().find(|item| item.id == page.id) {
        *existing = page;
    } else {
        session_pages.push(page);
        if session_pages.len() > 2_000 {
            let drain = session_pages.len() - 2_000;
            session_pages.drain(0..drain);
        }
    }
}

//! AI browser crawl commands (agent-browser CLI deferred).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use celestia_spider::CrawlControl;
use uuid::Uuid;

use crate::helpers::{
    add_log, is_terminal_status, normalize_strategy, now, persist_session, session_status,
    update_session,
};
use crate::runner::run_crawler_crawl;
use halo_core::tauri::{AppHandle, Emitter, Manager, async_runtime};
use halo_storage::HistoryBridge;

pub use crate::types::{
    AIInsight, ActivityLog, CrawlConfig, CrawlPage, CrawlSession, CrawlerState,
};

fn active_crawler_worker_count(state: &CrawlerState, session_id: &str) -> usize {
    state
        .controls
        .lock()
        .get(session_id)
        .map(|items| items.len())
        .unwrap_or(0)
}

fn cancel_session_controls(state: &CrawlerState, session_id: &str) {
    let controls = state.controls.lock().get(session_id).cloned();
    if let Some(controls) = controls {
        for control in controls.values() {
            control.cancel();
        }
    }
}

fn remove_session_controls(state: &CrawlerState, session_id: &str) {
    state.controls.lock().remove(session_id);
}

/// Start a crawl session (requires an active MITM proxy).
pub fn crawler_start_crawl(
    app: AppHandle,
    state: &CrawlerState,
    config: CrawlConfig,
    session_id: Option<String>,
    append_existing: Option<bool>,
) -> Result<CrawlSession, String> {
    let proxy_port = halo_traffic::active_proxy_port().unwrap_or(0);
    if proxy_port == 0 {
        return Err(
            "The Halo proxy is not running. Start the proxy first, then retry the crawl."
                .to_string(),
        );
    }

    let session_id = session_id.unwrap_or_else(|| format!("crawl-{}", Uuid::new_v4()));
    let session_exists_in_memory = state.sessions.lock().contains_key(&session_id);
    let appending_existing_session = append_existing.unwrap_or(false) || session_exists_in_memory;

    let session = CrawlSession {
        id: session_id,
        target_url: config.target_url.clone(),
        status: "running".to_string(),
        strategy: normalize_strategy(config.strategy.clone()),
        max_depth: config.max_depth,
        max_pages: config.max_pages,
        started_at: Some(now()),
        finished_at: None,
    };

    {
        let mut sessions = state.sessions.lock();
        sessions.insert(session.id.clone(), session.clone());
    }
    if !appending_existing_session {
        state.pages.lock().insert(session.id.clone(), Vec::new());
        state.insights.lock().insert(session.id.clone(), Vec::new());
        state.logs.lock().insert(session.id.clone(), Vec::new());
    }

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let worker_id = format!("worker-{}", Uuid::new_v4());
    state
        .cancellations
        .lock()
        .entry(session.id.clone())
        .or_default()
        .insert(worker_id.clone(), cancel_flag.clone());
    let crawl_control: Arc<CrawlControl> = Arc::new(CrawlControl::default());
    state
        .controls
        .lock()
        .entry(session.id.clone())
        .or_default()
        .insert(worker_id.clone(), crawl_control.clone());

    persist_session(&app, &session);
    if appending_existing_session {
        let _ = app.emit("crawler:session-updated", &session);
    } else {
        let _ = app.emit("crawler:session-started", &session);
    }
    add_log(
        &app,
        state,
        ActivityLog {
            id: Uuid::new_v4().to_string(),
            session_id: session.id.clone(),
            level: "info".to_string(),
            r#type: "session".to_string(),
            message: if appending_existing_session {
                format!(
                    "Resumed crawl from {}",
                    config
                        .resume_from_url
                        .as_deref()
                        .unwrap_or(session.target_url.as_str())
                )
            } else {
                format!("Started for {}", session.target_url)
            },
            url: Some(
                config
                    .resume_from_url
                    .clone()
                    .unwrap_or_else(|| session.target_url.clone()),
            ),
            ai_used_for_analysis: None,
            created_at: now(),
            extra: None,
            human_input_request: None,
        },
    );

    if config.enable_ai_insights {
        add_log(
            &app,
            state,
            ActivityLog {
                id: Uuid::new_v4().to_string(),
                session_id: session.id.clone(),
                level: "info".to_string(),
                r#type: "ai".to_string(),
                message:
                    "LLM page analysis is not wired yet; using deterministic heuristic findings."
                        .to_string(),
                url: Some(session.target_url.clone()),
                ai_used_for_analysis: Some(false),
                created_at: now(),
                extra: None,
                human_input_request: None,
            },
        );
    }

    let app_for_task = app.clone();
    let state_for_task = state.clone();
    let session_id_for_task = session.id.clone();
    let worker_id_for_task = worker_id.clone();
    let cancel_flag_for_task = cancel_flag.clone();
    let control_for_task = crawl_control.clone();
    let target_url = config.target_url.clone();

    async_runtime::spawn(async move {
        let sidecar_result = run_crawler_crawl(
            app_for_task.clone(),
            state_for_task.clone(),
            config,
            session_id_for_task.clone(),
            worker_id_for_task.clone(),
            cancel_flag_for_task,
            control_for_task,
        )
        .await;

        {
            let mut cancellations = state_for_task.cancellations.lock();
            if let Some(session_cancellations) = cancellations.get_mut(&session_id_for_task) {
                session_cancellations.remove(&worker_id_for_task);
                if session_cancellations.is_empty() {
                    cancellations.remove(&session_id_for_task);
                }
            }
        }
        {
            let mut controls = state_for_task.controls.lock();
            if let Some(session_controls) = controls.get_mut(&session_id_for_task) {
                session_controls.remove(&worker_id_for_task);
                if session_controls.is_empty() {
                    controls.remove(&session_id_for_task);
                }
            }
        }

        if let Err(error) = sidecar_result {
            if session_status(&state_for_task, &session_id_for_task)
                .as_deref()
                .map(is_terminal_status)
                .unwrap_or(false)
            {
                return;
            }
            add_log(
                &app_for_task,
                &state_for_task,
                ActivityLog {
                    id: Uuid::new_v4().to_string(),
                    session_id: session_id_for_task.clone(),
                    level: "error".to_string(),
                    r#type: "error".to_string(),
                    message: format!("Crawl worker failed: {error}"),
                    url: None,
                    ai_used_for_analysis: None,
                    created_at: now(),
                    extra: None,
                    human_input_request: None,
                },
            );
            if active_crawler_worker_count(&state_for_task, &session_id_for_task) > 0 {
                return;
            }
            let finished_at = now();
            let _ = update_session(
                &app_for_task,
                &state_for_task,
                &session_id_for_task,
                "failed",
                Some(finished_at),
            );
            let _ = app_for_task.emit(
                "crawler:session-failed",
                serde_json::json!({ "sessionId": session_id_for_task, "message": error }),
            );
        } else if !session_status(&state_for_task, &session_id_for_task)
            .as_deref()
            .map(is_terminal_status)
            .unwrap_or(false)
            && active_crawler_worker_count(&state_for_task, &session_id_for_task) == 0
        {
            let finished_at = now();
            if let Ok(session) = update_session(
                &app_for_task,
                &state_for_task,
                &session_id_for_task,
                "completed",
                Some(finished_at),
            ) {
                let _ = app_for_task.emit("crawler:session-finished", &session);

                let insights = state_for_task
                    .insights
                    .lock()
                    .get(&session_id_for_task)
                    .cloned()
                    .unwrap_or_default();
                let pages = state_for_task
                    .pages
                    .lock()
                    .get(&session_id_for_task)
                    .cloned()
                    .unwrap_or_default();

                crate::automation::ingest_scan_completed(
                    &app_for_task,
                    &session,
                    &pages,
                    &insights,
                );

                let insight_titles: Vec<String> = insights
                    .iter()
                    .map(|i| format!("{}: {}", i.severity, i.title))
                    .collect();
                let page_urls: Vec<&str> = pages
                    .iter()
                    .filter_map(|p| {
                        if p.status == "visited" || p.status == "interesting" {
                            Some(p.url.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();

                let _ = app_for_task.emit(
                    "ai-chat:crawl-completed",
                    serde_json::json!({
                        "sessionId": session_id_for_task,
                        "targetUrl": target_url,
                        "status": "completed",
                        "pagesVisited": pages.len(),
                        "insightsFound": insights.len(),
                        "insightTitles": insight_titles,
                        "pageUrls": page_urls,
                    }),
                );
            }
        }
    });

    Ok(session)
}

pub fn crawler_pause_crawl(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<(), String> {
    if session_status(state, session_id)
        .as_deref()
        .map(is_terminal_status)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let controls = state.controls.lock().get(session_id).cloned();
    if let Some(controls) = controls {
        for control in controls.values() {
            control.pause();
        }
    }

    update_session(app, state, session_id, "paused", None)?;
    add_log(
        app,
        state,
        ActivityLog {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            level: "info".to_string(),
            r#type: "session".to_string(),
            message: "Paused crawl".to_string(),
            url: None,
            ai_used_for_analysis: None,
            created_at: now(),
            extra: None,
            human_input_request: None,
        },
    );
    Ok(())
}

pub fn crawler_resume_crawl(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<(), String> {
    if session_status(state, session_id)
        .as_deref()
        .map(is_terminal_status)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let controls = state.controls.lock().get(session_id).cloned();
    if let Some(controls) = controls {
        for control in controls.values() {
            control.resume();
        }
    }

    update_session(app, state, session_id, "running", None)?;
    add_log(
        app,
        state,
        ActivityLog {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            level: "info".to_string(),
            r#type: "session".to_string(),
            message: "Resumed crawl".to_string(),
            url: None,
            ai_used_for_analysis: None,
            created_at: now(),
            extra: None,
            human_input_request: None,
        },
    );
    Ok(())
}

pub fn crawler_stop_crawl(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<(), String> {
    if let Some(cancel_flags) = state.cancellations.lock().remove(session_id) {
        for cancel_flag in cancel_flags.values() {
            cancel_flag.store(true, Ordering::SeqCst);
        }
    }

    cancel_session_controls(state, session_id);
    remove_session_controls(state, session_id);

    update_session(app, state, session_id, "stopped", Some(now()))?;
    add_log(
        app,
        state,
        ActivityLog {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            level: "warning".to_string(),
            r#type: "session".to_string(),
            message: "Stopped crawl".to_string(),
            url: None,
            ai_used_for_analysis: None,
            created_at: now(),
            extra: None,
            human_input_request: None,
        },
    );
    Ok(())
}

pub fn stop_all_active_crawls(app: &AppHandle, state: &CrawlerState) {
    let mut active_sessions: HashSet<String> = {
        let sessions = state.sessions.lock();
        sessions
            .values()
            .filter(|s| s.status == "running" || s.status == "paused")
            .map(|s| s.id.clone())
            .collect()
    };

    {
        let controls = state.controls.lock();
        active_sessions.extend(controls.keys().cloned());
    }

    for session_id in &active_sessions {
        {
            let mut cancellations = state.cancellations.lock();
            if let Some(cancel_flags) = cancellations.remove(session_id) {
                for flag in cancel_flags.values() {
                    flag.store(true, Ordering::SeqCst);
                }
            }
        }

        cancel_session_controls(state, session_id);
        remove_session_controls(state, session_id);
        let _ = update_session(app, state, session_id, "stopped", Some(now()));
        add_log(
            app,
            state,
            ActivityLog {
                id: Uuid::new_v4().to_string(),
                session_id: session_id.clone(),
                level: "warning".to_string(),
                r#type: "session".to_string(),
                message: "Crawl stopped because the app was closed.".to_string(),
                url: None,
                ai_used_for_analysis: None,
                created_at: now(),
                extra: None,
                human_input_request: None,
            },
        );
    }
}

pub fn delete_crawler_session(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<(), String> {
    if let Some(cancel_flags) = state.cancellations.lock().remove(session_id) {
        for cancel_flag in cancel_flags.values() {
            cancel_flag.store(true, Ordering::SeqCst);
        }
    }

    cancel_session_controls(state, session_id);
    remove_session_controls(state, session_id);

    state.sessions.lock().remove(session_id);
    state.pages.lock().remove(session_id);
    state.insights.lock().remove(session_id);
    state.logs.lock().remove(session_id);

    let history = app.state::<HistoryBridge>();
    for page in history.list_crawler_pages(session_id)? {
        for artifact_path in [page.screenshot_path, page.rendered_html_path]
            .into_iter()
            .flatten()
        {
            let path = PathBuf::from(artifact_path);
            if path.is_file() {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    history.delete_crawler_session(session_id)?;
    Ok(())
}

pub fn get_crawler_session(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<CrawlSession, String> {
    let history = app.state::<HistoryBridge>();
    if let Some(session) = history.get_crawler_session(session_id)? {
        return Ok(session);
    }

    state
        .sessions
        .lock()
        .get(session_id)
        .cloned()
        .ok_or_else(|| "Automation session not found".to_string())
}

pub fn list_crawler_pages(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<Vec<CrawlPage>, String> {
    let history = app.state::<HistoryBridge>();
    let persisted = history.list_crawler_pages(session_id)?;
    if !persisted.is_empty() {
        return Ok(persisted);
    }

    Ok(state
        .pages
        .lock()
        .get(session_id)
        .cloned()
        .unwrap_or_default())
}

pub fn list_crawler_insights(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<Vec<AIInsight>, String> {
    let history = app.state::<HistoryBridge>();
    let persisted = history.list_crawler_insights(session_id)?;
    if !persisted.is_empty() {
        return Ok(persisted);
    }

    Ok(state
        .insights
        .lock()
        .get(session_id)
        .cloned()
        .unwrap_or_default())
}

pub fn list_crawler_logs(
    app: &AppHandle,
    state: &CrawlerState,
    session_id: &str,
) -> Result<Vec<ActivityLog>, String> {
    let history = app.state::<HistoryBridge>();
    let persisted = history.list_crawler_logs(session_id)?;
    if !persisted.is_empty() {
        return Ok(persisted);
    }

    Ok(state
        .logs
        .lock()
        .get(session_id)
        .cloned()
        .unwrap_or_default())
}

pub fn list_recent_crawler_sessions(
    app: &AppHandle,
    limit: Option<u32>,
) -> Result<Vec<CrawlSession>, String> {
    app.state::<HistoryBridge>()
        .list_recent_crawler_sessions(limit.unwrap_or(20))
}

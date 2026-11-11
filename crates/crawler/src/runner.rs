//! Headless crawl engine powered by `celestia-spider`.
//!
//! LLM page analysis is deferred until AI settings/keyring are wired; interesting
//! pages still produce deterministic heuristic insights.

use crate::helpers::{add_log, now, persist_insight, persist_page, upsert_page_memory};
use crate::types::{AIInsight, ActivityLog, CrawlConfig, CrawlPage, CrawlerState};
use celestia_spider::{
    CrawlControl, CrawlResult, CrawlerEvent, Options as SpiderOptions, Runner, Strategy,
};
use celestia_spider::{extract_links, transform_html_to_ir};
use halo_core::tauri::{AppHandle, Emitter, Manager, async_runtime};
use halo_storage::HistoryBridge;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

/// url -> the page it was discovered from.
type ParentMap = Arc<Mutex<HashMap<String, String>>>;

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let truncated: String = value.chars().take(max).collect();
        format!("{truncated}...")
    }
}

/// Deterministic interesting-page heuristic (doc §2.2 fallback).
fn is_interesting_page(url: &str, markdown: &str) -> bool {
    let haystack = format!("{url}\n{markdown}").to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "/api/", "/v1/", "/v2/", "graphql", "login", "signin", "sign-in", "auth", "password",
        "register", "/admin", "/config", ".env", "swagger", "openapi", "token", "api-key",
        "apikey",
    ];
    MARKERS.iter().any(|m| haystack.contains(m)) || markdown.contains("# ")
}

fn map_strategy(config: &CrawlConfig) -> Strategy {
    match config
        .strategy
        .as_deref()
        .map(str::trim)
        .map(str::to_lowercase)
    {
        Some(value) if value == "dfs" || value == "depth-first" || value == "depthfirst" => {
            Strategy::DepthFirst
        }
        _ => Strategy::BreadthFirst,
    }
}

fn build_spider_options(config: &CrawlConfig, seed_url: &str) -> SpiderOptions {
    let mut options = SpiderOptions::with_defaults();
    options.urls = vec![seed_url.to_string()];
    options.max_depth = config.max_depth.max(1) as i32;
    options.strategy = map_strategy(config);
    options.headless = config.headless;
    options.stealth = true;
    options.timeout = (config.timeout_ms / 1000).max(1);
    options.retries = 1;
    if let Some(ref exclude) = config.exclude_paths {
        options.exclude = exclude
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect();
    }
    options.max_domain_pages = config.max_pages as usize;
    let delay_ms = config.request_delay_ms;
    if delay_ms >= 1000 {
        options.delay = delay_ms / 1000;
    } else if delay_ms > 0 {
        options.rate_limit = 1000_u64.checked_div(delay_ms).unwrap_or(1).max(1) as usize;
    }
    if let Some(proxy_port) = halo_traffic::active_proxy_port() {
        options.proxy = format!("http://127.0.0.1:{proxy_port}");
    }
    options.form_extraction = true;
    options.tech_detect = true;
    options
}

fn heuristic_finding_type(url: &str, markdown: &str) -> &'static str {
    let hay = format!("{url}\n{markdown}").to_ascii_lowercase();
    if hay.contains("/api/") || hay.contains("graphql") || hay.contains("swagger") {
        "api-route"
    } else if hay.contains("login") || hay.contains("auth") || hay.contains("password") {
        "auth-endpoint"
    } else if hay.contains("/admin") || hay.contains("/config") || hay.contains(".env") {
        "sensitive-path"
    } else {
        "interesting-page"
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_crawler_crawl(
    app: AppHandle,
    state: CrawlerState,
    config: CrawlConfig,
    session_id: String,
    _worker_id: String,
    cancel_flag: Arc<AtomicBool>,
    control: Arc<CrawlControl>,
) -> Result<(), String> {
    if cancel_flag.load(Ordering::SeqCst) || control.is_cancelled() {
        return Ok(());
    }

    let seed_url = config
        .resume_from_url
        .clone()
        .unwrap_or_else(|| config.target_url.clone());

    add_log(
        &app,
        &state,
        ActivityLog {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            level: "info".to_string(),
            r#type: "session".to_string(),
            message: format!(
                "Initializing celestia-spider engine ({} engine) for {}",
                if config.headless {
                    "headless"
                } else {
                    "standard"
                },
                seed_url
            ),
            url: Some(seed_url.clone()),
            ai_used_for_analysis: Some(false),
            created_at: now(),
            extra: None,
            human_input_request: None,
        },
    );

    let parent_map: ParentMap = Arc::new(Mutex::new(HashMap::new()));
    let mut options = build_spider_options(&config, &seed_url);

    let enable_ai_insights = config.enable_ai_insights;
    let result_app = app.clone();
    let result_state = state.clone();
    let result_session = session_id.clone();
    let result_cancel = cancel_flag.clone();
    let result_parents = parent_map.clone();
    options.on_result = Some(Box::new(move |result: &CrawlResult| {
        if result_cancel.load(Ordering::SeqCst) {
            return;
        }

        let current_now = now();

        let Some(response) = result.response.as_ref() else {
            let url = result
                .request
                .as_ref()
                .map(|request| request.url.clone())
                .unwrap_or_default();
            if !result.error.is_empty() {
                add_log(
                    &result_app,
                    &result_state,
                    ActivityLog {
                        id: Uuid::new_v4().to_string(),
                        session_id: result_session.clone(),
                        level: "warning".to_string(),
                        r#type: "error".to_string(),
                        message: format!("Failed to fetch {url}: {}", result.error),
                        url: Some(url),
                        ai_used_for_analysis: Some(false),
                        created_at: current_now,
                        extra: None,
                        human_input_request: None,
                    },
                );
            }
            return;
        };

        let url = if response.source.is_empty() {
            result
                .request
                .as_ref()
                .map(|request| request.url.clone())
                .unwrap_or_default()
        } else {
            response.source.clone()
        };
        if url.is_empty() {
            return;
        }

        let page_ir = transform_html_to_ir(&url, &response.body);
        let markdown = page_ir.markdown_ir;
        let title = page_ir.title;
        let interesting = is_interesting_page(&url, &markdown);
        let summary = truncate_chars(&markdown, 300);

        let page = CrawlPage {
            id: format!("page-{}", Uuid::new_v4()),
            session_id: result_session.clone(),
            url: url.clone(),
            title: if title.is_empty() {
                None
            } else {
                Some(title.clone())
            },
            status: "visited".to_string(),
            depth: response.depth.max(0) as u32,
            parent_url: result_parents.lock().get(&url).cloned(),
            http_status: u16::try_from(response.status_code.clamp(0, u16::MAX as i32)).ok(),
            links_found: extract_links(&url, &response.body)
                .map(|links| links.len() as u32)
                .unwrap_or(0),
            forms_found: response.forms.len() as u32,
            discovered_at: current_now.clone(),
            visited_at: Some(current_now.clone()),
            ai_summary: Some(summary),
            ai_used_for_analysis: Some(false),
            interesting: Some(interesting),
            screenshot_path: None,
            rendered_html_path: None,
        };

        upsert_page_memory(&result_state, page.clone());
        persist_page(&result_app, &page);
        let _ = result_app.emit("crawler:page-discovered", &page);
        let _ = result_app.emit("crawler:page-updated", &page);
        crate::automation::ingest_crawled_page(&result_app, &page);

        let tech_note = if response.technologies.is_empty() {
            String::new()
        } else {
            format!(" [{}]", response.technologies.join(", "))
        };
        add_log(
            &result_app,
            &result_state,
            ActivityLog {
                id: Uuid::new_v4().to_string(),
                session_id: result_session.clone(),
                level: "info".to_string(),
                r#type: "navigation".to_string(),
                message: format!(
                    "Crawled page: {} ({}){}",
                    url,
                    if title.is_empty() { "No Title" } else { &title },
                    tech_note
                ),
                url: Some(url.clone()),
                ai_used_for_analysis: Some(false),
                created_at: current_now.clone(),
                extra: None,
                human_input_request: None,
            },
        );

        if enable_ai_insights && interesting {
            let finding_type = heuristic_finding_type(&url, &markdown);
            let insight = AIInsight {
                id: Uuid::new_v4().to_string(),
                session_id: result_session.clone(),
                page_id: Some(page.id.clone()),
                severity: "info".to_string(),
                r#type: finding_type.to_string(),
                title: format!("Discovered Content: {url}"),
                description: format!(
                    "Extracted Page IR Title: {}",
                    if title.is_empty() { &url } else { &title }
                ),
                url: Some(url),
                ai_used_for_analysis: Some(false),
                analysis_source: Some("heuristic".to_string()),
                analysis_tool_id: None,
                analysis_tool_name: None,
                reviewed: false,
                created_at: current_now,
            };
            {
                let mut insights = result_state.insights.lock();
                let list = insights.entry(insight.session_id.clone()).or_default();
                list.push(insight.clone());
                if list.len() > 500 {
                    let drain = list.len() - 500;
                    list.drain(0..drain);
                }
            }
            persist_insight(&result_app, &insight);
            let _ = result_app.emit("crawler:insight-created", insight);
        }
    }));

    // Do not persist or emit a SQLite/UI log entry for every skipped URL:
    // a single page with hundreds of out-of-scope links would otherwise flood
    // `crawler_logs` and the UI event bus.
    options.on_skip_url = None;

    if let Err(error) = options.validate() {
        return Err(format!("Invalid crawl configuration: {error}"));
    }

    let mut runner = Runner::with_control(options, control.clone())
        .map_err(|error| format!("Failed to build celestia-spider runner: {error}"))?;

    let (events_tx, mut events_rx) = tokio::sync::broadcast::channel(512);
    runner.set_event_channel(events_tx);

    let event_app = app.clone();
    let event_state = state.clone();
    let event_session = session_id.clone();
    let event_parents = parent_map.clone();
    async_runtime::spawn(async move {
        loop {
            match events_rx.recv().await {
                Ok(event) => match event {
                    CrawlerEvent::LinkDiscovered { url, from, .. } => {
                        let is_new = {
                            let mut guard = event_parents.lock();
                            if guard.len() >= 5_000 && !guard.contains_key(&url) {
                                false
                            } else {
                                guard.insert(url.clone(), from.clone()).is_none()
                            }
                        };
                        if is_new {
                            if let Err(error) = event_app
                                .state::<HistoryBridge>()
                                .insert_crawler_edge(&event_session, &from, &url)
                            {
                                eprintln!("[crawler] failed to persist edge: {error}");
                            }
                        }
                    }
                    CrawlerEvent::Message { level, text } => {
                        add_log(
                            &event_app,
                            &event_state,
                            ActivityLog {
                                id: Uuid::new_v4().to_string(),
                                session_id: event_session.clone(),
                                level: match level.as_str() {
                                    "error" => "error".to_string(),
                                    "warning" => "warning".to_string(),
                                    "debug" => "debug".to_string(),
                                    _ => "info".to_string(),
                                },
                                r#type: "extraction".to_string(),
                                message: text,
                                url: None,
                                ai_used_for_analysis: Some(false),
                                created_at: now(),
                                extra: None,
                                human_input_request: None,
                            },
                        );
                    }
                    CrawlerEvent::Finished { summary } => {
                        add_log(
                            &event_app,
                            &event_state,
                            ActivityLog {
                                id: Uuid::new_v4().to_string(),
                                session_id: event_session.clone(),
                                level: "info".to_string(),
                                r#type: "session".to_string(),
                                message: format!(
                                    "Engine finished: {} pages, {} skipped, {} failed in {} ms",
                                    summary.results,
                                    summary.skipped,
                                    summary.failed,
                                    summary.duration_ms
                                ),
                                url: None,
                                ai_used_for_analysis: Some(false),
                                created_at: now(),
                                extra: None,
                                human_input_request: None,
                            },
                        );
                        break;
                    }
                    _ => {}
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    let summary = runner
        .run()
        .await
        .map_err(|error| format!("celestia-spider crawl failed: {error}"))?;

    let cancelled_note = if summary.cancelled {
        " (cancelled)"
    } else {
        ""
    };
    add_log(
        &app,
        &state,
        ActivityLog {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            level: "info".to_string(),
            r#type: "session".to_string(),
            message: format!(
                "Completed crawl: {} pages, {} skipped, {} failed{cancelled_note}",
                summary.results, summary.skipped, summary.failed
            ),
            url: Some(config.target_url),
            ai_used_for_analysis: Some(false),
            created_at: now(),
            extra: None,
            human_input_request: None,
        },
    );

    Ok(())
}

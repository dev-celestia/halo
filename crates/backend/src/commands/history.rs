//! HTTP history, documents, and WebSocket log commands.

use halo_core::types::{ProxyFilter, ProxyRecord, WebSocketFilter};
use halo_storage::db::repository::{
    DocumentRecord, PaginatedResponse, TreeNode,
};
use halo_storage::history::{
    ProxyLogSummary, WebSocketConnectionDetail, WebSocketConnectionSummary,
};

use crate::BackendRuntime;
use crate::commands::run_blocking;

// ── Proxy Logs ─────────────────────────────────────────────────────

pub async fn get_proxy_recent(
    ctx: &BackendRuntime,
    limit: Option<u32>,
    filter: Option<ProxyFilter>,
    sort_order: Option<String>,
) -> Result<Vec<ProxyLogSummary>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_recent(limit.unwrap_or(100), filter, sort_order)).await
}

pub async fn get_proxy_all(ctx: &BackendRuntime) -> Result<Vec<ProxyRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_all()).await
}

pub async fn get_proxy_filtered(
    ctx: &BackendRuntime,
    filter: ProxyFilter,
) -> Result<Vec<ProxyRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_filtered(filter)).await
}

pub async fn get_proxy_detail(
    ctx: &BackendRuntime,
    log_id: String,
) -> Result<ProxyRecord, String> {
    let history = ctx.history.clone();
    let lookup = log_id.clone();
    run_blocking(move || history.get_by_id(&lookup))
        .await?
        .ok_or_else(|| format!("Log not found: {}", log_id))
}

pub async fn get_proxy_tree(
    ctx: &BackendRuntime,
    filter: Option<ProxyFilter>,
) -> Result<Vec<TreeNode>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_tree(filter)).await
}

pub async fn delete_proxy_by_id(ctx: &BackendRuntime, log_id: String) -> Result<(), String> {
    if let Ok(id) = uuid::Uuid::parse_str(&log_id) {
        ctx.proxy_state.delete_record(&id);
    }
    let history = ctx.history.clone();
    run_blocking(move || history.delete_by_id(&log_id)).await
}

pub async fn clear_proxy_all(ctx: &BackendRuntime) -> Result<(), String> {
    ctx.proxy_state.clear_records();
    let history = ctx.history.clone();
    run_blocking(move || history.clear_all()).await
}

/// `keep_range` is one of `all` / `today` / `week` / `month` / `custom`.
pub async fn clear_proxy_by_date(
    ctx: &BackendRuntime,
    keep_range: String,
    custom_date: Option<String>,
) -> Result<usize, String> {
    let history = ctx.history.clone();

    if keep_range == "all" {
        ctx.proxy_state.clear_records();
        return run_blocking(move || {
            let _ = history.clear_websocket_all();
            history.clear_all()?;
            Ok(0)
        })
        .await;
    }

    let now = chrono::Utc::now();
    let cutoff = match keep_range.as_str() {
        "today" => {
            let start_of_day = now.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(start_of_day, chrono::Utc)
        }
        "week" => now - chrono::Duration::days(7),
        "month" => now - chrono::Duration::days(30),
        "custom" => {
            let date_str = custom_date
                .as_deref()
                .ok_or_else(|| "custom_date is required when keep_range is 'custom'".to_string())?;
            let naive_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .map_err(|e| format!("Invalid date format (expected YYYY-MM-DD): {}", e))?;
            let start_of_day = naive_date.and_hms_opt(0, 0, 0).unwrap_or_default();
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(start_of_day, chrono::Utc)
        }
        _ => return Err(format!("Unknown keep range: {}", keep_range)),
    };

    let cutoff_rfc3339 = cutoff.to_rfc3339();
    ctx.proxy_state.clear_records_before(&cutoff);
    run_blocking(move || history.clear_before(&cutoff_rfc3339)).await
}

// ── Documents ──────────────────────────────────────────────────────

pub async fn get_documents(ctx: &BackendRuntime) -> Result<Vec<DocumentRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_documents()).await
}

pub async fn save_document(
    ctx: &BackendRuntime,
    document: DocumentRecord,
) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.save_document(&document)).await
}

pub async fn delete_document(ctx: &BackendRuntime, document_id: String) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_document(&document_id)).await
}

// ── WebSocket ──────────────────────────────────────────────────────

pub async fn get_websocket_paginated(
    ctx: &BackendRuntime,
    page: u32,
    per_page: u32,
    filter: Option<WebSocketFilter>,
) -> Result<PaginatedResponse<WebSocketConnectionSummary>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_websocket_paginated(page, per_page, filter)).await
}

pub async fn get_websocket_detail(
    ctx: &BackendRuntime,
    connection_id: String,
) -> Result<WebSocketConnectionDetail, String> {
    let history = ctx.history.clone();
    let lookup = connection_id.clone();
    run_blocking(move || history.get_websocket_detail(&lookup))
        .await?
        .ok_or_else(|| format!("WebSocket connection not found: {}", connection_id))
}

pub async fn clear_websocket_all(ctx: &BackendRuntime) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.clear_websocket_all()).await
}

pub async fn delete_websocket_by_id(
    ctx: &BackendRuntime,
    connection_id: String,
) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_websocket_connection(&connection_id)).await
}

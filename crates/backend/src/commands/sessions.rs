//! HTTP capture-session commands.

use halo_storage::db::repository::{HttpSessionRecord, HttpSessionSummary};

use crate::BackendRuntime;
use crate::commands::run_blocking;

pub async fn list_http_sessions(ctx: &BackendRuntime) -> Result<Vec<HttpSessionSummary>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.list_http_sessions()).await
}

pub async fn get_active_http_session(
    ctx: &BackendRuntime,
) -> Result<Option<HttpSessionRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_active_http_session()).await
}

// The six filter arguments mirror the capture contract the UI and the CLI both
// expose; folding them into a struct would only move the naming problem.
#[allow(clippy::too_many_arguments)]
pub async fn create_http_session(
    ctx: &BackendRuntime,
    name: String,
    description: Option<String>,
    capture_mode: Option<String>,
    capture_filter: Option<String>,
    exclude_filter: Option<String>,
    storage_mode: Option<String>,
) -> Result<HttpSessionRecord, String> {
    let history = ctx.history.clone();
    let rec = run_blocking(move || {
        history.create_http_session(
            &name,
            description.as_deref(),
            capture_mode.as_deref(),
            capture_filter.as_deref(),
            exclude_filter.as_deref(),
            storage_mode.as_deref(),
        )
    })
    .await?;

    sync_session_filter_to_proxy_state(&rec, ctx);
    Ok(rec)
}

pub async fn promote_session(ctx: &BackendRuntime, session_id: String) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || {
        let payload_store = history
            .payload_store()
            .cloned()
            .ok_or_else(|| "payload store unavailable".to_string())?;
        let database = history.database().clone();
        crate::db::promote_session(&database, &payload_store, &session_id)
    })
    .await
}

pub async fn set_active_http_session(
    ctx: &BackendRuntime,
    session_id: String,
) -> Result<(), String> {
    let history = ctx.history.clone();
    let sess = run_blocking(move || {
        history.set_active_http_session(&session_id)?;
        history.get_active_http_session()
    })
    .await?;

    if let Some(sess) = sess {
        sync_session_filter_to_proxy_state(&sess, ctx);
    }
    Ok(())
}

pub async fn update_http_session_filter(
    ctx: &BackendRuntime,
    session_id: String,
    capture_mode: String,
    capture_filter: String,
    exclude_filter: String,
) -> Result<(), String> {
    let lookup = session_id.clone();
    let history = ctx.history.clone();
    let active = run_blocking(move || {
        history.update_http_session_filter(
            &lookup,
            &capture_mode,
            &capture_filter,
            &exclude_filter,
        )?;
        history.get_active_http_session()
    })
    .await?;

    if let Some(active) = active {
        if active.id == session_id {
            sync_session_filter_to_proxy_state(&active, ctx);
        }
    }
    Ok(())
}

pub async fn delete_http_session(ctx: &BackendRuntime, session_id: String) -> Result<(), String> {
    ctx.proxy_state.clear_records();
    let history = ctx.history.clone();
    run_blocking(move || history.delete_http_session(&session_id)).await
}

pub async fn rename_http_session(
    ctx: &BackendRuntime,
    session_id: String,
    name: String,
) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.rename_http_session(&session_id, &name)).await
}

pub async fn clear_http_session_logs(
    ctx: &BackendRuntime,
    session_id: String,
) -> Result<usize, String> {
    ctx.proxy_state.clear_records();
    let history = ctx.history.clone();
    run_blocking(move || history.clear_http_session_logs(&session_id)).await
}

/// Push a session's capture filter onto the live proxy, so the next request is
/// recorded (or skipped) without restarting it.
pub fn sync_session_filter_to_proxy_state(session: &HttpSessionRecord, ctx: &BackendRuntime) {
    let mode = match session.capture_mode.as_str() {
        "target_scope" => crate::proxy::types::ProxyRecordMode::TargetScope,
        "custom" => crate::proxy::types::ProxyRecordMode::Custom,
        _ => crate::proxy::types::ProxyRecordMode::All,
    };

    let custom_hosts: Vec<String> =
        serde_json::from_str(&session.capture_filter).unwrap_or_default();
    let exclude_hosts: Vec<String> =
        serde_json::from_str(&session.exclude_filter).unwrap_or_default();

    let mut config = ctx.proxy_state.get_db_filter_config();
    config.mode = mode;
    config.custom_hosts = custom_hosts;
    config.exclude_hosts = exclude_hosts;
    ctx.proxy_state.set_db_filter_config(config);
}

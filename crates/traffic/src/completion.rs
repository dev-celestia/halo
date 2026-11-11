use halo_core::tauri::{AppHandle, Emitter, Manager, async_runtime};
use halo_storage::{HistoryBridge, ProxyLogSummary};
use parking_lot::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

use super::lifecycle::Ctx;
use super::state::ProxyRecord;

/// Bounded so a slow SQLite writer cannot turn a traffic burst into unbounded
/// memory growth. A record can carry up to
/// [`super::lifecycle::MAX_RECORDED_BODY_BYTES`] of body, so this queue is the
/// only thing standing between a burst and the OOM killer.
const LOG_CHANNEL_CAPACITY: usize = 256;

static LOG_SENDER: OnceLock<mpsc::Sender<(ProxyRecord, Option<String>)>> = OnceLock::new();
/// Records dropped because the queue was full. Surfaced in logs so a user who
/// notices gaps in the history can tell why.
static DROPPED_RECORDS: AtomicU64 = AtomicU64::new(0);
// Shared buffer so an exit-time flush can drain records the worker hasn't
// persisted yet.
static LOG_BUFFER: Mutex<Vec<(ProxyRecord, Option<String>)>> = Mutex::new(Vec::new());

pub fn init_proxy_log_worker(app_handle: AppHandle) {
    let (tx, mut rx) = mpsc::channel::<(ProxyRecord, Option<String>)>(LOG_CHANNEL_CAPACITY);
    if LOG_SENDER.set(tx).is_err() {
        return;
    }

    async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(150));

        loop {
            tokio::select! {
                Some(item) = rx.recv() => {
                    LOG_BUFFER.lock().push(item);
                    if LOG_BUFFER.lock().len() >= 50 {
                        flush_log_buffer(&app_handle).await;
                    }
                }
                _ = interval.tick() => {
                    if !LOG_BUFFER.lock().is_empty() {
                        flush_log_buffer(&app_handle).await;
                    }
                }
            }
        }
    });
}

fn drain_log_buffer() -> Vec<(ProxyRecord, Option<String>)> {
    std::mem::take(&mut *LOG_BUFFER.lock())
}

/// Synchronously persists any buffered records. Called on app exit so
/// in-flight batches aren't dropped mid-write; blocking is acceptable there.
pub fn flush_proxy_log_buffer_on_exit(app_handle: &AppHandle) {
    let records = drain_log_buffer();
    if records.is_empty() {
        return;
    }
    insert_records(app_handle, &records);
}

async fn flush_log_buffer(app_handle: &AppHandle) {
    let records = drain_log_buffer();
    if records.is_empty() {
        return;
    }
    let app_handle = app_handle.clone();
    // SQLite + payload file writes are blocking; keep them off the async runtime.
    // Awaited by the worker so batches commit in arrival order.
    let flush = async_runtime::spawn_blocking(move || insert_records(&app_handle, &records));
    let _ = flush.await;
}

fn insert_records(app_handle: &AppHandle, records: &[(ProxyRecord, Option<String>)]) {
    if let Some(history) = app_handle.try_state::<HistoryBridge>() {
        if let Err(e) = history.insert_records_batch(records) {
            eprintln!("[completion] failed to batch insert to DB: {}", e);
        }
    }
}

pub fn build_record(ctx: &Ctx) -> ProxyRecord {
    // Slice before copying so we never allocate a full 16 MiB Vec just to truncate it to 2 MiB.
    let req_cap = ctx
        .req_body
        .len()
        .min(crate::lifecycle::MAX_RECORDED_BODY_BYTES);
    let res_cap = ctx
        .res_body
        .len()
        .min(crate::lifecycle::MAX_RECORDED_BODY_BYTES);
    ProxyRecord {
        id: ctx.transaction_id,
        timestamp: chrono::Utc::now(),
        client_addr: ctx.client_addr.clone(),
        server_addr: ctx.server_addr.clone(),
        request: super::state::ProxyRequest {
            method: ctx.req_method.clone(),
            uri: ctx.req_uri.clone(),
            http_version: ctx.req_http_version.clone(),
            headers: ctx.req_headers.clone(),
            body: ctx.req_body[..req_cap].to_vec(),
            content_decoded: ctx.req_content_decoded,
        },
        response: Some(super::state::ProxyResponse {
            status_code: ctx.res_status_code,
            status_text: ctx.res_status_text.clone(),
            http_version: ctx.res_http_version.clone(),
            headers: ctx.res_headers.clone(),
            body: ctx.res_body[..res_cap].to_vec(),
            content_decoded: ctx.res_content_decoded,
        }),
    }
}

pub async fn save_and_emit(ctx: &Ctx, app_handle: &AppHandle) {
    let txn = build_record(ctx);

    // ponytail: check if DB recording filter permits this record
    if let Some(proxy_state) = app_handle.try_state::<crate::state::ProxyState>() {
        if !proxy_state.should_record_to_db(&txn) {
            return;
        }
    }

    // SQLite read is blocking; keep it off the async runtime.
    let session_handle = app_handle.clone();
    let active_session = async_runtime::spawn_blocking(move || {
        session_handle
            .try_state::<HistoryBridge>()
            .and_then(|history| history.get_active_http_session().ok().flatten())
            .map(|s| s.id)
    })
    .await;
    let session_id_opt = active_session.unwrap_or(None);

    let summary = ProxyLogSummary {
        id: txn.id.to_string(),
        session_id: session_id_opt.clone().unwrap_or_default(),
        timestamp: txn.timestamp.to_rfc3339(),
        method: txn.request.method.clone(),
        url: txn.request.uri.clone(),
        response_status: txn.response.as_ref().map(|r| r.status_code),
        response_status_text: txn.response.as_ref().map(|r| r.status_text.clone()),
        response_content_type: txn.response.as_ref().and_then(|r| {
            r.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.clone())
        }),
        // Sizes reflect the full bodies held on the Ctx, before any truncation.
        request_body_size: ctx.req_body.len(),
        response_body_size: ctx.res_body.len(),
        server_addr: txn.server_addr.clone(),
        user_agent: txn
            .request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
            .map(|(_, v)| v.clone()),
        host: txn
            .request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case(":authority"))
            .map(|(_, v)| v.clone()),
    };

    crate::automation::ingest_proxy_record(app_handle, &txn);

    // Persist first (moved into the channel to avoid cloning full bodies),
    // then notify the UI from the already-built summary.
    match LOG_SENDER.get() {
        None => insert_direct(app_handle, txn, session_id_opt).await,
        Some(sender) => match sender.try_send((txn, session_id_opt)) {
            Ok(()) => {}
            // Best-effort persistence: dropping a record beats blocking the
            // proxy request path or growing memory without limit.
            Err(mpsc::error::TrySendError::Full(_)) => {
                let dropped = DROPPED_RECORDS.fetch_add(1, Ordering::Relaxed) + 1;
                if dropped == 1 || dropped % 100 == 0 {
                    eprintln!(
                        "[completion] record queue full ({LOG_CHANNEL_CAPACITY}); \
                         {dropped} record(s) not persisted (proxy keeps forwarding)"
                    );
                }
            }
            // Worker stopped (shutdown): fall back to a direct insert.
            Err(mpsc::error::TrySendError::Closed((txn, session_id_opt))) => {
                insert_direct(app_handle, txn, session_id_opt).await;
            }
        },
    }

    if let Err(e) = app_handle.emit("proxy-record", &summary) {
        eprintln!("[completion] failed to emit event: {}", e);
    }
}

/// Insert a record without the batching worker, off the async runtime.
async fn insert_direct(app_handle: &AppHandle, txn: ProxyRecord, session_id: Option<String>) {
    let app_handle = app_handle.clone();
    let fallback = async_runtime::spawn_blocking(move || {
        if let Some(history) = app_handle.try_state::<HistoryBridge>() {
            if let Err(e) = history.insert_record(&txn, session_id.as_deref()) {
                eprintln!("[completion] failed to insert to DB: {}", e);
            }
        }
    });
    let _ = fallback.await;
}

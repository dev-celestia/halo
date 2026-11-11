//! Collections / stashes / endpoints / contexts / chronicle — the saved-request
//! library. Pure SQLite passthroughs.

use crate::BackendRuntime;
use crate::commands::run_blocking;
use crate::{ChronicleLogRecord, CollectionRecord, ContextRecord, StashEndpointRecord, StashRecord};

// ── Collections ────────────────────────────────────────────────────

pub async fn get_collections(ctx: &BackendRuntime) -> Result<Vec<CollectionRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_collections()).await
}

pub async fn save_collection(
    ctx: &BackendRuntime,
    record: CollectionRecord,
) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.save_collection(&record)).await
}

pub async fn delete_collection(ctx: &BackendRuntime, id: String) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_collection(&id)).await
}

// ── Stashes ────────────────────────────────────────────────────────

pub async fn get_stashes(ctx: &BackendRuntime) -> Result<Vec<StashRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_stashes()).await
}

pub async fn save_stash(ctx: &BackendRuntime, record: StashRecord) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.save_stash(&record)).await
}

pub async fn delete_stash(ctx: &BackendRuntime, id: String) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_stash(&id)).await
}

// ── Stash endpoints ────────────────────────────────────────────────

pub async fn get_stash_endpoints(
    ctx: &BackendRuntime,
) -> Result<Vec<StashEndpointRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_stash_endpoints()).await
}

pub async fn save_stash_endpoint(
    ctx: &BackendRuntime,
    record: StashEndpointRecord,
) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.save_stash_endpoint(&record)).await
}

pub async fn delete_stash_endpoint(ctx: &BackendRuntime, id: String) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_stash_endpoint(&id)).await
}

// ── Contexts ───────────────────────────────────────────────────────

pub async fn get_contexts(ctx: &BackendRuntime) -> Result<Vec<ContextRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_contexts()).await
}

pub async fn save_context(ctx: &BackendRuntime, record: ContextRecord) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.save_context(&record)).await
}

pub async fn delete_context(ctx: &BackendRuntime, id: String) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_context(&id)).await
}

// ── Chronicle ──────────────────────────────────────────────────────

pub async fn get_chronicle_logs(
    ctx: &BackendRuntime,
    limit: u32,
) -> Result<Vec<ChronicleLogRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.get_chronicle_logs(limit)).await
}

pub async fn add_chronicle_log(
    ctx: &BackendRuntime,
    record: ChronicleLogRecord,
) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.add_chronicle_log(&record)).await
}

pub async fn clear_chronicle_logs(ctx: &BackendRuntime) -> Result<(), String> {
    let history = ctx.history.clone();
    run_blocking(move || history.clear_chronicle_logs()).await
}

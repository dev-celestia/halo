//! Notes commands.

use crate::BackendRuntime;
use crate::NoteRecord;
use crate::commands::run_blocking;

pub async fn list_notes(ctx: &BackendRuntime) -> Result<Vec<NoteRecord>, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.list_notes()).await
}

pub async fn upsert_note(ctx: &BackendRuntime, note: NoteRecord) -> Result<NoteRecord, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.upsert_note(&note)).await
}

pub async fn delete_note(ctx: &BackendRuntime, id: String) -> Result<usize, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_note(&id)).await
}

pub async fn delete_notes(ctx: &BackendRuntime, ids: Vec<String>) -> Result<usize, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.delete_notes(&ids)).await
}

pub async fn import_notes(ctx: &BackendRuntime, notes: Vec<NoteRecord>) -> Result<usize, String> {
    let history = ctx.history.clone();
    run_blocking(move || history.import_notes(&notes)).await
}

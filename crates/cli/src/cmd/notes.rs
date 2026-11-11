//! `halo-cli notes …`

use anyhow::Context as _;
use halo_backend::commands::notes as notes_cmd;

use crate::context::Ctx;
use crate::output::{Table, emit, emit_json};

pub async fn list(ctx: &Ctx) -> anyhow::Result<()> {
    let notes = notes_cmd::list_notes(ctx.ctx())
        .await
        .map_err(anyhow::Error::msg)?;

    let mut table = Table::new(&["ID", "NAME", "UPDATED", "NOTE"]);
    for note in &notes {
        table.row([
            note.id.clone(),
            note.name.clone(),
            note.updated_at.clone(),
            preview(&note.note),
        ]);
    }

    emit(ctx.json, &table, &notes)
}

pub async fn remove(ctx: &Ctx, id: &str) -> anyhow::Result<()> {
    let removed = notes_cmd::delete_note(ctx.ctx(), id.to_string())
        .await
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&serde_json::json!({ "deleted": id, "count": removed }))?;
    } else {
        println!("deleted {id}");
    }
    Ok(())
}

/// Imports a JSON array of note records — the same shape `notes ls --json` emits.
pub async fn import(ctx: &Ctx, file: &str) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(file).with_context(|| format!("read {file}"))?;
    let notes: Vec<halo_backend::NoteRecord> =
        serde_json::from_str(&raw).with_context(|| format!("parse {file}"))?;

    let count = notes_cmd::import_notes(ctx.ctx(), notes)
        .await
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&serde_json::json!({ "imported": count }))?;
    } else {
        println!("imported {count} notes");
    }
    Ok(())
}

fn preview(note: &str) -> String {
    let flat = note.replace(['\n', '\r'], " ");
    if flat.chars().count() > 60 {
        let head: String = flat.chars().take(60).collect();
        format!("{head}…")
    } else {
        flat
    }
}

//! `halo-cli session …`

use halo_backend::commands::sessions as session_cmd;

use crate::context::Ctx;
use crate::output::{Table, emit, emit_json};

pub async fn list(ctx: &Ctx) -> anyhow::Result<()> {
    let sessions = session_cmd::list_http_sessions(ctx.ctx())
        .await
        .map_err(anyhow::Error::msg)?;

    let mut table = Table::new(&["", "ID", "NAME", "MODE", "CREATED"]);
    for session in &sessions {
        table.row([
            if session.is_active { "*" } else { " " }.to_string(),
            session.id.clone(),
            session.name.clone(),
            session.capture_mode.clone(),
            session.created_at.clone(),
        ]);
    }

    emit(ctx.json, &table, &sessions)
}

pub async fn create(ctx: &Ctx, name: &str) -> anyhow::Result<()> {
    let record = session_cmd::create_http_session(
        ctx.ctx(),
        name.to_string(),
        None,
        Some("all".to_string()),
        None,
        None,
        Some("persistent".to_string()),
    )
    .await
    .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&record)?;
    } else {
        println!("created session {} ({})", record.name, record.id);
    }
    Ok(())
}

pub async fn activate(ctx: &Ctx, id: &str) -> anyhow::Result<()> {
    session_cmd::set_active_http_session(ctx.ctx(), id.to_string())
        .await
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&serde_json::json!({ "active": id }))?;
    } else {
        println!("active session is now {id}");
    }
    Ok(())
}

pub async fn clear(ctx: &Ctx, id: &str) -> anyhow::Result<()> {
    let removed = session_cmd::clear_http_session_logs(ctx.ctx(), id.to_string())
        .await
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&serde_json::json!({ "session": id, "removed": removed }))?;
    } else {
        println!("removed {removed} entries from {id}");
    }
    Ok(())
}

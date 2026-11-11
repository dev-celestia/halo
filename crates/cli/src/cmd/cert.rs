//! `halo-cli cert …` — the MITM CA.

use halo_backend::commands::settings;

use crate::context::Ctx;
use crate::output::emit_json;

pub fn show(ctx: &Ctx) -> anyhow::Result<()> {
    let pem = settings::get_ca_cert().map_err(anyhow::Error::msg)?;
    if ctx.json {
        emit_json(&serde_json::json!({ "pem": pem }))?;
    } else {
        print!("{pem}");
    }
    Ok(())
}

pub fn export(ctx: &Ctx, path: &str) -> anyhow::Result<()> {
    let written = settings::export_ca_cert(path).map_err(anyhow::Error::msg)?;
    if ctx.json {
        emit_json(&serde_json::json!({ "path": written }))?;
    } else {
        println!("wrote {written}");
    }
    Ok(())
}

pub fn trust(ctx: &Ctx) -> anyhow::Result<()> {
    let message = settings::trust_ca().map_err(anyhow::Error::msg)?;
    if ctx.json {
        emit_json(&serde_json::json!({ "message": message }))?;
    } else {
        println!("{message}");
    }
    Ok(())
}

pub fn regenerate(ctx: &Ctx) -> anyhow::Result<()> {
    settings::regenerate_ca_cert().map_err(anyhow::Error::msg)?;
    if ctx.json {
        emit_json(&serde_json::json!({ "regenerated": true }))?;
    } else {
        println!("CA regenerated — clients that trusted the old certificate must be updated");
    }
    Ok(())
}

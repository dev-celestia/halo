//! `halo-cli storage …` — disk usage, artifact pruning, full reset.

use halo_backend::commands::settings;

use crate::context::Ctx;
use crate::output::{Table, emit, emit_json, human_bytes};

pub fn info(ctx: &Ctx) -> anyhow::Result<()> {
    let info = settings::get_storage_info(ctx.ctx()).map_err(anyhow::Error::msg)?;

    let mut table = Table::new(&["ITEM", "SIZE", "PATH"]);
    table.row([
        "database".to_string(),
        human_bytes(info.database_size_bytes),
        info.database_path.clone(),
    ]);
    table.row([
        "crawler artifacts".to_string(),
        human_bytes(info.crawler_artifacts_size_bytes),
        info.crawler_artifacts_path.clone(),
    ]);
    table.row([
        "browser artifacts".to_string(),
        human_bytes(info.browser_artifacts_size_bytes),
        info.browser_artifacts_path.clone(),
    ]);
    table.row([
        "regression artifacts".to_string(),
        human_bytes(info.regression_artifacts_size_bytes),
        String::new(),
    ]);
    table.row([
        "log file".to_string(),
        human_bytes(info.log_file_size_bytes),
        String::new(),
    ]);
    table.row([
        "app data".to_string(),
        String::new(),
        info.app_data_dir.clone(),
    ]);

    emit(ctx.json, &table, &info)
}

pub fn prune(ctx: &Ctx, artifact: &str) -> anyhow::Result<()> {
    let result = settings::delete_storage_artifact(ctx.ctx(), artifact)
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&result)?;
    } else {
        println!("removed {} ({})", human_bytes(result.bytes_deleted), result.label);
    }
    Ok(())
}

pub async fn reset(ctx: &Ctx) -> anyhow::Result<()> {
    let result = settings::reset_all_app_data(ctx.ctx())
        .await
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&result)?;
    } else {
        println!(
            "reset complete — {} files, {} freed",
            result.files_deleted,
            human_bytes(result.bytes_deleted)
        );
        println!("backup: {}", result.artifact_dir);
    }
    Ok(())
}

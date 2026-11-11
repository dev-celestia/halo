//! `halo-cli library …` — collections, stashes, endpoints, contexts, chronicle.

use halo_backend::commands::library as library_cmd;

use crate::cli::LibraryKind;
use crate::context::Ctx;
use crate::output::{Table, emit};

pub async fn list(ctx: &Ctx, kind: LibraryKind, limit: u32) -> anyhow::Result<()> {
    match kind {
        LibraryKind::Collection => {
            let rows = library_cmd::get_collections(ctx.ctx())
                .await
                .map_err(anyhow::Error::msg)?;
            let mut table = Table::new(&["ID", "NAME", "UPDATED"]);
            for row in &rows {
                table.row([row.id.clone(), row.name.clone(), row.updated_at.clone()]);
            }
            emit(ctx.json, &table, &rows)
        }
        LibraryKind::Stash => {
            let rows = library_cmd::get_stashes(ctx.ctx())
                .await
                .map_err(anyhow::Error::msg)?;
            let mut table = Table::new(&["ID", "NAME", "COLLECTION", "UPDATED"]);
            for row in &rows {
                table.row([
                    row.id.clone(),
                    row.name.clone(),
                    row.collection_id.clone().unwrap_or_default(),
                    row.updated_at.clone(),
                ]);
            }
            emit(ctx.json, &table, &rows)
        }
        LibraryKind::Endpoint => {
            let rows = library_cmd::get_stash_endpoints(ctx.ctx())
                .await
                .map_err(anyhow::Error::msg)?;
            let mut table = Table::new(&["ID", "METHOD", "URL", "STASH"]);
            for row in &rows {
                table.row([
                    row.id.clone(),
                    row.method.clone(),
                    row.url.clone(),
                    row.stash_id.clone(),
                ]);
            }
            emit(ctx.json, &table, &rows)
        }
        LibraryKind::Context => {
            let rows = library_cmd::get_contexts(ctx.ctx())
                .await
                .map_err(anyhow::Error::msg)?;
            let mut table = Table::new(&["ID", "NAME", "UPDATED"]);
            for row in &rows {
                table.row([row.id.clone(), row.name.clone(), row.updated_at.clone()]);
            }
            emit(ctx.json, &table, &rows)
        }
        LibraryKind::Chronicle => {
            let rows = library_cmd::get_chronicle_logs(ctx.ctx(), limit)
                .await
                .map_err(anyhow::Error::msg)?;
            let mut table = Table::new(&["TIME", "METHOD", "STATUS", "URL"]);
            for row in &rows {
                table.row([
                    row.timestamp.clone(),
                    row.method.clone(),
                    row.response_status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".into()),
                    row.url.clone(),
                ]);
            }
            emit(ctx.json, &table, &rows)
        }
    }
}

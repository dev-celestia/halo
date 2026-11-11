//! `halo-cli scan` — TCP connect scan.

use anyhow::Context as _;
use halo_backend::commands::port_scan;

use crate::context::Ctx;
use crate::output::{Table, emit, emit_json};

pub async fn run(
    ctx: &Ctx,
    target: String,
    ports: String,
    timeout_ms: u64,
    concurrency: u32,
    banner: bool,
) -> anyhow::Result<()> {
    let ports = parse_ports(&ports)?;
    if ports.is_empty() {
        anyhow::bail!("no ports to scan");
    }

    let scan_id = uuid_like(&target);
    let results = port_scan::scan(
        ctx.ctx(),
        scan_id,
        target.clone(),
        ports,
        timeout_ms,
        concurrency,
        banner,
    )
    .await;

    // `run_scan` returns open ports only — closed and filtered are counted but
    // never materialised as rows.
    if ctx.json {
        return emit_json(&serde_json::json!({
            "target": target,
            "open": results.len(),
            "results": results,
        }));
    }

    let mut table = Table::new(&["PORT", "SERVICE", "TIME", "BANNER"]);
    for row in &results {
        table.row([
            row.port.to_string(),
            row.service.clone(),
            row.response_time_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "-".into()),
            row.banner.clone().unwrap_or_default(),
        ]);
    }

    println!("{target}: {} open", results.len());
    emit(false, &table, &results)
}

/// `80`, `443,8000-8100`, `1-1024`.
fn parse_ports(spec: &str) -> anyhow::Result<Vec<u16>> {
    let mut ports = Vec::new();
    for token in spec.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some((from, to)) = token.split_once('-') {
            let from: u16 = from
                .parse()
                .with_context(|| format!("bad port range {token}"))?;
            let to: u16 = to
                .parse()
                .with_context(|| format!("bad port range {token}"))?;
            if from > to {
                anyhow::bail!("port range {token} is reversed");
            }
            ports.extend(from..=to);
        } else {
            ports.push(
                token
                    .parse()
                    .with_context(|| format!("bad port {token}"))?,
            );
        }
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

/// The scanner only uses the id as an event key; a timestamp is enough.
fn uuid_like(target: &str) -> String {
    format!(
        "cli-{}-{}",
        target.replace(['.', ':', '/'], "-"),
        chrono::Utc::now().timestamp_millis()
    )
}

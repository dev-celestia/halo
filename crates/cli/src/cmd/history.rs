//! `halo-cli history …`

use anyhow::{Context as _, bail};
use halo_backend::ProxyFilter;
use halo_backend::commands::history as history_cmd;

use crate::context::Ctx;
use crate::output::{Table, emit, emit_json};

pub async fn ls(
    ctx: &Ctx,
    limit: u32,
    method: Option<String>,
    status: Option<String>,
    host: Option<String>,
    search: Option<String>,
) -> anyhow::Result<()> {
    let filter = build_filter(method, status, host, search)?;

    let rows = history_cmd::get_proxy_recent(ctx.ctx(), Some(limit), filter, Some("desc".into()))
        .await
        .map_err(anyhow::Error::msg)?;

    let mut table = Table::new(&["TIME", "METHOD", "STATUS", "SIZE", "URL"]);
    for row in &rows {
        table.row([
            short_time(&row.timestamp),
            row.method.clone(),
            row.response_status
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".into()),
            crate::output::human_bytes(row.response_body_size as u64),
            row.url.clone(),
        ]);
    }

    emit(ctx.json, &table, &rows)
}

pub async fn show(ctx: &Ctx, id: &str) -> anyhow::Result<()> {
    let record = history_cmd::get_proxy_detail(ctx.ctx(), id.to_string())
        .await
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        return emit_json(&record);
    }

    println!(
        "{} {} {}",
        record.request.method, record.request.uri, record.request.http_version
    );
    for (key, value) in sorted(&record.request.headers) {
        println!("{key}: {value}");
    }
    println!();

    match &record.response {
        Some(response) => {
            println!(
                "{} {} {}",
                response.http_version, response.status_code, response.status_text
            );
            for (key, value) in sorted(&response.headers) {
                println!("{key}: {value}");
            }
            println!();
            print_body(&response.body);
        }
        None => println!("(no response captured)"),
    }

    Ok(())
}

pub async fn remove(ctx: &Ctx, id: &str) -> anyhow::Result<()> {
    history_cmd::delete_proxy_by_id(ctx.ctx(), id.to_string())
        .await
        .map_err(anyhow::Error::msg)?;
    if ctx.json {
        emit_json(&serde_json::json!({ "deleted": id }))?;
    } else {
        println!("deleted {id}");
    }
    Ok(())
}

pub async fn clear(
    ctx: &Ctx,
    all: bool,
    before: Option<&str>,
    date: Option<&str>,
) -> anyhow::Result<()> {
    let window = match (all, before) {
        (true, _) => "all",
        (false, Some(window)) => window,
        (false, None) => bail!("pass --all, or --before <today|week|month|custom>"),
    };

    let removed = history_cmd::clear_proxy_by_date(
        ctx.ctx(),
        window.to_string(),
        date.map(str::to_string),
    )
    .await
    .map_err(anyhow::Error::msg)?;

    if ctx.json {
        emit_json(&serde_json::json!({ "window": window, "removed": removed }))?;
    } else if removed == 0 {
        println!("cleared ({window})");
    } else {
        println!("removed {removed} entries ({window})");
    }
    Ok(())
}

fn build_filter(
    method: Option<String>,
    status: Option<String>,
    host: Option<String>,
    search: Option<String>,
) -> anyhow::Result<Option<ProxyFilter>> {
    if method.is_none() && status.is_none() && host.is_none() && search.is_none() {
        return Ok(None);
    }

    Ok(Some(ProxyFilter {
        search,
        path: None,
        methods: method.map(|raw| {
            raw.split(',')
                .map(|part| part.trim().to_uppercase())
                .filter(|part| !part.is_empty())
                .collect()
        }),
        status_codes: status.map(|raw| parse_status_codes(&raw)).transpose()?,
        scope: host.map(|raw| {
            raw.split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect()
        }),
        session_id: None,
    }))
}

/// Accepts `404`, `4xx`, and `200-204` — comma separated.
fn parse_status_codes(raw: &str) -> anyhow::Result<Vec<u16>> {
    let mut codes = Vec::new();
    for token in raw.split(',') {
        let token = token.trim().to_lowercase();
        if token.is_empty() {
            continue;
        }
        if let Some(class) = token.strip_suffix("xx") {
            let base: u16 = class.parse().with_context(|| format!("bad status class {token}"))?;
            let start = base * 100;
            codes.extend(start..start + 100);
        } else if let Some((from, to)) = token.split_once('-') {
            let from: u16 = from.parse().with_context(|| format!("bad status range {token}"))?;
            let to: u16 = to.parse().with_context(|| format!("bad status range {token}"))?;
            codes.extend(from..=to);
        } else {
            codes.push(token.parse().with_context(|| format!("bad status code {token}"))?);
        }
    }
    Ok(codes)
}

fn short_time(timestamp: &str) -> String {
    // RFC3339: 2026-10-30T14:51:36.123Z → 14:51:36
    timestamp
        .split_once('T')
        .map(|(_, rest)| rest.chars().take(8).collect())
        .unwrap_or_else(|| timestamp.to_string())
}

fn sorted(headers: &std::collections::HashMap<String, String>) -> Vec<(&String, &String)> {
    let mut entries: Vec<_> = headers.iter().collect();
    entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    entries
}

fn print_body(body: &[u8]) {
    if body.is_empty() {
        println!("(empty body)");
        return;
    }
    const MAX: usize = 4000;
    let text = String::from_utf8_lossy(body);
    if text.len() > MAX {
        println!("{}", &text[..floor_boundary(&text, MAX)]);
        println!("… {} bytes total", body.len());
    } else {
        println!("{text}");
    }
}

fn floor_boundary(text: &str, max: usize) -> usize {
    let mut end = max.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

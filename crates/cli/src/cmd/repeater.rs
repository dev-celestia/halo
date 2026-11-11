//! `halo-cli repeater send` — send one crafted request and print the response.

use std::collections::HashMap;

use anyhow::Context as _;
use halo_backend::commands::repeater::{RepeaterRequest, send_repeater_request};

use crate::context::Ctx;
use crate::output::emit_json;

pub async fn send(
    ctx: &Ctx,
    raw: Option<String>,
    method: Option<String>,
    url: Option<String>,
    headers: Vec<String>,
    body: Option<String>,
) -> anyhow::Result<()> {
    let request = match raw {
        Some(path) => parse_raw(&read(&path)?)?,
        None => {
            let url = url.ok_or_else(|| anyhow::anyhow!("pass --url, or --raw <file>"))?;
            RepeaterRequest {
                method: method.unwrap_or_else(|| "GET".into()).to_uppercase(),
                url,
                headers: parse_headers(&headers)?,
                body: body.unwrap_or_default(),
            }
        }
    };

    let response = send_repeater_request(request)
        .await
        .map_err(anyhow::Error::msg)?;

    if ctx.json {
        return emit_json(&response);
    }

    println!(
        "{} {} ({} ms)",
        response.status, response.status_text, response.time_ms
    );
    println!("{}", response.final_url);
    println!();
    let mut entries: Vec<_> = response.headers.iter().collect();
    entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (key, value) in entries {
        println!("{key}: {value}");
    }
    println!();
    println!("{}", response.body);
    Ok(())
}

fn read(path: &str) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("read {path}"))
}

/// Parses a raw HTTP/1.x request file:
/// `METHOD URL HTTP/1.1`, headers, blank line, body.
fn parse_raw(text: &str) -> anyhow::Result<RepeaterRequest> {
    let (head, body) = match text.split_once("\r\n\r\n") {
        Some((head, body)) => (head, body),
        None => match text.split_once("\n\n") {
            Some((head, body)) => (head, body),
            None => (text, ""),
        },
    };

    let mut lines = head.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty request file"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_uppercase();
    let url = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("request line needs a URL: {request_line}"))?;

    let mut headers = HashMap::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(RepeaterRequest {
        method,
        url: url.to_string(),
        headers,
        body: body.trim_end_matches(['\r', '\n']).to_string(),
    })
}

fn parse_headers(raw: &[String]) -> anyhow::Result<HashMap<String, String>> {
    let mut headers = HashMap::new();
    for entry in raw {
        let (key, value) = entry
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("header must be `Name: value`, got {entry}"))?;
        headers.insert(key.trim().to_string(), value.trim().to_string());
    }
    Ok(headers)
}

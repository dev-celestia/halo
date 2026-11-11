//! HTTP table helpers — formatting and re-exports of shared semantic colors.

use halo_bridge::{HttpLogDetail, HttpLogRow};

pub use crate::pages::colors::{method_color, status_color};

/// Build a bash `curl` one-liner from a captured request (doc §3.3 Copy as cURL).
pub fn build_curl_command(detail: &HttpLogDetail) -> String {
    let mut parts = Vec::new();
    parts.push(format!(
        "curl -X {} {}",
        detail.method,
        shell_single_quote(&detail.url)
    ));

    let mut headers: Vec<_> = detail.req_headers.iter().collect();
    headers.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    for (name, value) in headers {
        if skip_curl_header(name) {
            continue;
        }
        parts.push(format!(
            "-H {}",
            shell_single_quote(&format!("{name}: {value}"))
        ));
    }

    if !detail.req_body.is_empty() {
        parts.push(format!(
            "--data-binary {}",
            shell_single_quote(&detail.req_body)
        ));
    }

    parts.join(" \\\n  ")
}

fn skip_curl_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "host"
            | ":authority"
            | ":method"
            | ":path"
            | ":scheme"
    )
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn short_mime(content_type: Option<&str>) -> String {
    let Some(ct) = content_type else {
        return "—".into();
    };
    let main = ct.split(';').next().unwrap_or(ct).trim();
    if let Some(sub) = main.split('/').nth(1) {
        let sub = sub.trim();
        if sub.len() > 14 {
            format!("{}…", &sub[..14])
        } else {
            sub.to_string()
        }
    } else {
        main.to_string()
    }
}

pub fn format_bytes(n: usize) -> String {
    crate::pages::chrome::format_bytes(n as u64)
}

pub fn format_time(ts: &str) -> String {
    // Prefer HH:MM:SS from RFC3339 / ISO timestamps.
    if let Some(t) = ts.split('T').nth(1) {
        let t = t.trim_end_matches('Z');
        if t.len() >= 8 {
            return t[..8].to_string();
        }
        return t.to_string();
    }
    if ts.len() > 12 {
        ts[ts.len().saturating_sub(12)..].to_string()
    } else {
        ts.to_string()
    }
}

/// Mark path under `assets/browser/` for the client that sent a request, if we
/// have a mark for it.
///
/// The traffic table draws this inline in the `Host` cell, so it reads
/// `[icon] host`. Edge, curl and unrecognised agents return `None` — there is
/// no asset for them, and a placeholder square would read as a logo.
pub fn browser_mark(row: &HttpLogRow) -> Option<&'static str> {
    let ua = row.user_agent.as_deref().unwrap_or("").to_ascii_lowercase();
    if ua.contains("edg/") {
        // Edge advertises `Chrome/…` too, so it has to be ruled out first.
        None
    } else if ua.contains("chrome") || ua.contains("chromium") {
        Some("browser/chrome.png")
    } else if ua.contains("firefox") {
        Some("browser/firefox.png")
    } else if ua.contains("safari") {
        Some("browser/safari.png")
    } else {
        None
    }
}

pub fn host_and_path(row: &HttpLogRow) -> (String, String) {
    let host = row
        .host
        .clone()
        .or_else(|| {
            row.url
                .split("://")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .map(|h| h.to_string())
        })
        .unwrap_or_else(|| "—".into());

    let path = if let Some(after) = row.url.split("://").nth(1) {
        let path = after.find('/').map(|i| &after[i..]).unwrap_or("/");
        path.to_string()
    } else if row.url.starts_with('/') {
        row.url.clone()
    } else {
        row.url.clone()
    };

    (host, path)
}

//! Assorted small helpers: timestamps, invoker parsing, intercept hosts, filter chips.

use super::*;

pub(crate) fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // RFC3339-ish UTC without pulling chrono into the bridge.
    format!("{secs}")
}

pub(crate) fn parse_invoker_result(payload: &serde_json::Value) -> Result<IntruderResultRow, ()> {
    use halo_backend::commands::intruder::InvokerAttackResult;
    const MAX_UI_INTRUDER_BODY_BYTES: usize = 16 * 1024;
    let result: InvokerAttackResult = serde_json::from_value(payload.clone()).map_err(|_| ())?;
    let response_body = result.response.as_ref().map(|r| {
        if r.body.len() <= MAX_UI_INTRUDER_BODY_BYTES {
            r.body.clone()
        } else {
            let mut end = MAX_UI_INTRUDER_BODY_BYTES;
            while end > 0 && !r.body.is_char_boundary(end) {
                end -= 1;
            }
            r.body[..end].to_string()
        }
    });
    Ok(IntruderResultRow {
        id: result.id,
        payload_values: result.payload_values,
        status: result.status,
        response_length: result.response_length,
        response_time_ms: result.response_time_ms,
        error: result.error,
        grep_match: result.grep_match,
        grep_extracted: result.grep_extracted,
        response_body,
        response_headers: result
            .response
            .as_ref()
            .map(|r| r.headers.clone())
            .unwrap_or_default(),
        final_url: result.response.as_ref().map(|r| r.final_url.clone()),
        status_text: result.response.as_ref().map(|r| r.status_text.clone()),
    })
}

pub(crate) fn normalize_intercept_host(host: &str) -> String {
    let mut cleaned = host.trim().to_lowercase();
    if cleaned.is_empty() {
        return cleaned;
    }
    if let Some(rest) = cleaned.strip_prefix("https://") {
        cleaned = rest.to_string();
    } else if let Some(rest) = cleaned.strip_prefix("http://") {
        cleaned = rest.to_string();
    }
    cleaned = cleaned
        .split('/')
        .next()
        .unwrap_or(&cleaned)
        .trim()
        .to_string();
    if let Some(stripped) = cleaned.strip_suffix(":80") {
        cleaned = stripped.to_string();
    } else if let Some(stripped) = cleaned.strip_suffix(":443") {
        cleaned = stripped.to_string();
    }
    cleaned
}

pub(crate) fn capture_pattern_matches_host(pattern: &str, host: &str) -> bool {
    let normalized_pattern = normalize_intercept_host(pattern);
    let normalized_host = normalize_intercept_host(host);
    let host_without_port = normalized_host
        .split(':')
        .next()
        .unwrap_or(&normalized_host);

    if let Some(domain) = normalized_pattern.strip_prefix("*.") {
        return normalized_host.ends_with(domain) || host_without_port.ends_with(domain);
    }

    normalized_pattern == normalized_host || normalized_pattern == host_without_port
}

use std::collections::HashMap;
use std::time::Instant;

use base64::{Engine as _, engine::general_purpose};
use reqwest::Method;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RepeaterRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// Alias used by the Forge / API Collection path (same wire shape).
pub type ForgeRequest = RepeaterRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeaterResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub time_ms: u128,
    pub final_url: String,
}

pub type ForgeResponse = RepeaterResponse;

/// Decode `data:*/*;base64,...` bodies into raw bytes; otherwise return UTF-8 bytes.
fn decode_body(body: &str) -> Vec<u8> {
    if body.starts_with("data:") && body.contains(";base64,") {
        if let Some(pos) = body.find(";base64,") {
            let base64_data = &body[pos + 8..];
            if let Ok(decoded) = general_purpose::STANDARD.decode(base64_data) {
                return decoded;
            }
        }
    }
    body.as_bytes().to_vec()
}

pub async fn send_repeater_request(request: RepeaterRequest) -> Result<RepeaterResponse, String> {
    send_forge_request(request).await
}

/// Full-featured Forge request engine: redirects capped at 10, binary data-URL bodies,
/// content-encoding re-encode when a Content-Encoding header is present.
pub async fn send_forge_request(request: ForgeRequest) -> Result<ForgeResponse, String> {
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|error| format!("Invalid HTTP method: {}", error))?;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        // Without these the caller blocks forever on a server that accepts the
        // connection and never answers — and the caller runs on the UI thread.
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| format!("Failed to build HTTP client: {}", error))?;

    let mut builder = client.request(method, &request.url);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }

    if !request.body.is_empty() {
        let mut body_bytes = decode_body(&request.body);

        let content_encoding = request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
            .map(|(_, v)| v.clone());

        if let Some(encoding) = content_encoding {
            if !encoding.is_empty() {
                match halo_traffic::utils::encode_body(&encoding, &body_bytes) {
                    Ok(encoded) => body_bytes = encoded,
                    Err(e) => eprintln!("[forge] Failed to re-encode body ({encoding}): {e}"),
                }
            }
        }

        builder = builder.body(body_bytes);
    }

    let started_at = Instant::now();
    let response = builder
        .send()
        .await
        .map_err(|error| format!("Failed to send request: {}", error))?;
    let status = response.status();
    let final_url = response.url().to_string();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let body_bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read response body: {}", error))?;
    const MAX_REPEATER_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
    let slice = &body_bytes[..body_bytes.len().min(MAX_REPEATER_RESPONSE_BYTES)];
    let body = String::from_utf8_lossy(slice).into_owned();

    Ok(ForgeResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        headers,
        body,
        time_ms: started_at.elapsed().as_millis(),
        final_url,
    })
}

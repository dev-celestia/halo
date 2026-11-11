//! Format HTTP wire text from detail DTOs (for CodeEditor panes).

use crate::types::HttpLogDetail;
use std::collections::HashMap;

/// Wire text ends on its own line: the `http` highlight grammar drops the
/// last line into an error-recovery section when the text does not end with
/// a newline, which cuts the final `}` out of JSON bodies.
fn finish(out: &mut String) {
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

pub fn format_raw_request(detail: &HttpLogDetail) -> String {
    let mut out = String::new();
    let path = path_from_url(&detail.url);
    out.push_str(&format!(
        "{} {} {}\n",
        detail.method, path, detail.req_http_version
    ));
    let mut headers: Vec<_> = detail.req_headers.iter().collect();
    headers.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push('\n');
    out.push_str(&detail.req_body);
    finish(&mut out);
    out
}

/// Display wire text for the Intercept editor. LF newlines: the `http`
/// highlight grammar parses headers under CRLF but orphans the body after
/// `\r\n\r\n`, and `parse_raw_request` normalizes CRLF when reading back.
pub fn format_raw_request_parts(
    method: &str,
    url: &str,
    http_version: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> String {
    let mut out = String::new();
    let path = path_from_url(url);
    let version = if http_version.is_empty() {
        "HTTP/1.1"
    } else {
        http_version
    };
    out.push_str(&format!("{method} {path} {version}\n"));
    let mut hdrs: Vec<_> = headers.iter().collect();
    hdrs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (k, v) in hdrs {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push('\n');
    out.push_str(&String::from_utf8_lossy(body));
    finish(&mut out);
    out
}

pub fn format_raw_response(detail: &HttpLogDetail) -> String {
    let mut out = String::new();
    let status = detail.res_status.unwrap_or(0);
    let status_text = detail.res_status_text.as_deref().unwrap_or("");
    let version = detail.res_http_version.as_deref().unwrap_or("HTTP/1.1");
    out.push_str(&format!("{version} {status} {status_text}\n"));
    let mut headers: Vec<_> = detail.res_headers.iter().collect();
    headers.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push('\n');
    out.push_str(&detail.res_body);
    finish(&mut out);
    out
}

/// Display wire text for the Intercept editor; LF for the same reason as
/// [`format_raw_request_parts`].
pub fn format_raw_response_parts(
    status: u16,
    status_text: &str,
    http_version: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
) -> String {
    let mut out = String::new();
    let version = if http_version.is_empty() {
        "HTTP/1.1"
    } else {
        http_version
    };
    out.push_str(&format!("{version} {status} {status_text}\n"));
    let mut hdrs: Vec<_> = headers.iter().collect();
    hdrs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (k, v) in hdrs {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push('\n');
    out.push_str(&String::from_utf8_lossy(body));
    finish(&mut out);
    out
}

pub fn parse_raw_request(
    raw: &str,
    default_url: &str,
) -> (String, String, HashMap<String, String>, String) {
    let normalized = raw.replace("\r\n", "\n");
    let (header_block, body) = match normalized.split_once("\n\n") {
        Some((h, b)) => (h, b.to_string()),
        None => (normalized.as_str(), String::new()),
    };

    let mut lines = header_block.lines();
    let first_line = lines.next().unwrap_or("").trim();
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    let method = parts.first().copied().unwrap_or("GET").to_uppercase();
    let path = parts.get(1).copied().unwrap_or("/");

    let mut headers = HashMap::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let url = if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        let scheme = if default_url.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        let host = headers
            .get("host")
            .or_else(|| headers.get("Host"))
            .cloned()
            .unwrap_or_else(|| {
                default_url
                    .split("://")
                    .nth(1)
                    .and_then(|r| r.split('/').next())
                    .unwrap_or("")
                    .to_string()
            });

        if !host.is_empty() {
            let clean_path = if path.starts_with('/') {
                path.to_string()
            } else {
                format!("/{path}")
            };
            format!("{scheme}://{host}{clean_path}")
        } else {
            default_url.to_string()
        }
    };

    (method, url, headers, body)
}

pub fn parse_raw_response(raw: &str) -> (u16, Option<String>, HashMap<String, String>, String) {
    let normalized = raw.replace("\r\n", "\n");
    let (header_block, body) = match normalized.split_once("\n\n") {
        Some((h, b)) => (h, b.to_string()),
        None => (normalized.as_str(), String::new()),
    };

    let mut lines = header_block.lines();
    let first_line = lines.next().unwrap_or("").trim();
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    let status = parts
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(200);
    let status_text = if parts.len() > 2 {
        Some(parts[2..].join(" "))
    } else {
        None
    };

    let mut headers = HashMap::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    (status, status_text, headers, body)
}

pub fn format_json_body(raw_http: &str) -> String {
    let crlf = raw_http.contains("\r\n\r\n");
    let delimiter = if crlf { "\r\n\r\n" } else { "\n\n" };
    if let Some((headers, body)) = raw_http.split_once(delimiter) {
        let trimmed_body = body.trim();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed_body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                let formatted_pretty = if crlf {
                    pretty.replace('\n', "\r\n")
                } else {
                    pretty
                };
                let mut out = format!("{headers}{delimiter}{formatted_pretty}");
                finish(&mut out);
                return out;
            }
        }
    }
    raw_http.to_string()
}

pub fn path_from_url(url: &str) -> String {
    if let Some(rest) = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
    {
        if let Some(slash) = rest.find('/') {
            return rest[slash..].to_string();
        }
        return "/".into();
    }
    if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_format_request() {
        let raw = "POST /api/login HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/json\r\n\r\n{\"user\":\"admin\"}";
        let (method, url, headers, body) = parse_raw_request(raw, "https://example.com/api/login");
        assert_eq!(method, "POST");
        assert_eq!(url, "https://example.com/api/login");
        assert_eq!(
            headers.get("Host").or(headers.get("host")).unwrap(),
            "example.com"
        );
        assert_eq!(body, "{\"user\":\"admin\"}");

        let formatted = format_json_body(raw);
        assert!(formatted.contains("{\r\n  \"user\": \"admin\"\r\n}"));
    }

    #[test]
    fn test_parse_and_format_response() {
        let raw =
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\":\"success\"}";
        let (status, status_text, headers, body) = parse_raw_response(raw);
        assert_eq!(status, 200);
        assert_eq!(status_text.as_deref(), Some("OK"));
        assert_eq!(
            headers
                .get("content-type")
                .or(headers.get("Content-Type"))
                .unwrap(),
            "application/json"
        );
        assert_eq!(body, "{\"status\":\"success\"}");
    }

    #[test]
    fn test_parts_formatters_emit_lf_for_display() {
        let mut headers = HashMap::new();
        headers.insert("Host".into(), "example.com".into());
        let req =
            format_raw_request_parts("GET", "https://example.com/x", "HTTP/1.1", &headers, b"{}");
        assert_eq!(req, "GET /x HTTP/1.1\nHost: example.com\n\n{}\n");

        let res = format_raw_response_parts(200, "OK", "", &headers, b"hi");
        assert_eq!(res, "HTTP/1.1 200 OK\nHost: example.com\n\nhi\n");
    }
}

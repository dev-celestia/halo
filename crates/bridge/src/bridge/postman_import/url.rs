//! Postman URL object rebuilding and query handling.

use super::*;

pub(crate) fn rebuild_url(url: &Value) -> String {
    if let Some(raw) = url.as_str() {
        return raw.to_string();
    }
    if let Some(raw) = url.get("raw").and_then(Value::as_str) {
        if !raw.is_empty() {
            return raw.to_string();
        }
    }
    let protocol = url.get("protocol").and_then(Value::as_str).unwrap_or("");
    let host = join_host(url.get("host"));
    let path = join_path(url.get("path"));
    let query = join_query(url.get("query"));
    let mut out = String::new();
    if !protocol.is_empty() && !host.is_empty() {
        out.push_str(protocol);
        out.push_str("://");
        out.push_str(&host);
    } else if !host.is_empty() {
        out.push_str(&host);
    }
    out.push_str(&path);
    out.push_str(&query);
    out
}

pub(crate) fn join_host(host: Option<&Value>) -> String {
    match host {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(Value::as_str)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("."),
        _ => String::new(),
    }
}

pub(crate) fn join_path(path: Option<&Value>) -> String {
    match path {
        Some(Value::String(text)) if text.is_empty() => String::new(),
        Some(Value::String(text)) if text.starts_with('/') => text.clone(),
        Some(Value::String(text)) => format!("/{text}"),
        Some(Value::Array(parts)) => {
            let joined = parts
                .iter()
                .filter_map(Value::as_str)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("/");
            if joined.is_empty() {
                String::new()
            } else {
                format!("/{joined}")
            }
        }
        _ => String::new(),
    }
}

pub(crate) fn join_query(query: Option<&Value>) -> String {
    let Some(items) = query.and_then(Value::as_array) else {
        return String::new();
    };
    let mut pairs = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        if !is_enabled(obj) {
            continue;
        }
        let key = obj.get("key").map(scalar_string).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let value = obj.get("value").map(scalar_string).unwrap_or_default();
        pairs.push(format!("{}={}", form_encode(&key), form_encode(&value)));
    }
    if pairs.is_empty() {
        String::new()
    } else {
        format!("?{}", pairs.join("&"))
    }
}

pub(crate) fn append_query(url: &str, key: &str, value: &str) -> String {
    let (base, fragment) = match url.split_once('#') {
        Some((base, fragment)) => (base, Some(fragment)),
        None => (url, None),
    };
    let sep = if base.contains('?') { '&' } else { '?' };
    let mut out = format!("{base}{sep}{}={}", form_encode(key), form_encode(value));
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

pub(crate) fn query_has_key(url: &str, key: &str) -> bool {
    let Some((_, rest)) = url.split_once('?') else {
        return false;
    };
    let query = rest.split('#').next().unwrap_or("");
    let encoded = form_encode(key);
    query.split('&').any(|pair| {
        let name = pair.split_once('=').map(|(name, _)| name).unwrap_or(pair);
        name == key || name == encoded
    })
}

/// Percent-encode a form field, leaving `{{var}}` tokens intact.
pub(crate) fn form_encode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(rel) = input[i + 2..].find("}}") {
                let end = i + 2 + rel + 2;
                out.push_str(&input[i..end]);
                i = end;
                continue;
            }
        }
        match bytes[i] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(bytes[i] as char);
            }
            b' ' => out.push('+'),
            byte => out.push_str(&format!("%{byte:02X}")),
        }
        i += 1;
    }
    out
}

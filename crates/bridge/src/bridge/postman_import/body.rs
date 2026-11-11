//! Postman request-body conversion (urlencoded, graphql, formdata, raw).

use super::*;

pub(crate) fn parse_headers(header: Option<&Value>) -> Map<String, Value> {
    let mut headers = Map::new();
    let Some(header) = header else {
        return headers;
    };
    match header {
        Value::Array(items) => {
            for item in items {
                let Some(obj) = item.as_object() else {
                    continue;
                };
                if !is_enabled(obj) {
                    continue;
                }
                let key = obj.get("key").map(scalar_string).unwrap_or_default();
                let key = key.trim();
                if key.is_empty() {
                    continue;
                }
                let value = obj.get("value").map(scalar_string).unwrap_or_default();
                insert_header(&mut headers, key, &value);
            }
        }
        Value::String(raw) => {
            for line in raw.lines() {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let key = key.trim();
                if key.is_empty() {
                    continue;
                }
                insert_header(&mut headers, key, value.trim());
            }
        }
        Value::Object(obj) => {
            for (key, value) in obj {
                if key.trim().is_empty() {
                    continue;
                }
                match value {
                    Value::Object(inner) => {
                        if !is_enabled(inner) {
                            continue;
                        }
                        let text = inner.get("value").map(scalar_string).unwrap_or_default();
                        insert_header(&mut headers, key.trim(), &text);
                    }
                    other => insert_header(&mut headers, key.trim(), &scalar_string(other)),
                }
            }
        }
        _ => {}
    }
    headers
}

pub(crate) fn parse_body(
    body: Option<&Value>,
    headers: &mut Map<String, Value>,
) -> (String, String) {
    let Some(body) = body else {
        return (String::new(), "none".into());
    };
    if body.is_null() {
        return (String::new(), "none".into());
    }
    let mode = body
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("raw")
        .to_ascii_lowercase();
    match mode.as_str() {
        "urlencoded" => urlencoded_body(body, headers),
        "graphql" => graphql_body(body, headers),
        "formdata" => formdata_body(body, headers),
        "file" | "none" => (String::new(), "none".into()),
        _ => raw_body(body, headers),
    }
}

pub(crate) fn raw_body(body: &Value, headers: &mut Map<String, Value>) -> (String, String) {
    let text = body.get("raw").map(scalar_string).unwrap_or_default();
    if text.is_empty() {
        return (String::new(), "none".into());
    }
    let language = body
        .get("options")
        .and_then(|opts| opts.get("raw"))
        .and_then(|raw| raw.get("language"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if language == "json" {
        ensure_content_type(headers, "application/json");
        (text, "json".into())
    } else {
        if let Some(content_type) = raw_content_type(&language) {
            ensure_content_type(headers, content_type);
        }
        (text, "raw".into())
    }
}

pub(crate) fn raw_content_type(language: &str) -> Option<&'static str> {
    match language {
        "xml" => Some("application/xml"),
        "html" => Some("text/html"),
        "javascript" | "js" => Some("application/javascript"),
        "text" | "plain" => Some("text/plain"),
        _ => None,
    }
}

pub(crate) fn urlencoded_body(body: &Value, headers: &mut Map<String, Value>) -> (String, String) {
    let Some(fields) = body.get("urlencoded").and_then(Value::as_array) else {
        return (String::new(), "none".into());
    };
    let mut pairs = Vec::new();
    for field in fields {
        let Some(obj) = field.as_object() else {
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
        return (String::new(), "none".into());
    }
    ensure_content_type(headers, "application/x-www-form-urlencoded");
    (pairs.join("&"), "raw".into())
}

pub(crate) fn graphql_body(body: &Value, headers: &mut Map<String, Value>) -> (String, String) {
    let Some(graphql) = body.get("graphql") else {
        return (String::new(), "none".into());
    };
    let query = graphql.get("query").map(scalar_string).unwrap_or_default();
    let variables = match graphql.get("variables") {
        Some(Value::String(text)) if text.trim().is_empty() => Value::Object(Map::new()),
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(Value::Null) | None => Value::Object(Map::new()),
        Some(other) => other.clone(),
    };
    let variables_empty = match &variables {
        Value::Object(map) => map.is_empty(),
        Value::String(text) => text.is_empty(),
        Value::Null => true,
        _ => false,
    };
    if query.is_empty() && variables_empty {
        return (String::new(), "none".into());
    }
    ensure_content_type(headers, "application/json");
    let payload = serde_json::json!({ "query": query, "variables": variables });
    (payload.to_string(), "json".into())
}

pub(crate) fn formdata_body(body: &Value, headers: &mut Map<String, Value>) -> (String, String) {
    let Some(fields) = body.get("formdata").and_then(Value::as_array) else {
        return (String::new(), "none".into());
    };
    let mut parts = Vec::new();
    for field in fields {
        let Some(obj) = field.as_object() else {
            continue;
        };
        if !is_enabled(obj) {
            continue;
        }
        let key = obj.get("key").map(scalar_string).unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let kind = obj.get("type").and_then(Value::as_str).unwrap_or("text");
        let is_file = kind.eq_ignore_ascii_case("file");
        let disposition = if is_file {
            let src = obj.get("src").and_then(Value::as_str).unwrap_or("");
            let filename = std::path::Path::new(src)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| !n.is_empty())
                .unwrap_or(key.as_str());
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"",
                escape_quoted(&key),
                escape_quoted(filename)
            )
        } else {
            format!(
                "Content-Disposition: form-data; name=\"{}\"",
                escape_quoted(&key)
            )
        };
        let content = if is_file {
            let src = obj.get("src").and_then(Value::as_str).unwrap_or("");
            let note = if src.is_empty() { key.as_str() } else { src };
            format!("[file: {note}]")
        } else {
            obj.get("value").map(scalar_string).unwrap_or_default()
        };
        parts.push(format!(
            "--{MULTIPART_BOUNDARY}\r\n{disposition}\r\n\r\n{content}\r\n"
        ));
    }
    if parts.is_empty() {
        return (String::new(), "none".into());
    }
    ensure_content_type(
        headers,
        &format!("multipart/form-data; boundary={MULTIPART_BOUNDARY}"),
    );
    parts.push(format!("--{MULTIPART_BOUNDARY}--\r\n"));
    (parts.join(""), "raw".into())
}

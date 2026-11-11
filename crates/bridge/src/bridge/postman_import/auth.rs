//! Postman auth resolution into headers and URL query.

use super::*;

pub(crate) fn apply_auth(headers: &mut Map<String, Value>, url: &mut String, auth: Option<&Value>) {
    let Some(auth) = auth else {
        return;
    };
    let kind = auth
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    match kind.as_str() {
        "bearer" => {
            if has_header(headers, "Authorization") {
                return;
            }
            let token = auth_field(auth, "bearer", "token").unwrap_or_default();
            insert_header(headers, "Authorization", &format!("Bearer {token}"));
        }
        "basic" => {
            if has_header(headers, "Authorization") {
                return;
            }
            let user = auth_field(auth, "basic", "username").unwrap_or_default();
            let pass = auth_field(auth, "basic", "password").unwrap_or_default();
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            insert_header(headers, "Authorization", &format!("Basic {encoded}"));
        }
        "apikey" => {
            let key = auth_field(auth, "apikey", "key").unwrap_or_default();
            if key.is_empty() {
                return;
            }
            let value = auth_field(auth, "apikey", "value").unwrap_or_default();
            let location = auth_field(auth, "apikey", "in").unwrap_or_else(|| "header".into());
            if location.eq_ignore_ascii_case("query") {
                if !query_has_key(url, &key) {
                    *url = append_query(url, &key, &value);
                }
            } else if !has_header(headers, &key) {
                insert_header(headers, &key, &value);
            }
        }
        _ => {}
    }
}

pub(crate) fn auth_field(auth: &Value, bucket: &str, field: &str) -> Option<String> {
    let bucket = auth.get(bucket)?;
    if let Some(entries) = bucket.as_array() {
        for entry in entries {
            let key = entry.get("key").and_then(Value::as_str).unwrap_or("");
            if key.eq_ignore_ascii_case(field) {
                return Some(entry.get("value").map(scalar_string).unwrap_or_default());
            }
        }
        return None;
    }
    bucket.get(field).map(scalar_string)
}

/// Request auth wins. `inherit` (or a missing type) keeps the parent auth.
/// `noauth` blocks the parent auth.
pub(crate) fn resolve_auth<'a>(
    own: Option<&'a Value>,
    inherited: Option<&'a Value>,
) -> Option<&'a Value> {
    match own {
        Some(auth) => {
            let kind = auth.get("type").and_then(Value::as_str).unwrap_or("");
            if kind.is_empty() || kind.eq_ignore_ascii_case("inherit") {
                inherited
            } else {
                Some(auth)
            }
        }
        None => inherited,
    }
}

//! Postman variables and header-map helpers.

use super::*;

pub(crate) struct ParsedVariable {
    key: String,
    pub(crate) value: String,
    pub(crate) enabled: bool,
}

pub(crate) fn variable_list(value: Option<&Value>) -> Vec<ParsedVariable> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut variables = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let key = obj.get("key").map(scalar_string).unwrap_or_default();
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        variables.push(ParsedVariable {
            key: key.to_string(),
            value: obj.get("value").map(scalar_string).unwrap_or_default(),
            enabled: is_enabled(obj),
        });
    }
    variables
}

pub(crate) fn variables_to_json(variables: &[ParsedVariable]) -> String {
    let mut map = Map::new();
    for variable in variables {
        map.insert(
            variable.key.clone(),
            serde_json::json!({
                "value": variable.value,
                "enabled": variable.enabled,
            }),
        );
    }
    Value::Object(map).to_string()
}

pub(crate) fn is_enabled(obj: &Map<String, Value>) -> bool {
    if let Some(disabled) = obj.get("disabled").and_then(Value::as_bool) {
        return !disabled;
    }
    obj.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

pub(crate) fn insert_header(headers: &mut Map<String, Value>, key: &str, value: &str) {
    if let Some(existing_key) = find_header_key(headers, key) {
        let current = headers
            .get(&existing_key)
            .and_then(Value::as_str)
            .unwrap_or("");
        let joined = if current.is_empty() {
            value.to_string()
        } else {
            format!("{current}, {value}")
        };
        headers.insert(existing_key, Value::String(joined));
    } else {
        headers.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(crate) fn ensure_content_type(headers: &mut Map<String, Value>, value: &str) {
    if !has_header(headers, "content-type") {
        insert_header(headers, "Content-Type", value);
    }
}

pub(crate) fn has_header(headers: &Map<String, Value>, key: &str) -> bool {
    find_header_key(headers, key).is_some()
}

pub(crate) fn find_header_key(headers: &Map<String, Value>, key: &str) -> Option<String> {
    headers
        .keys()
        .find(|existing| existing.eq_ignore_ascii_case(key))
        .cloned()
}

pub(crate) fn scalar_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn non_empty_name(value: Option<&Value>, fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(crate) fn escape_quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

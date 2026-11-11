//! Forge craft pipeline helpers: environment variable expansion.

use std::collections::HashMap;

use serde_json::Value;

/// Parse context `variables` JSON into a flat enabled key→value map.
///
/// Accepts either:
/// - flat map: `{"host":"api.example.com"}`
/// - Postman-style: `{"host":{"value":"api.example.com","enabled":true}}`
pub fn parse_context_variables(variables_json: &str) -> HashMap<String, String> {
    let Ok(value) = serde_json::from_str::<Value>(variables_json) else {
        return HashMap::new();
    };
    let Some(obj) = value.as_object() else {
        return HashMap::new();
    };

    let mut out = HashMap::new();
    for (key, val) in obj {
        match val {
            Value::String(s) => {
                out.insert(key.clone(), s.clone());
            }
            Value::Object(inner) => {
                let enabled = inner
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if !enabled {
                    continue;
                }
                if let Some(s) = inner.get("value").and_then(|v| v.as_str()) {
                    out.insert(key.clone(), s.to_string());
                } else if let Some(s) = inner.get("value").and_then(|v| v.as_i64()) {
                    out.insert(key.clone(), s.to_string());
                } else if let Some(s) = inner.get("value").and_then(|v| v.as_f64()) {
                    out.insert(key.clone(), s.to_string());
                } else if let Some(b) = inner.get("value").and_then(|v| v.as_bool()) {
                    out.insert(key.clone(), b.to_string());
                }
            }
            Value::Number(n) => {
                out.insert(key.clone(), n.to_string());
            }
            Value::Bool(b) => {
                out.insert(key.clone(), b.to_string());
            }
            _ => {}
        }
    }
    out
}

/// Replace `{var_name}` and `{{var_name}}` using the given map. Unknown vars are left intact.
///
/// Single-brace names must be identifiers, so a JSON object in the same string
/// is not treated as a placeholder. `{{name}}` is matched first.
pub fn expand_vars(input: &str, vars: &HashMap<String, String>) -> String {
    if !input.contains('{') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find("}}") {
                let key = input[i + 2..i + 2 + end].trim();
                if let Some(val) = vars.get(key) {
                    out.push_str(val);
                    i = i + 2 + end + 2;
                    continue;
                }
            }
        } else if bytes[i] == b'{' {
            if let Some(end) = input[i + 1..].find('}') {
                let close = i + 1 + end;
                let followed_by_brace = input[close + 1..].starts_with('}');
                let key = input[i + 1..close].trim();
                if !followed_by_brace && is_var_ident(key) {
                    if let Some(val) = vars.get(key) {
                        out.push_str(val);
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        let ch = input[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn is_var_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

/// Expand vars across URL, headers, and body.
pub fn expand_request(
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    body: &str,
    vars: &HashMap<String, String>,
) -> (String, String, HashMap<String, String>, String) {
    let method = expand_vars(method, vars);
    let url = expand_vars(url, vars);
    let headers = headers
        .iter()
        .map(|(k, v)| (expand_vars(k, vars), expand_vars(v, vars)))
        .collect();
    let body = expand_vars(body, vars);
    (method, url, headers, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_simple_vars() {
        let mut vars = HashMap::new();
        vars.insert("host".into(), "api.example.com".into());
        vars.insert("token".into(), "abc".into());
        assert_eq!(
            expand_vars("https://{{host}}/v1?t={{token}}", &vars),
            "https://api.example.com/v1?t=abc"
        );
    }

    #[test]
    fn leaves_unknown_intact() {
        let vars = HashMap::new();
        assert_eq!(expand_vars("{{missing}}", &vars), "{{missing}}");
        assert_eq!(
            expand_vars("https://{missing}/me", &vars),
            "https://{missing}/me"
        );
    }

    #[test]
    fn expands_single_brace_idents() {
        let mut vars = HashMap::new();
        vars.insert("host".into(), "httpbin.org".into());
        assert_eq!(
            expand_vars("https://{host}/me", &vars),
            "https://httpbin.org/me"
        );
        assert_eq!(
            expand_vars(r#"{"host":"{host}"}"#, &vars),
            r#"{"host":"httpbin.org"}"#
        );
    }

    #[test]
    fn parses_nested_and_flat() {
        let flat = parse_context_variables(r#"{"a":"1"}"#);
        assert_eq!(flat.get("a").map(String::as_str), Some("1"));
        let nested = parse_context_variables(
            r#"{"b":{"value":"2","enabled":true},"c":{"value":"3","enabled":false}}"#,
        );
        assert_eq!(nested.get("b").map(String::as_str), Some("2"));
        assert!(!nested.contains_key("c"));
    }
}

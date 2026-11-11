//! Postman event-script extraction (prerequest / test).

use super::*;

pub(crate) fn script_text(events: Option<&Value>, listen: &str) -> String {
    let Some(events) = events.and_then(Value::as_array) else {
        return String::new();
    };
    let mut lines = Vec::new();
    for event in events {
        let kind = event.get("listen").and_then(Value::as_str).unwrap_or("");
        if !listen_matches(kind, listen) {
            continue;
        }
        match event.get("script").and_then(|script| script.get("exec")) {
            Some(Value::Array(parts)) => {
                for part in parts {
                    if let Some(line) = part.as_str() {
                        lines.push(line.to_string());
                    }
                }
            }
            Some(Value::String(text)) => lines.push(text.clone()),
            _ => {}
        }
    }
    lines.join("\n")
}

pub(crate) fn listen_matches(kind: &str, listen: &str) -> bool {
    if kind.eq_ignore_ascii_case(listen) {
        return true;
    }
    listen == "prerequest" && kind.eq_ignore_ascii_case("pre-request")
}

pub(crate) fn join_scripts(inherited: &str, local: &str) -> String {
    match (inherited.is_empty(), local.is_empty()) {
        (true, _) => local.to_string(),
        (false, true) => inherited.to_string(),
        (false, false) => format!("{inherited}\n{local}"),
    }
}

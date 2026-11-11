//! Turning an accessibility tree into text the model can afford to read.
//!
//! The engine already bounds the tree it walks; this module bounds the *text*.
//! They are different budgets and conflating them is how a wrapper ends up
//! either lying about completeness or flooding the context.
//!
//! The line format is deliberately close to the browser tool's:
//!
//! ```text
//! window "Untitled"
//!   textarea "Body" @s8f3k2p9:e3 = "hello"
//!   button "Save" @s8f3k2p9:e7 [enabled]
//! ```

use serde_json::Value;

use crate::types::Truncation;
use crate::HARD_MAX_CHARS;

/// Longest label kept on one line. A document title can be a paragraph.
const MAX_LABEL: usize = 80;

/// Deepest level rendered. The engine caps its own walk at 10 by default;
/// this is only a guard against a malformed tree.
const MAX_DEPTH: usize = 40;

/// Render a tree, then apply the rendering budget.
pub fn tree(value: &Value, max_chars: usize) -> (String, Option<Truncation>) {
    let mut lines = Vec::new();
    walk(value, 0, &mut lines);
    let text = lines.join("\n");
    truncate(&text, max_chars.min(HARD_MAX_CHARS))
}

/// One compact line per element, indented by depth.
fn walk(node: &Value, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    out.push(line_for(node, depth));

    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            walk(child, depth + 1, out);
        }
        return;
    }

    // A cut branch says how much is behind it, so the model knows to drill
    // rather than assume the app is empty.
    if let Some(count) = node.get("children_count").and_then(Value::as_u64) {
        if count > 0 {
            out.push(format!("{}+{count} more", "  ".repeat(depth + 1)));
        }
    }
}

fn line_for(node: &Value, depth: usize) -> String {
    let mut line = "  ".repeat(depth);
    line.push_str(
        node.get("role")
            .and_then(Value::as_str)
            .unwrap_or("node"),
    );

    if let Some(name) = node.get("name").and_then(Value::as_str) {
        if !name.trim().is_empty() {
            line.push_str(&format!(" \"{}\"", clip(name)));
        }
    }

    if let Some(reference) = node.get("ref_id").and_then(Value::as_str) {
        line.push(' ');
        line.push_str(reference);
    }

    if let Some(value) = node.get("value").and_then(Value::as_str) {
        if !value.is_empty() {
            line.push_str(&format!(" = \"{}\"", clip(value)));
        }
    }

    if let Some(states) = node.get("states").and_then(Value::as_array) {
        let states: Vec<&str> = states.iter().filter_map(Value::as_str).collect();
        if !states.is_empty() {
            line.push_str(&format!(" [{}]", states.join(", ")));
        }
    }

    line
}

fn clip(text: &str) -> String {
    let single_line = text.replace(['\n', '\r'], " ");
    let trimmed = single_line.trim();
    if trimmed.chars().count() <= MAX_LABEL {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(MAX_LABEL).collect();
    format!("{head}…")
}

/// Render a list-shaped result — apps, windows, `find` matches — one per line.
///
/// The engine's shapes differ per command (`list-apps` wraps its array, `find`
/// returns matches, `get` returns one object), so this reads defensively instead
/// of pinning a schema that would break on the next engine version.
pub fn list(value: &Value, max_chars: usize) -> (String, Option<Truncation>) {
    let items = items_of(value);
    if items.is_empty() {
        return ("(no matches)".to_string(), None);
    }
    let lines: Vec<String> = items.into_iter().map(summarize).collect();
    truncate(&lines.join("\n"), max_chars.min(HARD_MAX_CHARS))
}

fn items_of(value: &Value) -> Vec<&Value> {
    if let Some(array) = value.as_array() {
        return array.iter().collect();
    }
    if let Some(object) = value.as_object() {
        for key in ["apps", "windows", "matches", "results", "items", "elements"] {
            if let Some(array) = object.get(key).and_then(Value::as_array) {
                return array.iter().collect();
            }
        }
        // Single-result wrappers: `find --first` answers `{"match": {…}}`, and
        // `get` answers `{"property", "ref", "value"}`.
        for key in ["match", "element", "node", "result"] {
            if let Some(inner) = object.get(key) {
                return vec![inner];
            }
        }
    }
    vec![value]
}

fn summarize(item: &Value) -> String {
    if let Some(text) = item.as_str() {
        return format!("- {text}");
    }

    // `get`: one property of one element.
    if let Some(property) = item.get("property").and_then(Value::as_str) {
        let rendered = match item.get("value") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Null) | None => "(none)".to_string(),
            Some(other) => other.to_string(),
        };
        return format!("- {property} = {rendered}");
    }

    let mut parts: Vec<String> = Vec::new();
    for key in ["name", "title", "app", "label"] {
        if let Some(text) = item.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                parts.push(clip(text));
                break;
            }
        }
    }
    for key in ["role", "ref_id"] {
        if let Some(text) = item.get(key).and_then(Value::as_str) {
            parts.push(text.to_string());
        }
    }
    for key in ["pid", "window_id", "id"] {
        match item.get(key) {
            Some(Value::String(text)) => {
                parts.push(text.clone());
                break;
            }
            Some(Value::Number(number)) => {
                parts.push(number.to_string());
                break;
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        return format!("- {item}");
    }
    format!("- {}", parts.join(" · "))
}

/// Cut on a line boundary and say how to narrow.
pub fn truncate(text: &str, max_chars: usize) -> (String, Option<Truncation>) {
    let total = text.chars().count();
    if total <= max_chars {
        return (text.to_string(), None);
    }

    let cut = text
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let head = &text[..cut];
    // Prefer the last whole line; a half-rendered element is worse than one
    // fewer element.
    let boundary = head.rfind('\n').unwrap_or(head.len());
    let kept = &head[..boundary];
    let kept_chars = kept.chars().count();
    let dropped_chars = total - kept_chars;

    let hint = format!(
        "truncated {dropped_chars} chars; narrow with --root or --app, or raise --max (hard cap {HARD_MAX_CHARS})"
    );
    let mut rendered = kept.to_string();
    rendered.push_str(&format!("\n… [{hint}]"));
    (
        rendered,
        Some(Truncation {
            kept_chars,
            dropped_chars,
            hint,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_refs_names_values_and_states() {
        let tree = json!({
            "role": "window", "name": "Untitled", "children": [
                {"role": "textarea", "name": "Body", "ref_id": "@s1:e3", "value": "hello"},
                {"role": "button", "name": "Save", "ref_id": "@s1:e7", "states": ["enabled"]}
            ]
        });
        let (text, truncation) = tree_at(&tree, 4_000);
        assert!(truncation.is_none());
        assert!(text.starts_with("window \"Untitled\""), "{text}");
        assert!(text.contains("  textarea \"Body\" @s1:e3 = \"hello\""), "{text}");
        assert!(text.contains("  button \"Save\" @s1:e7 [enabled]"), "{text}");
    }

    #[test]
    fn a_cut_branch_reports_its_child_count() {
        let tree = json!({"role": "group", "children_count": 12});
        let (text, _) = tree_at(&tree, 4_000);
        assert!(text.contains("+12 more"), "{text}");
    }

    #[test]
    fn long_labels_are_clipped_to_one_line() {
        let long = "x".repeat(200);
        let tree = json!({"role": "button", "name": long});
        let (text, _) = tree_at(&tree, 4_000);
        assert!(text.chars().count() < 120, "{text}");
        assert!(text.contains('…'), "{text}");
        assert!(!text.contains('\n'), "one element is one line: {text}");
    }

    #[test]
    fn lists_summarise_the_fields_that_exist() {
        let apps = json!({"apps": [
            {"name": "TextEdit", "pid": 611},
            {"title": "Untitled", "window_id": "w-110407", "role": "window"}
        ]});
        let (text, _) = list(&apps, 4_000);
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains("TextEdit · 611"), "{text}");
        assert!(text.contains("Untitled"), "{text}");
    }

    #[test]
    fn an_empty_list_says_so() {
        let (text, _) = list(&json!([]), 4_000);
        assert_eq!(text, "(no matches)");
    }

    #[test]
    fn find_first_unwraps_the_match_object() {
        let found = json!({"match": {
            "interactive": true, "role": "button", "ref_id": "@s153:e1",
            "states": ["offscreen"], "value": null
        }, "snapshot_id": "s153"});
        let (text, _) = list(&found, 4_000);
        assert_eq!(text, "- button · @s153:e1", "{text}");
    }

    #[test]
    fn get_renders_as_property_equals_value() {
        let read = json!({"property": "role", "ref": "@s1:e1", "value": "scrollarea"});
        let (text, _) = list(&read, 4_000);
        assert_eq!(text, "- role = scrollarea", "{text}");

        let empty = json!({"property": "title", "ref": "@s1:e1", "value": null});
        assert_eq!(list(&empty, 4_000).0, "- title = (none)");
    }

    #[test]
    fn truncation_cuts_on_a_line_boundary_and_names_the_fix() {
        let tree = json!({"role": "window", "children": [
            {"role": "button", "name": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            {"role": "button", "name": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
            {"role": "button", "name": "cccccccccccccccccccccccccccccc"}
        ]});
        let (text, truncation) = tree_at(&tree, 60);
        let truncation = truncation.expect("must truncate");
        assert!(truncation.dropped_chars > 0);
        assert!(truncation.hint.contains("--root"), "{}", truncation.hint);
        // The kept part ends on a whole element, never mid-word.
        let kept = text.split("\n… [").next().unwrap();
        assert!(!kept.ends_with('a'), "{kept}");
        assert!(kept.starts_with("window"), "{kept}");
    }

    #[test]
    fn a_budget_below_one_line_still_returns_something_readable() {
        let tree = json!({"role": "window", "name": "Notes"});
        let (text, truncation) = tree_at(&tree, 1);
        assert!(truncation.is_some());
        assert!(text.contains("truncated"), "{text}");
    }

    fn tree_at(value: &Value, budget: usize) -> (String, Option<Truncation>) {
        tree(value, budget)
    }
}

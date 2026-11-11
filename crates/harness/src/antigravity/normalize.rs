//! ACP `session/update` → [`AntigravityEvent`] (protocol v1 shapes).

use serde_json::Value;

use super::types::AntigravityEvent;
use crate::usage::{self, ContextMeter};

fn str_field(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_owned()
}

fn content_block_text(block: &Value) -> Option<&str> {
    (block.get("type").and_then(Value::as_str) == Some("text"))
        .then(|| block.get("text").and_then(Value::as_str))
        .flatten()
}

fn chunk_text(update: &Value) -> Option<String> {
    let text = content_block_text(update.get("content")?)?;
    (!text.is_empty()).then(|| text.to_owned())
}

fn tool_output(update: &Value) -> Option<String> {
    let parts: Vec<&str> = update
        .get("content")?
        .as_array()?
        .iter()
        .filter(|c| c.get("type").and_then(Value::as_str) == Some("content"))
        .filter_map(|c| content_block_text(c.get("content")?))
        .filter(|t| !t.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("\n"))
}

fn tool_name(update: &Value) -> String {
    let kind = update.get("kind").and_then(Value::as_str).unwrap_or("tool");
    let title = str_field(update, "title");
    if !title.is_empty() {
        title
    } else {
        kind.into()
    }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "rejected")
}

pub fn map_update(update: &Value) -> Vec<AntigravityEvent> {
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or("");
    match kind {
        "agent_message_chunk" => chunk_text(update)
            .map(|text| vec![AntigravityEvent::TextDelta { text }])
            .unwrap_or_default(),
        "agent_thought_chunk" => chunk_text(update)
            .map(|text| vec![AntigravityEvent::ReasoningDelta { text }])
            .unwrap_or_default(),
        "tool_call" => {
            let id = str_field(update, "toolCallId");
            let mut events = vec![AntigravityEvent::ToolCall {
                id: id.clone(),
                name: tool_name(update),
                args: update.get("rawInput").cloned().unwrap_or(Value::Null),
            }];
            let status = str_field(update, "status");
            if is_terminal_status(&status) {
                events.push(AntigravityEvent::ToolResult {
                    id,
                    output: tool_output(update),
                    is_error: status == "failed" || status == "rejected",
                });
            }
            events
        }
        "tool_call_update" => {
            let id = str_field(update, "toolCallId");
            let status = str_field(update, "status");
            let mut events = Vec::new();
            if update.get("kind").is_some()
                || update.get("title").is_some()
                || update.get("rawInput").is_some()
            {
                events.push(AntigravityEvent::ToolCall {
                    id: id.clone(),
                    name: tool_name(update),
                    args: update.get("rawInput").cloned().unwrap_or(Value::Null),
                });
            }
            if is_terminal_status(&status) {
                events.push(AntigravityEvent::ToolResult {
                    id,
                    output: tool_output(update),
                    is_error: status == "failed" || status == "rejected",
                });
            }
            events
        }
        "usage_update" => usage_from(update).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// ACP `usage_update`, or a prompt result that nests the same object.
pub fn usage_from(value: &Value) -> Option<AntigravityEvent> {
    if value.get("sessionUpdate").and_then(Value::as_str) == Some("usage_update") {
        return meter_event(value, "");
    }
    value
        .get("usage")
        .and_then(|usage| meter_event(usage, ""))
        .or_else(|| value.get("usageUpdate").and_then(|usage| meter_event(usage, "")))
}

fn meter_event(value: &Value, model: &str) -> Option<AntigravityEvent> {
    let used = usage::json_count(value.get("used"));
    let size = usage::json_count(value.get("size"));
    let nested = value.get("usage");
    let input = usage::json_count(
        value
            .get("inputTokens")
            .or_else(|| value.get("input"))
            .or_else(|| nested.and_then(|item| item.get("inputTokens"))),
    );
    let output = usage::json_count(
        value
            .get("outputTokens")
            .or_else(|| value.get("output"))
            .or_else(|| nested.and_then(|item| item.get("outputTokens"))),
    );
    let window = (size > 0).then_some(size);
    let ContextMeter {
        used_tokens,
        window_tokens,
        input_tokens,
        output_tokens,
    } = usage::occupied(model, used, input, output, window)?;
    Some(AntigravityEvent::Usage {
        used_tokens,
        window_tokens,
        input_tokens,
        output_tokens,
    })
}

pub fn preferred_allow_option(options: &[Value]) -> Option<String> {
    let by_kind = |kind: &str| {
        options
            .iter()
            .find(|o| o.get("kind").and_then(Value::as_str) == Some(kind))
    };
    by_kind("allow_always")
        .or_else(|| by_kind("allow_once"))
        .or_else(|| options.first())
        .map(|o| str_field(o, "optionId"))
        .filter(|id| !id.is_empty())
}

pub fn preferred_reject_option(options: &[Value]) -> Option<String> {
    options
        .iter()
        .find(|o| {
            matches!(
                o.get("kind").and_then(Value::as_str),
                Some("reject_once" | "reject_always")
            )
        })
        .or_else(|| options.last())
        .map(|o| str_field(o, "optionId"))
        .filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_usage_update_fills_the_meter() {
        let update = json!({
            "sessionUpdate": "usage_update",
            "used": 53000,
            "size": 200000
        });
        let events = map_update(&update);
        assert!(matches!(
            events.as_slice(),
            [AntigravityEvent::Usage {
                used_tokens: 53_000,
                window_tokens: 200_000,
                ..
            }]
        ));
    }

    #[test]
    fn message_chunk_maps() {
        let update = json!({
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hello" },
        });
        assert!(matches!(
            map_update(&update).as_slice(),
            [AntigravityEvent::TextDelta { text }] if text == "hello"
        ));
    }
}

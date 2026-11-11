//! OpenCode SSE reader: wire frames in, harness events out.
//!
//! Split out of `run.rs` so the transport (spawn, control, HTTP) and the
//! protocol (event mapping, tool/subagent routing) stay readable apart.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::Ordering;
use std::time::Instant;

use serde_json::Value;

use super::run::Shared;
use super::types::{OpenCodeDoneStatus, OpenCodeEvent};

/// Tracking for a `task` / `subagent` child session.
#[derive(Default)]
struct ChildState {
    /// Parent tool call id; `None` until the spawn progress frame arrives.
    parent_tool_call_id: Option<String>,
    title: String,
    agent: Option<String>,
}

pub(super) fn read_sse(
    mut sse: reqwest::blocking::Response,
    session_id: &str,
    shared: &Shared,
    model: Option<String>,
    on_event: &dyn Fn(OpenCodeEvent),
) {
    let mut reader = BufReader::new(sse.by_ref());
    let mut data_lines: Vec<String> = Vec::new();
    let mut turn_done = false;
    // Tool call id → (name, input). Input is kept for subagent spawn titles.
    let mut calls: HashMap<String, (String, Value)> = HashMap::new();
    let mut children: HashMap<String, ChildState> = HashMap::new();
    let mut model = model.filter(|id| !id.trim().is_empty());
    let mut buf = String::new();

    loop {
        if shared.shutdown.load(Ordering::Acquire) || shared.interrupted.load(Ordering::Acquire) {
            break;
        }

        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let line = buf.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if data_lines.is_empty() {
                continue;
            }
            let payload = data_lines.join("\n");
            data_lines.clear();
            // A frame is proof the turn moved; the supervisor's watchdog reads
            // this clock instead of guessing from a local timer.
            *shared.last_event.lock() = Instant::now();
            if let Ok(event) = serde_json::from_str::<Value>(&payload) {
                turn_done |= handle_v2_event(
                    &event,
                    session_id,
                    shared,
                    &mut calls,
                    &mut children,
                    &mut model,
                    on_event,
                );
                if turn_done {
                    // The steer gate reads this: after the terminal frame a
                    // steer can never reach the turn.
                    shared.settled.store(true, Ordering::Release);
                    return;
                }
            }
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim().to_string());
        }
    }

    if !turn_done {
        // The stream ended without a terminal frame. A turn only counts as
        // finished when the server says so (`session.execution.*`), so this is
        // a dropped connection — never a silent success.
        let interrupted = shared.interrupted.load(Ordering::Acquire);
        let shutdown = shared.shutdown.load(Ordering::Acquire);
        let (status, error) = if interrupted {
            (OpenCodeDoneStatus::Interrupted, None)
        } else if shutdown {
            // The app let this run go; the transcript is already detached.
            (OpenCodeDoneStatus::Completed, None)
        } else {
            (
                OpenCodeDoneStatus::Errored,
                Some(
                    "OpenCode closed the event stream before the turn finished. Resend to continue."
                        .to_string(),
                ),
            )
        };
        on_event(OpenCodeEvent::Done {
            status,
            error,
            session_id: Some(session_id.into()),
        });
    }
}

/// Returns true when the root turn has finished.
fn handle_v2_event(
    event: &Value,
    session_id: &str,
    shared: &Shared,
    calls: &mut HashMap<String, (String, Value)>,
    children: &mut HashMap<String, ChildState>,
    model: &mut Option<String>,
    on_event: &dyn Fn(OpenCodeEvent),
) -> bool {
    let ty = event["type"].as_str().unwrap_or("");
    let data = &event["data"];
    let sid = data["sessionID"].as_str().unwrap_or("");
    let is_child = !sid.is_empty() && sid != session_id;
    if is_child && !children.contains_key(sid) {
        // Grandchildren and unrelated sessions are not modeled.
        return false;
    }
    if !is_child {
        if matches!(ty, "session.model.selected" | "session.step.started") {
            if let Some(key) = model_key(data) {
                *model = Some(key);
            }
        }
    }
    let emit = |ev: OpenCodeEvent| {
        if is_child {
            on_event(OpenCodeEvent::SubagentEvent {
                session_id: sid.to_string(),
                event: Box::new(ev),
            });
        } else {
            on_event(ev);
        }
    };

    // Session topology and approvals need ownership before normal routing.
    match ty {
        "session.created" => {
            if data["parentID"].as_str() == Some(session_id) {
                if let Some(child_id) = data["sessionID"].as_str().filter(|id| !id.is_empty()) {
                    let title = child_title(data["title"].as_str().unwrap_or(""));
                    let agent = data["agent"].as_str().map(str::to_owned);
                    observe_child(children, shared, child_id, None, title, agent, on_event);
                }
            }
            return false;
        }
        "session.tool.progress" => {
            let call_id = data["id"].as_str().unwrap_or("");
            let child = data.pointer("/metadata/sessionID").and_then(Value::as_str);
            if let (Some(child_id), Some((name, input))) = (child, calls.get(call_id)) {
                if matches!(name.as_str(), "task" | "subagent") && child_id != session_id {
                    let title = spawn_title(input);
                    let agent = input
                        .get("agent")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    observe_child(
                        children,
                        shared,
                        child_id,
                        Some(call_id),
                        title,
                        agent,
                        on_event,
                    );
                }
            }
            return false;
        }
        "permission.asked" => {
            let id = data["id"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                return false;
            }
            let action = data["action"].as_str().unwrap_or("tool");
            let resources = data
                .get("resources")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(3)
                .collect::<Vec<_>>()
                .join(", ");
            let mode = shared.approval_mode.lock().clone();
            if shared.plan_mode.load(Ordering::Acquire)
                && !crate::approval::is_read_only(action)
            {
                shared
                    .auto_replies
                    .lock()
                    .push((id, sid.to_string(), false));
                return false;
            }
            if !crate::approval::should_ask(&mode, action, &resources) {
                shared
                    .auto_replies
                    .lock()
                    .push((id, sid.to_string(), true));
                return false;
            }
            let title = action.to_string();
            let message = if resources.is_empty() {
                format!("Allow `{action}`?")
            } else {
                format!("Allow `{action}` on {resources}?")
            };
            shared.pending_permissions.lock().insert(
                id.clone(),
                super::run::PendingPermission {
                    session_id: sid.to_string(),
                    action: action.to_string(),
                    detail: resources.clone(),
                },
            );
            emit(OpenCodeEvent::InputRequested {
                request_id: id,
                title,
                message,
            });
            return false;
        }
        "permission.replied" | "form.replied" | "form.cancelled" => {
            let id = data["requestID"]
                .as_str()
                .or_else(|| data["id"].as_str())
                .unwrap_or("")
                .to_string();
            if !id.is_empty() {
                let owner = shared.pending_permissions.lock().remove(&id);
                let resolved = OpenCodeEvent::InputResolved { request_id: id };
                match owner {
                    Some(owner) if owner.session_id != session_id => {
                        on_event(OpenCodeEvent::SubagentEvent {
                            session_id: owner.session_id,
                            event: Box::new(resolved),
                        });
                    }
                    _ => on_event(resolved),
                }
            }
            return false;
        }
        "session.execution.succeeded" if is_child => {
            on_event(OpenCodeEvent::SubagentDone {
                session_id: sid.to_string(),
                status: OpenCodeDoneStatus::Completed,
                error: None,
            });
            return false;
        }
        "session.execution.interrupted" if is_child => {
            on_event(OpenCodeEvent::SubagentDone {
                session_id: sid.to_string(),
                status: OpenCodeDoneStatus::Interrupted,
                error: None,
            });
            return false;
        }
        "session.execution.failed" if is_child => {
            on_event(OpenCodeEvent::SubagentDone {
                session_id: sid.to_string(),
                status: OpenCodeDoneStatus::Errored,
                error: Some(execution_error(data)),
            });
            return false;
        }
        _ => {}
    }

    match ty {
        "session.text.delta" => {
            let delta = data["delta"].as_str().unwrap_or("");
            if !delta.is_empty() {
                emit(OpenCodeEvent::TextDelta { text: delta.into() });
            }
        }
        "session.reasoning.delta" => {
            let delta = data["delta"].as_str().unwrap_or("");
            if !delta.is_empty() {
                emit(OpenCodeEvent::ReasoningDelta { text: delta.into() });
            }
        }
        "session.tool.input.started" => {
            let id = data["id"].as_str().unwrap_or("").to_string();
            let name = data["name"].as_str().unwrap_or("tool").to_string();
            if !id.is_empty() {
                calls.insert(id, (name, Value::Null));
            }
        }
        "session.tool.called" => {
            let id = data["id"].as_str().unwrap_or("").to_string();
            let args = data.get("input").cloned().unwrap_or(Value::Null);
            let name = calls
                .get(&id)
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| "tool".into());
            calls.insert(id.clone(), (name.clone(), args.clone()));
            emit(OpenCodeEvent::ToolCall { id, name, args });
        }
        "session.tool.success" | "session.tool.failed" => {
            let id = data["id"].as_str().unwrap_or("").to_string();
            let is_error = ty == "session.tool.failed";
            // The call is dropped here, so take its name and input first: an
            // unsupported interactive tool is described from its arguments.
            let (name, input) = calls.remove(&id).unwrap_or_default();
            let output = if is_error && is_interactive_tool(&name) {
                interactive_tool_failure(&input)
            } else {
                tool_output_text(data, is_error)
            };
            emit(OpenCodeEvent::ToolResult {
                id,
                output: Some(output),
                is_error,
            });
        }
        "session.step.ended" | "session.next.step.ended" if !is_child => {
            if let Some(event) = usage_from_step(model.as_deref().unwrap_or(""), data) {
                on_event(event);
            }
        }
        "session.execution.started" => {}
        "session.execution.succeeded" => {
            on_event(OpenCodeEvent::Done {
                status: OpenCodeDoneStatus::Completed,
                error: None,
                session_id: Some(session_id.into()),
            });
            return true;
        }
        "session.execution.interrupted" => {
            on_event(OpenCodeEvent::Done {
                status: OpenCodeDoneStatus::Interrupted,
                error: None,
                session_id: Some(session_id.into()),
            });
            return true;
        }
        "session.execution.failed" => {
            on_event(OpenCodeEvent::Done {
                status: OpenCodeDoneStatus::Errored,
                error: Some(execution_error(data)),
                session_id: Some(session_id.into()),
            });
            return true;
        }
        _ => {}
    }
    false
}

/// `provider/id` from a model-selected or step-started frame.
fn model_key(data: &Value) -> Option<String> {
    let model = data.get("model").unwrap_or(data);
    if let Some(text) = model.as_str().map(str::trim).filter(|text| !text.is_empty()) {
        return Some(text.to_string());
    }
    let id = model
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    let provider = model
        .get("providerID")
        .or_else(|| model.get("provider"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|provider| !provider.is_empty());
    Some(match provider {
        Some(provider) if !id.contains('/') => format!("{provider}/{id}"),
        _ => id.to_string(),
    })
}

/// `session.step.ended` carries the step's token snapshot. The window comes
/// from the model id so the composer meter has a denominator.
fn usage_from_step(model: &str, data: &Value) -> Option<OpenCodeEvent> {
    let tokens = data.get("tokens")?;
    let input = crate::usage::json_count(tokens.get("input"));
    let output = crate::usage::json_count(tokens.get("output"))
        .saturating_add(crate::usage::json_count(tokens.get("reasoning")));
    let cache = tokens.get("cache");
    let extra = crate::usage::json_count(cache.and_then(|item| item.get("read")))
        .saturating_add(crate::usage::json_count(cache.and_then(|item| item.get("write"))));
    let meter = crate::usage::meter(model, input, output, extra, None)?;
    Some(OpenCodeEvent::Usage {
        used_tokens: meter.used_tokens,
        window_tokens: meter.window_tokens,
        input_tokens: meter.input_tokens,
        output_tokens: meter.output_tokens,
    })
}

fn execution_error(data: &Value) -> String {
    let kind = data
        .pointer("/error/type")
        .and_then(Value::as_str)
        .unwrap_or("error");
    let message = data
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("turn failed");
    format!("{kind}: {message}")
}

fn child_title(raw: &str) -> String {
    let title = raw
        .rsplit_once(" (@")
        .map_or(raw, |(description, _)| description)
        .trim();
    if title.is_empty() {
        "Subagent".into()
    } else {
        title.to_string()
    }
}

fn spawn_title(input: &Value) -> String {
    input
        .get("description")
        .or_else(|| input.get("prompt"))
        .and_then(Value::as_str)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or(value)
                .chars()
                .take(120)
                .collect::<String>()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Subagent".into())
}

/// Track a child session and emit Started/Bound once each, in either arrival order.
fn observe_child(
    children: &mut HashMap<String, ChildState>,
    shared: &Shared,
    session_id: &str,
    parent_tool_call_id: Option<&str>,
    title: String,
    agent: Option<String>,
    on_event: &dyn Fn(OpenCodeEvent),
) {
    let mut started = None;
    let mut bound = None;
    match children.entry(session_id.to_string()) {
        std::collections::hash_map::Entry::Vacant(slot) => {
            let state = slot.insert(ChildState {
                parent_tool_call_id: parent_tool_call_id.map(str::to_owned),
                title: title.clone(),
                agent: agent.clone(),
            });
            shared.child_sessions.lock().push(session_id.to_string());
            started = Some(OpenCodeEvent::SubagentStarted {
                parent_tool_call_id: state.parent_tool_call_id.clone(),
                session_id: session_id.to_string(),
                title,
                agent,
            });
        }
        std::collections::hash_map::Entry::Occupied(mut slot) => {
            let state = slot.get_mut();
            if !title.is_empty() && (state.title.is_empty() || state.title == "Subagent") {
                state.title = title;
            }
            if state.agent.is_none() {
                state.agent = agent;
            }
            if state.parent_tool_call_id.is_none() {
                if let Some(parent) = parent_tool_call_id {
                    state.parent_tool_call_id = Some(parent.to_string());
                    bound = Some(OpenCodeEvent::SubagentBound {
                        session_id: session_id.to_string(),
                        parent_tool_call_id: parent.to_string(),
                    });
                }
            }
        }
    }
    if let Some(event) = started {
        on_event(event);
    }
    if let Some(event) = bound {
        on_event(event);
    }
}

/// `question`-style tools need a client answer channel Halo does not have, so
/// the server fails them and echoes the argument JSON back as the "output".
/// Showing that blob tells the user nothing: repeat the question in words and
/// say what to do instead.
fn interactive_tool_failure(input: &Value) -> String {
    let first = input
        .get("questions")
        .and_then(Value::as_array)
        .and_then(|list| list.first());
    let question = input
        .get("question")
        .or_else(|| first.and_then(|q| q.get("question")))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let asked = if question.is_empty() {
        String::new()
    } else {
        let short: String = question.chars().take(200).collect();
        format!(" The agent asked: \"{short}\".")
    };
    format!(
        "Halo can't answer interactive questions, so this one was cancelled.{asked} \
         Reply in the composer instead — the turn keeps its context."
    )
}

fn is_interactive_tool(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "question" | "ask" | "ask_user" | "askuser" | "request_user_input" | "user_input"
    )
}

fn tool_output_text(data: &Value, is_error: bool) -> String {
    let blocks = data["content"].as_array().cloned().unwrap_or_default();
    let text = blocks
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        return text;
    }
    if is_error {
        return data
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("tool failed")
            .to_string();
    }
    data["output"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::OpenCodeEvent;
    use serde_json::json;

    #[test]
    fn a_failed_question_explains_itself_instead_of_dumping_json() {
        // What the server actually sends back for `question`: the argument blob.
        let input = json!({
            "questions": [{
                "header": "Grant permissions",
                "question": "Want me to launch the guided permission flow?",
                "options": [{ "label": "Run the guide" }]
            }]
        });
        let text = interactive_tool_failure(&input);
        assert!(!text.contains('{'), "raw JSON leaked: {text}");
        assert!(text.contains("Want me to launch the guided permission flow?"));
        assert!(text.contains("Reply in the composer"));
    }

    #[test]
    fn a_question_without_text_still_reads_as_a_sentence() {
        let text = interactive_tool_failure(&Value::Null);
        assert!(!text.contains("asked:"));
        assert!(text.starts_with("Halo can't answer interactive questions"));
    }

    #[test]
    fn a_step_end_fills_the_context_meter() {
        let data = json!({
            "sessionID": "ses_1",
            "tokens": {
                "input": 1200,
                "output": 300,
                "reasoning": 20,
                "cache": { "read": 800, "write": 0 }
            }
        });
        let OpenCodeEvent::Usage {
            used_tokens,
            window_tokens,
            input_tokens,
            output_tokens,
        } = usage_from_step("anthropic/claude-sonnet-5-5", &data).unwrap()
        else {
            panic!("step end must produce usage");
        };
        assert_eq!(input_tokens, 1200);
        assert_eq!(output_tokens, 320);
        assert_eq!(used_tokens, 1200 + 320 + 800);
        assert_eq!(window_tokens, 200_000);
    }

    #[test]
    fn other_failures_keep_the_server_message() {
        let data = json!({ "error": { "message": "permission denied" } });
        assert_eq!(tool_output_text(&data, true), "permission denied");
    }
}

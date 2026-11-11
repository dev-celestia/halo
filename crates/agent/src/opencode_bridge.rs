//! OpenCode harness bridge — maps HTTP/SSE driver events to Halo Agent events.

use halo_harness::{OpenCodeDoneStatus, OpenCodeEvent};
use serde_json::Value;

use super::types::{AgentEvent, DoneStatus, ToolCall};

pub fn map_opencode_event(event: OpenCodeEvent) -> AgentEvent {
    match event {
        OpenCodeEvent::SessionStarted {
            session_id,
            model,
            cwd,
        } => AgentEvent::SessionStarted {
            harness: "opencode".into(),
            model: model.unwrap_or_default(),
            tools: vec!["read".into(), "write".into(), "edit".into(), "bash".into()],
            cwd: cwd.unwrap_or_default(),
            session_id,
        },
        OpenCodeEvent::TextDelta { text } => AgentEvent::TextDelta { text },
        OpenCodeEvent::ReasoningDelta { text } => AgentEvent::ReasoningDelta { text },
        OpenCodeEvent::CommandAccepted { command_id } => AgentEvent::CommandAccepted { command_id },
        OpenCodeEvent::CommandRejected { command_id, error } => {
            AgentEvent::CommandRejected { command_id, error }
        }
        OpenCodeEvent::ToolCall { id, name, args } => {
            let call = match name.to_ascii_lowercase().as_str() {
                "read" | "read_file" => {
                    let path = args
                        .get("path")
                        .or_else(|| args.get("filePath"))
                        .or_else(|| args.get("file"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ToolCall::ReadFile { path }
                }
                "write" | "write_file" => {
                    let path = args
                        .get("path")
                        .or_else(|| args.get("filePath"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let content = args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    ToolCall::WriteFile { path, content }
                }
                "edit" | "edit_file" | "multiedit" => {
                    let path = args
                        .get("path")
                        .or_else(|| args.get("filePath"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ToolCall::EditFile {
                        path,
                        old_string: None,
                        new_string: None,
                    }
                }
                "bash" | "shell" | "exec" => {
                    let command = args
                        .get("command")
                        .or_else(|| args.get("cmd"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    match super::browser::classify_command(&command) {
                        Some(found) => ToolCall::Browser {
                            action: found.action,
                            target: found.target,
                        },
                        None => ToolCall::Exec { command },
                    }
                }
                "websearch" | "web_search" => ToolCall::WebSearch {
                    query: args
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                },
                "webfetch" | "web_fetch" => ToolCall::WebFetch {
                    url: args
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                },
                "grep" | "search" => {
                    let pattern = args
                        .get("pattern")
                        .or_else(|| args.get("query"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ToolCall::Search {
                        pattern,
                        path: None,
                    }
                }
                "glob" => {
                    let pattern = args
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ToolCall::Glob { pattern }
                }
                "task" | "subagent" => ToolCall::Task {
                    description: args
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    agent: args
                        .get("agent")
                        .or_else(|| args.get("subagent_type"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    prompt: args
                        .get("prompt")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                },
                // `question` takes either flat arguments or a `questions` list;
                // both are read so the card never falls back to raw JSON.
                "question" | "ask" | "ask_user" | "request_user_input" => {
                    let first = args
                        .get("questions")
                        .and_then(Value::as_array)
                        .and_then(|list| list.first());
                    let field = |key: &str| {
                        args.get(key)
                            .or_else(|| first.and_then(|q| q.get(key)))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    };
                    let options = args
                        .get("options")
                        .or_else(|| first.and_then(|q| q.get("options")))
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter_map(|option| {
                                    option.as_str().map(str::to_string).or_else(|| {
                                        option
                                            .get("label")
                                            .and_then(Value::as_str)
                                            .map(str::to_string)
                                    })
                                })
                                .filter(|label| !label.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                    ToolCall::Question {
                        header: field("header"),
                        question: field("question"),
                        options,
                    }
                }
                other => ToolCall::Unknown {
                    name: other.to_string(),
                    input_json: Some(args.to_string()),
                },
            };
            AgentEvent::ToolCall { id, call }
        }
        OpenCodeEvent::ToolResult {
            id,
            output,
            is_error,
        } => AgentEvent::ToolResult {
            id,
            output,
            is_error,
        },
        OpenCodeEvent::InputRequested {
            request_id,
            title,
            message,
        } => AgentEvent::InputRequested {
            questions: vec![super::types::UserInputQuestion {
                id: request_id.clone(),
                header: title,
                question: message,
                options: vec!["Allow".into(), "Block".into()],
                multi_select: false,
            }],
            request_id,
        },
        OpenCodeEvent::InputResolved { request_id } => AgentEvent::InputResolved { request_id },
        OpenCodeEvent::Usage {
            used_tokens,
            window_tokens,
            input_tokens,
            output_tokens,
        } => AgentEvent::Usage {
            used_tokens,
            window_tokens,
            input_tokens,
            output_tokens,
        },
        OpenCodeEvent::Done {
            status,
            error,
            session_id,
        } => AgentEvent::Done {
            status: map_done_status(status),
            result: None,
            error,
            session_id,
        },
        OpenCodeEvent::SubagentStarted {
            parent_tool_call_id,
            session_id,
            title,
            agent,
        } => AgentEvent::SubagentStarted {
            parent_tool_call_id,
            session_id,
            title,
            agent,
        },
        OpenCodeEvent::SubagentBound {
            session_id,
            parent_tool_call_id,
        } => AgentEvent::SubagentBound {
            session_id,
            parent_tool_call_id,
        },
        // Child material is always wrapped so it can never retire the root run.
        OpenCodeEvent::SubagentEvent { session_id, event } => AgentEvent::SubagentEvent {
            session_id,
            event: Box::new(map_opencode_event(*event)),
        },
        OpenCodeEvent::SubagentDone {
            session_id,
            status,
            error,
        } => AgentEvent::SubagentDone {
            session_id,
            status: map_done_status(status),
            error,
        },
    }
}

fn map_done_status(status: OpenCodeDoneStatus) -> DoneStatus {
    match status {
        OpenCodeDoneStatus::Completed => DoneStatus::Completed,
        OpenCodeDoneStatus::Interrupted => DoneStatus::Interrupted,
        OpenCodeDoneStatus::Errored => DoneStatus::Errored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool_call(name: &str, args: Value) -> AgentEvent {
        map_opencode_event(OpenCodeEvent::ToolCall {
            id: "call-1".into(),
            name: name.into(),
            args,
        })
    }

    #[test]
    fn question_reads_the_nested_questions_list() {
        let event = tool_call(
            "question",
            json!({
                "questions": [{
                    "question": "Which permissions should I request?",
                    "header": "Grant permissions",
                    "options": [
                        { "label": "Run the guide", "description": "…" },
                        { "label": "I'll grant manually" }
                    ]
                }]
            }),
        );
        let AgentEvent::ToolCall { call, .. } = event else {
            panic!("expected a tool call");
        };
        assert_eq!(
            call,
            ToolCall::Question {
                header: "Grant permissions".into(),
                question: "Which permissions should I request?".into(),
                options: vec!["Run the guide".into(), "I'll grant manually".into()],
            }
        );
        // The card title names the question, never the raw tool id.
        assert_eq!(call.label(), "Question · Grant permissions");
    }

    #[test]
    fn question_accepts_flat_arguments_too() {
        let AgentEvent::ToolCall { call, .. } = tool_call(
            "ask_user",
            json!({ "question": "Proceed?", "options": ["Yes", "No"] }),
        ) else {
            panic!("expected a tool call");
        };
        assert_eq!(
            call,
            ToolCall::Question {
                header: String::new(),
                question: "Proceed?".into(),
                options: vec!["Yes".into(), "No".into()],
            }
        );
        assert_eq!(call.label(), "Question · Proceed?");
    }

    #[test]
    fn unknown_tools_keep_their_name() {
        let AgentEvent::ToolCall { call, .. } = tool_call("weird_tool", json!({ "a": 1 })) else {
            panic!("expected a tool call");
        };
        assert_eq!(call.label(), "weird_tool");
    }
}

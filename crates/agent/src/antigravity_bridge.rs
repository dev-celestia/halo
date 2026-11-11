//! Antigravity ACP harness bridge — maps driver events to Halo Agent events.

use halo_harness::{AntigravityDoneStatus, AntigravityEvent};

use super::types::{AgentEvent, DoneStatus, ToolCall};

pub fn map_antigravity_event(event: AntigravityEvent) -> AgentEvent {
    match event {
        AntigravityEvent::SessionStarted {
            session_id,
            model,
            cwd,
        } => AgentEvent::SessionStarted {
            harness: "antigravity".into(),
            model: model.unwrap_or_default(),
            tools: vec!["read".into(), "write".into(), "edit".into(), "bash".into()],
            cwd: cwd.unwrap_or_default(),
            session_id,
        },
        AntigravityEvent::TextDelta { text } => AgentEvent::TextDelta { text },
        AntigravityEvent::ReasoningDelta { text } => AgentEvent::ReasoningDelta { text },
        AntigravityEvent::CommandAccepted { command_id } => {
            AgentEvent::CommandAccepted { command_id }
        }
        AntigravityEvent::ToolCall { id, name, args } => {
            let call = match name.to_ascii_lowercase().as_str() {
                "read" | "read_file" | "view" => {
                    let path = args
                        .get("path")
                        .or_else(|| args.get("filePath"))
                        .or_else(|| args.get("file_path"))
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
                "bash" | "shell" | "exec" | "terminal" | "execute" => {
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
                "glob" | "find" | "list" => {
                    let pattern = args
                        .get("pattern")
                        .or_else(|| args.get("path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ToolCall::Glob { pattern }
                }
                other => ToolCall::Unknown {
                    name: other.to_string(),
                    input_json: Some(args.to_string()),
                },
            };
            AgentEvent::ToolCall { id, call }
        }
        AntigravityEvent::ToolResult {
            id,
            output,
            is_error,
        } => AgentEvent::ToolResult {
            id,
            output,
            is_error,
        },
        AntigravityEvent::InputRequested {
            request_id,
            title,
            message,
            options,
        } => AgentEvent::InputRequested {
            questions: vec![super::types::UserInputQuestion {
                id: request_id.clone(),
                header: title,
                question: message,
                options: if options.is_empty() {
                    vec!["Allow".into(), "Block".into()]
                } else {
                    options
                },
                multi_select: false,
            }],
            request_id,
        },
        AntigravityEvent::InputResolved { request_id } => AgentEvent::InputResolved { request_id },
        AntigravityEvent::Usage {
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
        AntigravityEvent::Done {
            status,
            error,
            session_id,
        } => AgentEvent::Done {
            status: match status {
                AntigravityDoneStatus::Completed => DoneStatus::Completed,
                AntigravityDoneStatus::Interrupted => DoneStatus::Interrupted,
                AntigravityDoneStatus::Errored => DoneStatus::Errored,
            },
            result: None,
            error,
            session_id,
        },
    }
}

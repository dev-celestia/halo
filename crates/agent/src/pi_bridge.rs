//! Pi harness integration bridge — maps Pi JSON-L events to Halo Coder events.

use halo_harness::{PiDoneStatus, PiEvent};

use super::types::{AgentEvent, DoneStatus, ToolCall};

pub fn map_pi_event(event: PiEvent) -> AgentEvent {
    match event {
        PiEvent::SessionStarted {
            session_id,
            model,
            cwd,
        } => AgentEvent::SessionStarted {
            harness: "pi".into(),
            model,
            tools: vec!["read".into(), "write".into(), "edit".into(), "bash".into()],
            cwd,
            session_id,
        },
        PiEvent::TextDelta { text } => AgentEvent::TextDelta { text },
        PiEvent::ReasoningDelta { text } => AgentEvent::ReasoningDelta { text },
        PiEvent::ContentUpdated {
            message_id,
            content_index,
            text,
            reasoning,
            append,
            done,
        } => AgentEvent::ContentUpdated {
            message_id,
            content_index,
            text,
            reasoning,
            append,
            done,
        },
        PiEvent::CommandAccepted { command_id } => AgentEvent::CommandAccepted { command_id },
        PiEvent::CommandRejected { command_id, error } => {
            AgentEvent::CommandRejected { command_id, error }
        }
        PiEvent::InputRequested {
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
        PiEvent::InputResolved { request_id } => AgentEvent::InputResolved { request_id },
        PiEvent::Usage {
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
        PiEvent::ToolCall { id, name, args } => {
            let call = match name.as_str() {
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
                        .or_else(|| args.get("file"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let content = args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    ToolCall::WriteFile { path, content }
                }
                "edit" | "edit_file" => {
                    let path = args
                        .get("path")
                        .or_else(|| args.get("filePath"))
                        .or_else(|| args.get("file"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let old_string = args
                        .get("oldText")
                        .or_else(|| args.get("old_string"))
                        .or_else(|| args.get("search"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let new_string = args
                        .get("newText")
                        .or_else(|| args.get("new_string"))
                        .or_else(|| args.get("replace"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    ToolCall::EditFile {
                        path,
                        old_string,
                        new_string,
                    }
                }
                "bash" | "exec" | "command" => {
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
                _ => ToolCall::Unknown {
                    name,
                    input_json: Some(args.to_string()),
                },
            };
            AgentEvent::ToolCall { id, call }
        }
        PiEvent::ToolResult {
            id,
            is_error,
            output,
        } => AgentEvent::ToolResult {
            id,
            is_error,
            output,
        },
        PiEvent::Done {
            status,
            session_id,
            error,
        } => {
            let done_status = match status {
                PiDoneStatus::Completed => DoneStatus::Completed,
                PiDoneStatus::Interrupted => DoneStatus::Interrupted,
                PiDoneStatus::Errored => DoneStatus::Errored,
            };
            AgentEvent::Done {
                status: done_status,
                result: None,
                error,
                session_id,
            }
        }
        PiEvent::Error { message } => AgentEvent::Error { message },
    }
}

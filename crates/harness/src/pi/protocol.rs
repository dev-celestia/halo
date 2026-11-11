//! The verified 1.4.0 wire vocabulary; no prompt replay or guessed command aliases.
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::process::{MAX_COMMAND, MAX_FRAME};
use super::types::{PiEvent, PiImage};

pub(crate) const MAX_PENDING: usize = 16;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Frame {
    Response {
        id: Option<String>,
        command: String,
        success: bool,
        data: Option<Value>,
        error: Option<String>,
    },
    AgentStart,
    AgentEnd,
    AgentSettled {
        aborted: bool,
    },
    MessageStart {
        message: Message,
    },
    MessageEnd {
        message: Message,
    },
    MessageUpdate {
        #[serde(rename = "assistantMessageEvent")]
        update: Update,
    },
    ToolExecutionStart {
        #[serde(rename = "toolCallId")]
        id: String,
        #[serde(rename = "toolName")]
        name: String,
        args: Value,
    },
    ToolExecutionEnd {
        #[serde(rename = "toolCallId")]
        id: String,
        #[serde(rename = "isError")]
        is_error: bool,
        result: Value,
    },
    AutoRetryEnd {
        success: bool,
        #[serde(rename = "finalError")]
        final_error: Option<String>,
    },
    ExtensionUiRequest {
        id: String,
        method: String,
        title: Option<String>,
        message: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "statusKey")]
        status_key: Option<String>,
        #[serde(rename = "statusText")]
        status_text: Option<String>,
    },
    ExtensionError,
    Error {
        message: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub(crate) struct Message {
    pub role: String,
    #[serde(default)]
    pub content: Value,
    #[serde(rename = "stopReason")]
    pub stop_reason: Option<String>,
    #[serde(rename = "errorMessage")]
    pub error_message: Option<String>,
    #[serde(default)]
    pub usage: Option<MessageUsage>,
}

/// Token counts on an assistant `message_end` (Pi 1.4.0 camelCase).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
}

impl MessageUsage {
    /// Context footprint after the turn — input + output + cache tokens.
    pub fn context_tokens(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Update {
    TextDelta {
        #[serde(rename = "contentIndex")]
        index: usize,
        delta: String,
    },
    ThinkingDelta {
        #[serde(rename = "contentIndex")]
        index: usize,
        delta: String,
    },
    TextEnd {
        #[serde(rename = "contentIndex")]
        index: usize,
        content: String,
    },
    ThinkingEnd {
        #[serde(rename = "contentIndex")]
        index: usize,
        content: String,
    },
    Error {
        error: Message,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Clone)]
pub(crate) struct Model {
    pub id: String,
    pub provider: String,
    #[serde(default, rename = "contextWindow")]
    pub context_window: Option<u64>,
}

impl Model {
    pub fn key(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct State {
    pub model: Option<Model>,
    pub session_file: Option<String>,
    pub session_id: String,
    pub is_streaming: bool,
    pub is_compacting: bool,
}

#[derive(Deserialize)]
pub(crate) struct Models {
    pub models: Vec<Model>,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Frame, String> {
    serde_json::from_slice(bytes)
        .map_err(|_| "Invalid Pi RPC record (expected the documented 1.4.0 JSONL protocol)".into())
}

pub(crate) fn command(
    id: &str,
    kind: &str,
    prompt: &str,
    images: Vec<PiImage>,
) -> Result<Vec<u8>, String> {
    validate_id(id)?;
    if prompt.len() > 1024 * 1024 || images.len() > 8 {
        return Err("Pi prompt or attachment count exceeds the limit".into());
    }
    let image_size: usize = images.iter().map(|i| i.data.len()).sum();
    if image_size > 10 * 1024 * 1024 {
        return Err("Pi images exceed the 10 MiB encoded payload limit".into());
    }
    for image in &images {
        if !matches!(
            image.mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ) || image.data.is_empty()
            || image.data.len() % 4 != 0
            || !image
                .data
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        {
            return Err("Pi images must be base64 PNG, JPEG, GIF or WebP payloads".into());
        }
    }
    let bytes = serde_json::to_vec(&json!({"id":id,"type":kind,"message":prompt,"images":images}))
        .map_err(|_| "Cannot encode Pi command")?;
    if bytes.len() > MAX_COMMAND {
        return Err("Pi command exceeds the 12 MiB limit".into());
    }
    Ok(bytes)
}

pub(crate) fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 256
        || id.starts_with("halo-internal-")
        || id.chars().any(char::is_control)
    {
        Err("Pi command requires a unique, nonempty run/command ID (maximum 256 bytes)".into())
    } else {
        Ok(())
    }
}

pub(crate) fn brief(text: String) -> String {
    text.chars().take(4096).collect()
}

fn result_text(value: Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_owned());
    }
    let content = value.get("content")?.as_array()?;
    let texts: Vec<_> = content
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

#[derive(Default)]
struct Block {
    text: String,
    reasoning: bool,
    done: bool,
}

#[derive(Default)]
pub(crate) struct Transcript {
    message_id: Option<String>,
    closed: bool,
    blocks: BTreeMap<usize, Block>,
    pub error: Option<String>,
    pub aborted: bool,
}

impl Transcript {
    fn begin(&mut self, out: &mut Vec<PiEvent>) {
        self.finish_blocks(out);
        self.blocks.clear();
        self.message_id = Some(Uuid::new_v4().to_string());
        self.closed = false;
    }

    fn update(
        &mut self,
        index: usize,
        text: String,
        reasoning: bool,
        append: bool,
        done: bool,
        out: &mut Vec<PiEvent>,
    ) -> Result<(), String> {
        if index >= 256 {
            return Err("Pi contentIndex exceeds the 256-block response limit".into());
        }
        if self.message_id.is_none() || self.closed {
            self.begin(out);
        }
        let size: usize = self.blocks.values().map(|b| b.text.len()).sum();
        let old = self.blocks.get(&index).map_or(0, |b| b.text.len());
        if size - if append { 0 } else { old } + text.len() > MAX_FRAME {
            return Err("Pi assistant response exceeds the 4 MiB text limit".into());
        }
        let block = self.blocks.entry(index).or_default();
        let changed =
            append || block.text != text || block.done != done || block.reasoning != reasoning;
        if append {
            block.text.push_str(&text);
        } else {
            block.text.clone_from(&text);
        }
        block.reasoning = reasoning;
        block.done = done;
        if changed {
            out.push(PiEvent::ContentUpdated {
                message_id: self.message_id.clone().unwrap(),
                content_index: index,
                text,
                reasoning,
                append,
                done,
            });
        }
        Ok(())
    }

    pub fn finish_blocks(&mut self, out: &mut Vec<PiEvent>) {
        if let Some(id) = &self.message_id {
            for (&index, block) in &mut self.blocks {
                if !block.done {
                    out.push(PiEvent::ContentUpdated {
                        message_id: id.clone(),
                        content_index: index,
                        text: block.text.clone(),
                        reasoning: block.reasoning,
                        append: false,
                        done: true,
                    });
                    block.done = true;
                }
            }
        }
    }

    fn outcome(&mut self, message: &Message) {
        match message.stop_reason.as_deref() {
            Some("error") => {
                self.error = Some(brief(
                    message
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Pi provider request failed".into()),
                ))
            }
            Some("aborted") => self.aborted = true,
            Some("stop" | "length" | "toolUse") => self.error = None,
            _ => {}
        }
    }

    pub fn handle(&mut self, frame: Frame, out: &mut Vec<PiEvent>) -> Result<(), String> {
        match frame {
            Frame::MessageStart { message } if message.role == "assistant" => self.begin(out),
            Frame::MessageUpdate { update } => match update {
                Update::TextDelta { index, delta } => {
                    self.update(index, delta, false, true, false, out)?
                }
                Update::ThinkingDelta { index, delta } => {
                    self.update(index, delta, true, true, false, out)?
                }
                Update::TextEnd { index, content } => {
                    self.update(index, content, false, false, true, out)?
                }
                Update::ThinkingEnd { index, content } => {
                    self.update(index, content, true, false, true, out)?
                }
                Update::Error { error } => self.outcome(&error),
                _ => {}
            },
            Frame::MessageEnd { message } if message.role == "assistant" => {
                self.outcome(&message);
                if self.closed {
                    return Ok(());
                }
                if let Some(content) = message.content.as_array() {
                    if content.len() > 256 {
                        return Err("Pi message has too many content blocks".into());
                    }
                    let mut indices = Vec::new();
                    for (index, block) in content.iter().enumerate() {
                        let reasoning = block["type"] == "thinking";
                        if let Some(text) =
                            block[if reasoning { "thinking" } else { "text" }].as_str()
                        {
                            indices.push(index);
                            self.update(index, text.to_owned(), reasoning, false, true, out)?;
                        }
                    }
                    // Authoritative snapshots can remove provisional blocks too.
                    let removed: Vec<_> = self
                        .blocks
                        .keys()
                        .copied()
                        .filter(|i| !indices.contains(i))
                        .collect();
                    for index in removed {
                        let reasoning = self.blocks[&index].reasoning;
                        self.update(index, String::new(), reasoning, false, true, out)?;
                    }
                }
                self.finish_blocks(out);
                self.closed = true;
            }
            Frame::ToolExecutionStart { id, name, args } => {
                out.push(PiEvent::ToolCall { id, name, args })
            }
            Frame::ToolExecutionEnd {
                id,
                is_error,
                result,
            } => out.push(PiEvent::ToolResult {
                id,
                is_error,
                output: result_text(result),
            }),
            Frame::AutoRetryEnd {
                success,
                final_error,
            } => {
                self.error = if success {
                    None
                } else {
                    Some(brief(
                        final_error.unwrap_or_else(|| "Pi exhausted provider retries".into()),
                    ))
                };
            }
            Frame::Error { message } => {
                self.error = Some(brief(message.unwrap_or_else(|| "Pi provider error".into())))
            }
            _ => {}
        }
        Ok(())
    }
}

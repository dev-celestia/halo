//! Data types, options, and event models for Pi agent harness.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiModelInfo {
    pub id: String,
    pub display_name: String,
    pub provider: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiDoneStatus {
    Completed,
    Interrupted,
    Errored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiEvent {
    SessionStarted {
        session_id: String,
        model: String,
        cwd: String,
    },
    TextDelta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    ToolResult {
        id: String,
        is_error: bool,
        output: Option<String>,
    },
    Done {
        status: PiDoneStatus,
        session_id: Option<String>,
        error: Option<String>,
    },
    Error {
        message: String,
    },
    CommandAccepted {
        command_id: String,
    },
    CommandRejected {
        command_id: String,
        error: String,
    },
    ContentUpdated {
        message_id: String,
        content_index: usize,
        text: String,
        reasoning: bool,
        append: bool,
        done: bool,
    },
    InputRequested {
        request_id: String,
        title: String,
        message: String,
    },
    InputResolved {
        request_id: String,
    },
    /// Latest context window fill from an assistant message (or session stats).
    Usage {
        used_tokens: u64,
        window_tokens: u64,
        input_tokens: u64,
        output_tokens: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PiImage {
    pub data: String,
    #[serde(rename = "mimeType", alias = "mime_type")]
    pub mime_type: String,
}

impl Serialize for PiImage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut image = serializer.serialize_struct("PiImage", 3)?;
        image.serialize_field("type", "image")?;
        image.serialize_field("data", &self.data)?;
        image.serialize_field("mimeType", &self.mime_type)?;
        image.end()
    }
}

// Deliberately not Debug: options contain prompts, images and provider credentials.
#[derive(Clone)]
pub struct PiRunOptions {
    pub run_id: String,
    pub resume: Option<String>,
    pub images: Vec<PiImage>,
    /// `"ask"` | `"approve_edits"` | `"approve_safe"` | `"full_access"`.
    pub approval_mode: String,
    pub plan_mode: bool,
    pub prompt: String,
    pub model: Option<String>,
    pub cwd: String,
    pub custom_executable: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub deepseek_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
    pub extra_env: HashMap<String, String>,
}

/// Wire protocol and integration pin for `@earendil-works/pi-coding-agent`.
pub const REQUIRED_VERSION: &str = "1.4.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PiProbeResult {
    Installed {
        version: String,
        path: PathBuf,
    },
    /// A `pi` binary exists but is not the pinned integration version.
    WrongVersion {
        found: String,
        path: PathBuf,
    },
    BinaryNotFound {
        node_available: bool,
    },
    NodeMissing,
    ProbeFailed(String),
}

impl PiProbeResult {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Installed { version, .. } if version == REQUIRED_VERSION)
    }

    pub fn setup_hint(&self) -> String {
        match self {
            Self::Installed { version, path } if version == REQUIRED_VERSION => {
                format!("Pi {version} ready ({})", path.display())
            }
            Self::Installed { version, path }
            | Self::WrongVersion {
                found: version,
                path,
            } => {
                format!(
                    "Found Pi {version} at {}, but Halo needs @earendil-works/pi-coding-agent {REQUIRED_VERSION}.",
                    path.display()
                )
            }
            Self::BinaryNotFound {
                node_available: true,
            } => {
                format!(
                    "Pi is not installed. Install @earendil-works/pi-coding-agent {REQUIRED_VERSION} to use Halo Agent."
                )
            }
            Self::BinaryNotFound {
                node_available: false,
            } => {
                format!(
                    "Pi and Node.js are missing. Install Node.js ≥ 22.19, then @earendil-works/pi-coding-agent {REQUIRED_VERSION}."
                )
            }
            Self::NodeMissing => {
                "Pi needs Node.js ≥ 22.19 beside the executable or on PATH.".into()
            }
            Self::ProbeFailed(error) => error.clone(),
        }
    }
}

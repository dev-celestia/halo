//! Error type shared by the LSP client, URI helpers and server registry.

use thiserror::Error;

use crate::stdio::FrameError;

#[derive(Debug, Error)]
pub enum LspError {
    #[error("{0}")]
    Message(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame: {0}")]
    Frame(#[from] FrameError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server error {code}: {message}")]
    Server { code: i64, message: String },
    #[error("request timed out")]
    Timeout,
    #[error("language server not running")]
    NotRunning,
}

impl LspError {
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

pub type LspResult<T> = Result<T, LspError>;

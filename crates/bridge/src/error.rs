use thiserror::Error;

/// Errors returned from bridge commands to the UI.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BridgeError {
    #[error("{0}")]
    Message(String),

    #[error("backend not installed — call Bridge::install during bootstrap")]
    NotInstalled,

    #[error("not implemented yet (Track 2)")]
    NotImplemented,
}

impl BridgeError {
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

pub type BridgeResult<T> = Result<T, BridgeError>;

//! Shared client/server transport state: the reader thread's message dispatch
//! loop and the pending-request table that resolves it.

use std::collections::HashMap;
use std::process::ChildStdin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use lsp_types::notification::{Notification as _, Progress, PublishDiagnostics};
use lsp_types::{ProgressParams, PublishDiagnosticsParams};
use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::stdio::write_frame;

use super::error::{LspError, LspResult};

/// Incoming `textDocument/publishDiagnostics` notification.
pub type DiagnosticsHandler = Arc<dyn Fn(PublishDiagnosticsParams) + Send + Sync>;

/// Incoming `$/progress` notification.
pub type ProgressHandler = Arc<dyn Fn(ProgressParams) + Send + Sync>;

pub(super) struct Pending {
    pub(super) tx: Sender<LspResult<Value>>,
}

pub(super) struct Shared {
    pub(super) stdin: Mutex<ChildStdin>,
    pub(super) pending: Mutex<HashMap<u64, Pending>>,
    pub(super) next_id: AtomicU64,
    pub(super) alive: AtomicBool,
    pub(super) diagnostics: Mutex<Option<DiagnosticsHandler>>,
    pub(super) progress: Mutex<Option<ProgressHandler>>,
    /// Full LSP settings object (e.g. `{ "rust-analyzer": { … } }`) answered
    /// to `workspace/configuration` pulls and pushed via
    /// `workspace/didChangeConfiguration`.
    pub(super) config: Mutex<Value>,
}

impl Shared {
    pub(super) fn new(stdin: ChildStdin) -> Arc<Self> {
        Arc::new(Self {
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            diagnostics: Mutex::new(None),
            progress: Mutex::new(None),
            config: Mutex::new(Value::Null),
        })
    }

    /// Route one framed JSON-RPC body to a waiting caller or a notification handler.
    pub(super) fn dispatch_message(&self, body: &str) {
        if std::env::var_os("HALO_LSP_DEBUG").is_some() {
            eprintln!(
                "[lsp<-server] {}",
                body.chars().take(600).collect::<String>()
            );
        }
        let Ok(value) = serde_json::from_str::<Value>(body) else {
            return;
        };

        if let Some(id) = value.get("id").and_then(json_id) {
            if value.get("method").is_some() {
                // Server → client request. `workspace/configuration` is answered
                // from the stored settings so servers that pull config (rust-
                // analyzer) see the values the host pushed; everything else
                // gets a benign empty reply.
                let result = if value.get("method").and_then(|m| m.as_str())
                    == Some("workspace/configuration")
                {
                    let section = value
                        .pointer("/params/items")
                        .and_then(|v| v.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .map(|item| {
                                    item.get("section")
                                        .and_then(|s| s.as_str())
                                        .and_then(|name| {
                                            let cfg = self.config.lock();
                                            if let Some(exact) = cfg.get(name) {
                                                return Some(exact.clone());
                                            }
                                            let mut parts = name.split('.');
                                            let root = parts.next()?;
                                            let mut cur = cfg.get(root)?;
                                            for part in parts {
                                                cur = cur.get(part)?;
                                            }
                                            Some(cur.clone())
                                        })
                                        .unwrap_or(Value::Null)
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    json!(section)
                } else {
                    json!(null)
                };
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result,
                });
                let mut stdin = self.stdin.lock();
                let _ = write_frame(&mut *stdin, &reply.to_string());
                return;
            }

            let result = if let Some(err) = value.get("error") {
                Err(LspError::Server {
                    code: err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1),
                    message: err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                })
            } else {
                Ok(value.get("result").cloned().unwrap_or(Value::Null))
            };

            if let Some(pending) = self.pending.lock().remove(&id) {
                let _ = pending.tx.send(result);
            }
            return;
        }

        if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
            if method == PublishDiagnostics::METHOD {
                let params_value = value.get("params").cloned().unwrap_or(Value::Null);
                if let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(params_value)
                {
                    if let Some(handler) = self.diagnostics.lock().as_ref() {
                        handler(params);
                    }
                }
            } else if method == Progress::METHOD {
                let params_value = value.get("params").cloned().unwrap_or(Value::Null);
                if let Ok(params) = serde_json::from_value::<ProgressParams>(params_value) {
                    if let Some(handler) = self.progress.lock().as_ref() {
                        handler(params);
                    }
                }
            }
        }
    }

    /// Wake every caller still blocked on a response, e.g. after the server exits.
    pub(super) fn fail_all(&self, err: LspError) {
        let pending = std::mem::take(&mut *self.pending.lock());
        for (_, p) in pending {
            let _ = p.tx.send(Err(LspError::message(err.to_string())));
        }
    }
}

pub(super) fn wait_response(
    rx: &Receiver<LspResult<Value>>,
    timeout: Duration,
) -> LspResult<Value> {
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(LspError::Timeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(LspError::NotRunning),
    }
}

/// Servers disagree on id representation; accept number or numeric string.
fn json_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|i| i as u64))
        .or_else(|| value.as_str()?.parse().ok())
}

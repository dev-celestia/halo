//! Sync newline-delimited JSON-RPC 2.0 over a child process stdio (ACP transport).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

#[derive(Debug)]
pub enum Incoming {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Eof,
}

type Pending = Arc<Mutex<HashMap<i64, SyncSender<Result<Value, String>>>>>;

#[derive(Clone)]
pub struct RpcClient {
    next_id: Arc<AtomicI64>,
    pending: Pending,
    writer: Sender<String>,
    closed: Arc<AtomicBool>,
}

impl RpcClient {
    pub fn start(stdin: ChildStdin, stdout: ChildStdout) -> (Self, Receiver<Incoming>) {
        let (writer_tx, writer_rx) = mpsc::channel::<String>();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let (incoming_tx, incoming_rx) = mpsc::sync_channel(256);

        thread::Builder::new()
            .name("agy-acp-write".into())
            .spawn(move || write_loop(stdin, writer_rx))
            .ok();

        let pending_r = Arc::clone(&pending);
        let closed_r = Arc::clone(&closed);
        thread::Builder::new()
            .name("agy-acp-read".into())
            .spawn(move || read_loop(stdout, pending_r, incoming_tx, closed_r))
            .ok();

        (
            Self {
                next_id: Arc::new(AtomicI64::new(0)),
                pending,
                writer: writer_tx,
                closed,
            },
            incoming_rx,
        )
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request_timeout(method, params, Duration::from_secs(120))
    }

    pub fn request_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        if self.is_closed() {
            return Err("ACP server connection closed".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending.lock().unwrap().insert(id, tx);
        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.writer
            .send(format!("{frame}\n"))
            .map_err(|_| "ACP write channel closed".to_string())?;
        match rx.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(err),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("ACP {method} timed out"))
            }
        }
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let frame = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.writer
            .send(format!("{frame}\n"))
            .map_err(|_| "ACP write channel closed".to_string())
    }

    pub fn respond(&self, id: &Value, result: Value) -> Result<(), String> {
        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        self.writer
            .send(format!("{frame}\n"))
            .map_err(|_| "ACP write channel closed".to_string())
    }
}

/// ACP puts the actionable text on `error.data.message` and a short label on
/// `error.message` (`Internal error`, `Authentication required`).
fn rpc_error_message(err: &Value) -> String {
    let message = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("ACP error");
    let detail = err.get("data").and_then(error_detail);
    match detail {
        Some(detail) if !detail.is_empty() && detail != message && !message.contains(detail) => {
            format!("{message}: {detail}")
        }
        _ => message.to_string(),
    }
}

fn error_detail(data: &Value) -> Option<&str> {
    data.get("message")
        .and_then(Value::as_str)
        .or_else(|| data.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn write_loop(mut stdin: ChildStdin, rx: Receiver<String>) {
    while let Ok(line) = rx.recv() {
        if stdin.write_all(line.as_bytes()).is_err() {
            break;
        }
        let _ = stdin.flush();
    }
}

fn read_loop(
    stdout: ChildStdout,
    pending: Pending,
    incoming: SyncSender<Incoming>,
    closed: Arc<AtomicBool>,
) {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("method").is_some() {
            let method = message["method"].as_str().unwrap_or("").to_string();
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            if let Some(id) = message.get("id").cloned() {
                let _ = incoming.send(Incoming::Request { id, method, params });
            } else {
                let _ = incoming.send(Incoming::Notification { method, params });
            }
            continue;
        }
        if let Some(id) = message.get("id").and_then(Value::as_i64) {
            let result = if let Some(err) = message.get("error") {
                Err(rpc_error_message(err))
            } else {
                Ok(message.get("result").cloned().unwrap_or(Value::Null))
            };
            if let Some(tx) = pending.lock().unwrap().remove(&id) {
                let _ = tx.send(result);
            }
        }
    }
    closed.store(true, Ordering::Release);
    for (_, tx) in pending.lock().unwrap().drain() {
        let _ = tx.send(Err("ACP server closed".into()));
    }
    let _ = incoming.send(Incoming::Eof);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_error_keeps_the_data_message() {
        let err = json!({
            "code": -32000,
            "message": "Authentication required",
            "data": {
                "message": "settings.json could not be read: Permission denied"
            }
        });
        assert_eq!(
            rpc_error_message(&err),
            "Authentication required: settings.json could not be read: Permission denied"
        );
        assert_eq!(
            rpc_error_message(&json!({"message": "Internal error"})),
            "Internal error"
        );
    }
}

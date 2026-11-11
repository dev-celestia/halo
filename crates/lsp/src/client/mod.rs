//! Stdio language-server process + typed request helpers.
//!
//! Split by responsibility: [`error`] for the failure type, [`transport`] for the
//! reader-thread dispatch loop, [`initialize`] for the handshake, [`documents`]
//! and [`requests`] for the LSP method wrappers, [`uri`]/[`normalize`] for the
//! conversion helpers.

mod documents;
mod error;
mod initialize;
mod normalize;
mod requests;
mod transport;
mod uri;

pub use error::{LspError, LspResult};
pub use normalize::apply_text_edits;
pub use transport::{DiagnosticsHandler, ProgressHandler};
pub use uri::{path_to_uri, uri_to_path};

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lsp_types::Uri;
use lsp_types::notification::{Exit, Notification as _};
use lsp_types::request::{Request as _, Shutdown};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::stdio::{read_frame, write_frame};

use transport::{Pending, Shared, wait_response};

/// How to spawn a language server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageServerSpec {
    /// Catalog id (`rust-analyzer`, `typescript-language-server`, …).
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub language_ids: Vec<String>,
}

/// One connected language-server process.
pub struct LanguageClient {
    shared: Arc<Shared>,
    child: Mutex<Child>,
    reader: Mutex<Option<JoinHandle<()>>>,
    root_uri: Uri,
    root_path: PathBuf,
}

impl LanguageClient {
    /// Spawn `spec.command` with `cwd = root_path` and run LSP initialize.
    pub fn start(spec: LanguageServerSpec, root_path: &Path) -> LspResult<Self> {
        let root_path = root_path
            .canonicalize()
            .unwrap_or_else(|_| root_path.to_path_buf());
        let root_uri = path_to_uri(&root_path)?;

        let command = crate::which(&spec.command).unwrap_or_else(|| spec.command.clone());
        let mut child = Command::new(&command)
            .args(&spec.args)
            .current_dir(&root_path)
            // Bundled wrappers + ShellCheck live beside the app; GUI launches
            // do not inherit the user's shell PATH.
            .env("PATH", crate::path_with_bundled_bins())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                LspError::message(format!(
                    "failed to spawn `{}`: {e} (is it on PATH?)",
                    spec.command
                ))
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::message("language server stdin missing"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::message("language server stdout missing"))?;

        let shared = Shared::new(stdin);

        let reader_shared = Arc::clone(&shared);
        let reader = thread::Builder::new()
            .name("halo-lsp-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                while reader_shared.alive.load(Ordering::SeqCst) {
                    match read_frame(&mut reader) {
                        Ok(body) => reader_shared.dispatch_message(&body),
                        Err(_) => {
                            reader_shared.alive.store(false, Ordering::SeqCst);
                            reader_shared.fail_all(LspError::message("language server closed"));
                            break;
                        }
                    }
                }
            })
            .map_err(|e| LspError::message(format!("failed to spawn lsp reader: {e}")))?;

        let client = Self {
            shared,
            child: Mutex::new(child),
            reader: Mutex::new(Some(reader)),
            root_uri: root_uri.clone(),
            root_path,
        };

        client.initialize(&spec.id)?;
        Ok(client)
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn root_uri(&self) -> &Uri {
        &self.root_uri
    }

    pub fn is_alive(&self) -> bool {
        self.shared.alive.load(Ordering::SeqCst)
    }

    /// Register a handler for `textDocument/publishDiagnostics`.
    pub fn on_diagnostics(&self, handler: DiagnosticsHandler) {
        *self.shared.diagnostics.lock() = Some(handler);
    }

    /// Register a handler for `$/progress` work-done notifications.
    pub fn on_progress(&self, handler: ProgressHandler) {
        *self.shared.progress.lock() = Some(handler);
    }

    /// Ask the server to shut down, then kill and reap the process.
    pub fn shutdown_and_exit(&self) -> LspResult<()> {
        if !self.is_alive() {
            return Ok(());
        }
        let _ = self.request::<Value>(Shutdown::METHOD, Value::Null);
        let _ = self.notify(Exit::METHOD, Value::Null);
        self.shared.alive.store(false, Ordering::SeqCst);
        {
            let mut child = self.child.lock();
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.reader.lock().take() {
            let _ = handle.join();
        }
        Ok(())
    }

    /// Send a request and block until the reader thread resolves it (30s timeout).
    fn request<R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: impl serde::Serialize,
    ) -> LspResult<R> {
        let id = self.shared.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.shared.pending.lock().insert(id, Pending { tx });

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_raw(&msg.to_string())?;

        let value = wait_response(&rx, Duration::from_secs(30))?;
        Ok(serde_json::from_value(value)?)
    }

    fn notify(&self, method: &str, params: impl serde::Serialize) -> LspResult<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_raw(&msg.to_string())
    }

    fn write_raw(&self, body: &str) -> LspResult<()> {
        if !self.is_alive() {
            return Err(LspError::NotRunning);
        }
        let mut stdin = self.shared.stdin.lock();
        write_frame(&mut *stdin, body)?;
        Ok(())
    }
}

impl Drop for LanguageClient {
    fn drop(&mut self) {
        let _ = self.shutdown_and_exit();
    }
}

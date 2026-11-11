//! `textDocument/*` document-synchronisation notifications.

use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as _,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
};

use super::error::LspResult;
use serde_json::Value;

use super::LanguageClient;

impl LanguageClient {
    pub fn did_open(
        &self,
        uri: Uri,
        language_id: &str,
        version: i32,
        text: String,
    ) -> LspResult<()> {
        self.notify(
            DidOpenTextDocument::METHOD,
            DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id: language_id.to_string(),
                    version,
                    text,
                },
            },
        )
    }

    pub fn did_change_full(&self, uri: Uri, version: i32, text: String) -> LspResult<()> {
        self.notify(
            DidChangeTextDocument::METHOD,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri, version },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text,
                }],
            },
        )
    }

    pub fn did_close(&self, uri: Uri) -> LspResult<()> {
        self.notify(
            DidCloseTextDocument::METHOD,
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
            },
        )
    }

    pub fn did_save(&self, uri: Uri, text: Option<String>) -> LspResult<()> {
        self.notify(
            DidSaveTextDocument::METHOD,
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
                text,
            },
        )
    }
}

impl LanguageClient {
    /// Push the full LSP settings object and notify the server. Servers with
    /// the `workspace.configuration` pull re-read it via
    /// `workspace/configuration`, which the transport answers from the same
    /// stored value.
    pub fn set_config(&self, settings: Value) -> LspResult<()> {
        *self.shared.config.lock() = settings.clone();
        self.notify(
            "workspace/didChangeConfiguration",
            serde_json::json!({ "settings": settings }),
        )
    }

    /// Link `paths` as a standalone inline rust-project so rust-analyzer
    /// analyzes them even when the workspace has no discoverable Cargo
    /// project (or the file sits outside every member crate). Each file
    /// becomes its own one-file crate with the real toolchain sysroot.
    pub fn set_standalone_project(&self, paths: Vec<std::path::PathBuf>) -> LspResult<()> {
        let sysroot = std::process::Command::new("rustc")
            .arg("--print")
            .arg("sysroot")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        let Some(sysroot) = sysroot else {
            return Ok(());
        };
        let sysroot_src = format!("{sysroot}/lib/rustlib/src/rust/library");

        let crates: Vec<serde_json::Value> = paths
            .iter()
            .filter_map(|p| {
                let stem = p.file_stem()?.to_string_lossy().into_owned();
                let name: String = stem
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let name = if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    format!("_{name}")
                } else {
                    name
                };
                Some(serde_json::json!({
                    "crate_name": name,
                    "root_module": p.display().to_string(),
                    "edition": "2021",
                    "deps": [],
                    "is_workspace_member": true,
                }))
            })
            .collect();

        let inline_project = serde_json::json!({
            "sysroot": sysroot,
            "sysroot_src": sysroot_src,
            "crates": crates,
        });
        let ra_settings =
            super::initialize::rust_analyzer_settings(&self.root_path, Some(inline_project));

        self.set_config(serde_json::json!({
            "rust-analyzer": ra_settings
        }))
    }
}

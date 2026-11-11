//! The LSP `initialize` / `initialized` handshake and the capabilities we advertise.

use std::path::Path;

use lsp_types::notification::{Initialized, Notification as _};
use lsp_types::request::{Initialize, Request as _};
use lsp_types::*;
use serde_json::{Value, json};

use super::LanguageClient;
use super::error::LspResult;

impl LanguageClient {
    /// Send `initialize`, then `initialized`. Called once from [`LanguageClient::start`].
    #[allow(deprecated)]
    pub(super) fn initialize(&self, server_id: &str) -> LspResult<InitializeResult> {
        let initialization_options = if server_id == "rust-analyzer" {
            let settings = rust_analyzer_settings(&self.root_path, None);
            *self.shared.config.lock() = json!({
                "rust-analyzer": settings.clone()
            });
            Some(settings)
        } else if server_id == "bash-language-server" {
            // bash-language-server pulls its settings through
            // `workspace/configuration` (`bashIde`) and only runs ShellCheck
            // when it can find the binary. Point it at the bundled copy so a
            // GUI launch without Homebrew still gets lint diagnostics.
            let mut settings = json!({
                "globPattern": "**/*@(.sh|.inc|.bash|.command)"
            });
            if let Some(shellcheck) = crate::which("shellcheck") {
                settings["shellcheckPath"] = json!(shellcheck);
            }
            *self.shared.config.lock() = json!({ "bashIde": settings });
            None
        } else {
            None
        };

        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(self.root_uri.clone()),
            root_path: Some(self.root_path.display().to_string()),
            initialization_options,
            capabilities: client_capabilities(),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: self.root_uri.clone(),
                name: self
                    .root_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "workspace".into()),
            }]),
            ..Default::default()
        };

        let result: InitializeResult = self.request(Initialize::METHOD, params)?;
        self.notify(Initialized::METHOD, InitializedParams {})?;
        Ok(result)
    }
}

/// Build the `rust-analyzer` settings payload.
///
/// When `<root>/Cargo.toml` exists, pin `linkedProjects` to it (plus any
/// standalone inline project) so `rust-analyzer` does not crawl `references/`
/// or treat `vendor/` crates as competing independent workspaces. Also disable
/// upfront full-graph `cachePriming` and workspace-wide `cargo check` so editor
/// diagnostics arrive in seconds instead of blocking for minutes on cold start.
pub(super) fn rust_analyzer_settings(root_path: &Path, standalone_project: Option<Value>) -> Value {
    let mut linked_projects = Vec::new();
    let root_manifest = root_path.join("Cargo.toml");
    if root_manifest.is_file() {
        linked_projects.push(json!(root_manifest.display().to_string()));
    }
    if let Some(inline_project) = standalone_project {
        linked_projects.push(inline_project);
    }

    let mut settings = json!({
        "cachePriming": { "enable": false },
        "cargo": {
            "buildScripts": { "enable": false },
            "targetDir": true
        },
        "check": { "workspace": false },
        "diagnostics": {
            "enable": true,
            "experimental": { "enable": true }
        },
        "files": {
            "excludeDirs": [".git", "target", "node_modules", "references"]
        }
    });

    if !linked_projects.is_empty() {
        if let Some(obj) = settings.as_object_mut() {
            obj.insert("linkedProjects".to_string(), json!(linked_projects));
        }
    }
    settings
}

fn client_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        workspace: Some(WorkspaceClientCapabilities {
            workspace_folders: Some(true),
            configuration: Some(true),
            ..Default::default()
        }),
        text_document: Some(TextDocumentClientCapabilities {
            synchronization: Some(TextDocumentSyncClientCapabilities {
                did_save: Some(true),
                ..Default::default()
            }),
            completion: Some(CompletionClientCapabilities {
                completion_item: Some(CompletionItemCapability {
                    // The editor inserts `new_text` verbatim and cannot move
                    // the caret into a placeholder afterwards, so a snippet
                    // would land as literal `${1:name}`. Ask for plain text;
                    // the completion adapter also strips placeholders from
                    // servers that send them anyway.
                    snippet_support: Some(false),
                    documentation_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            hover: Some(HoverClientCapabilities {
                content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                ..Default::default()
            }),
            definition: Some(GotoCapability {
                link_support: Some(true),
                ..Default::default()
            }),
            code_action: Some(CodeActionClientCapabilities {
                code_action_literal_support: Some(CodeActionLiteralSupport {
                    code_action_kind: CodeActionKindLiteralSupport {
                        value_set: vec![
                            CodeActionKind::QUICKFIX.as_str().to_string(),
                            CodeActionKind::REFACTOR.as_str().to_string(),
                            CodeActionKind::SOURCE.as_str().to_string(),
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS.as_str().to_string(),
                        ],
                    },
                }),
                resolve_support: None,
                ..Default::default()
            }),
            formatting: Some(DocumentFormattingClientCapabilities {
                dynamic_registration: Some(false),
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
            ..Default::default()
        }),
        window: Some(WindowClientCapabilities {
            work_done_progress: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_analyzer_settings_pins_root_manifest_and_disables_cache_priming() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = dir.path().join("Cargo.toml");
        std::fs::write(&manifest, "[workspace]\n").unwrap();

        let settings = rust_analyzer_settings(dir.path(), None);
        assert_eq!(settings["cachePriming"]["enable"], json!(false));
        assert_eq!(settings["check"]["workspace"], json!(false));
        assert_eq!(
            settings["linkedProjects"],
            json!([manifest.display().to_string()])
        );
    }
}

//! Coder LSP session: start/stop language servers, doc sync, gpui providers.

use std::collections::{HashMap, HashSet};
use std::ops::Range as OpsRange;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use crate::work::CoderLspStatus;
use anyhow::{Result as AnyResult, anyhow};
use celestia_ui::components::CodeEditor;
use gpui::{App, AsyncApp, Entity, Global, SharedString, Task, Window};
use gpui_component::input::{
    CodeActionProvider, CompletionProvider, DefinitionProvider, EditorState, HoverProvider,
    InputEvent, Rope, RopeExt, ShowDocumentHandler,
};
use halo_lsp::{
    LanguageClient, LspError, is_command_available, is_registered, path_to_uri,
    server_for_language, server_id_for_language, uri_to_path,
};
use lsp_types::{
    CodeAction, CodeActionContext, CompletionContext, CompletionResponse, DiagnosticSeverity,
    Hover, LocationLink, NumberOrString, ProgressParams, ProgressParamsValue,
    PublishDiagnosticsParams, Range, ShowDocumentParams, TextDocumentEdit, TextEdit, Uri,
    WorkDoneProgress, WorkspaceEdit,
};

use super::*;

mod bridge;
mod diagnostics;
mod providers;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use diagnostics::*;
pub(crate) use providers::*;
pub(crate) use state::*;

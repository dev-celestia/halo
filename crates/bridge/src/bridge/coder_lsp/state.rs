//! LSP runtime state: the global [`CoderLspRuntime`], its accessors, and path helpers.

use super::*;

pub(crate) struct OpenDoc {
    pub(crate) language_id: String,
    pub(crate) server_id: String,
    pub(crate) version: AtomicI32,
}

#[derive(Clone, Debug)]
pub(crate) struct LspProgressTask {
    pub(crate) title: String,
    pub(crate) message: Option<String>,
    pub(crate) percentage: Option<u32>,
    pub(crate) seq: u64,
}

impl LspProgressTask {
    pub(crate) fn summary(&self) -> String {
        let title = self.title.trim();
        let title = if title.is_empty() { "Working" } else { title };
        if let Some(pct) = self.percentage {
            return format!("{title} {pct}%");
        }
        if let Some(msg) = &self.message {
            let head = msg.split(':').next().unwrap_or("").trim();
            if !head.is_empty()
                && head.contains('/')
                && head.chars().all(|c| c.is_ascii_digit() || c == '/')
            {
                return format!("{title} {head}");
            }
        }
        format!("{title}…")
    }
}

pub(crate) struct CoderLspRuntime {
    /// Catalog server id → live client.
    pub(crate) clients: HashMap<String, Arc<LanguageClient>>,
    /// Server ids currently spawning.
    pub(crate) starting: HashSet<String>,
    /// Active `$/progress` tasks keyed by `(server_id, token)`.
    pub(crate) progress: HashMap<(String, String), LspProgressTask>,
    pub(crate) progress_seq: u64,
    /// Absolute path → open document metadata.
    pub(crate) docs: HashMap<String, OpenDoc>,
    /// Latest diagnostics by canonical path.
    pub(crate) diagnostics: HashMap<String, Vec<lsp_types::Diagnostic>>,
    /// Instant tree-sitter syntax-linter diagnostics by canonical path.
    pub(crate) lint_diagnostics: HashMap<String, Vec<lsp_types::Diagnostic>>,
    /// Path currently bound to each editor pane.
    pub(crate) bound_path: [Option<String>; crate::work::CODER_MAX_PANES],
    /// EditorState handles for applying diagnostics / providers.
    pub(crate) editors: [Option<Entity<EditorState>>; crate::work::CODER_MAX_PANES],
    /// CodeEditor entities used by `show_document` to open other files.
    pub(crate) code_editors: [Option<Entity<CodeEditor>>; crate::work::CODER_MAX_PANES],
    /// `.rs` files linked to rust-analyzer as a standalone inline project —
    /// files that live outside every Cargo package it discovered. Without
    /// this the server loads no workspace at all and publishes zero
    /// diagnostics for them.
    pub(crate) standalone_files: HashMap<String, Vec<PathBuf>>,
    pub(crate) change_seq: u64,
}

impl Default for CoderLspRuntime {
    fn default() -> Self {
        Self {
            clients: HashMap::new(),
            starting: HashSet::new(),
            progress: HashMap::new(),
            progress_seq: 0,
            docs: HashMap::new(),
            diagnostics: HashMap::new(),
            lint_diagnostics: HashMap::new(),
            bound_path: std::array::from_fn(|_| None),
            editors: std::array::from_fn(|_| None),
            code_editors: std::array::from_fn(|_| None),
            standalone_files: HashMap::new(),
            change_seq: 0,
        }
    }
}

impl Global for CoderLspRuntime {}

pub(crate) fn runtime(cx: &mut App) -> &mut CoderLspRuntime {
    if !cx.has_global::<CoderLspRuntime>() {
        cx.set_global(CoderLspRuntime::default());
    }
    cx.global_mut::<CoderLspRuntime>()
}

pub(crate) fn runtime_ref(cx: &App) -> Option<&CoderLspRuntime> {
    cx.try_global::<CoderLspRuntime>()
}

/// True when `path` is not covered by any Cargo package rust-analyzer would
/// discover from the workspace root: no `[package]` manifest between the
/// file's directory and the filesystem root. Workspace-root files (a virtual
/// `[workspace]` manifest without members above them) and bare folders both
/// land here, which is exactly when rust-analyzer loads no package for them.
pub(crate) fn path_is_outside_cargo_packages(path: &Path) -> bool {
    for dir in path.ancestors().skip(1) {
        let manifest = dir.join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        // A `[package]` manifest claims the files beneath it; a virtual
        // workspace manifest does not cover files sitting next to it.
        let in_package = text
            .split("\n")
            .take(64)
            .any(|line| line.trim_start().starts_with("[package]"));
        if in_package {
            return false;
        }
    }
    true
}

pub(crate) fn pane_idx(group: u8) -> usize {
    (group as usize).min(crate::work::CODER_MAX_PANES.saturating_sub(1))
}

pub(crate) fn canonical_path_str(path: &str) -> String {
    Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .display()
        .to_string()
}

pub(crate) fn paths_match(a: &str, b: &str) -> bool {
    a == b || canonical_path_str(a) == canonical_path_str(b)
}

pub(crate) fn progress_token_key(token: &NumberOrString) -> String {
    match token {
        NumberOrString::Number(n) => n.to_string(),
        NumberOrString::String(s) => s.clone(),
    }
}

pub(crate) fn client_for_path(rt: &CoderLspRuntime, path: &str) -> Option<Arc<LanguageClient>> {
    let doc = rt.docs.get(path).or_else(|| {
        let canon = canonical_path_str(path);
        rt.docs
            .iter()
            .find(|(k, _)| canonical_path_str(k) == canon)
            .map(|(_, v)| v)
    })?;
    let client = rt.clients.get(&doc.server_id)?;
    if client.is_alive() {
        Some(client.clone())
    } else {
        None
    }
}

pub(crate) fn any_client_alive(rt: &CoderLspRuntime) -> bool {
    rt.clients.values().any(|c| c.is_alive())
}

pub(crate) fn active_progress_label(rt: &CoderLspRuntime) -> Option<String> {
    rt.progress
        .values()
        .max_by_key(|t| t.seq)
        .map(LspProgressTask::summary)
}

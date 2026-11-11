//! Editor provider adapters: completion, hover, definition, and code actions.

use super::*;

pub(crate) struct CoderLspAdapter {
    pub(crate) group: u8,
}

impl CoderLspAdapter {
    fn path_and_client(&self, cx: &App) -> Option<(String, Arc<LanguageClient>)> {
        let rt = runtime_ref(cx)?;
        let path = rt.bound_path[pane_idx(self.group)].clone()?;
        let client = client_for_path(rt, &path)?;
        Some((path, client))
    }

    fn uri_for_path(path: &str) -> AnyResult<Uri> {
        path_to_uri(Path::new(path)).map_err(|e| anyhow!("{e}"))
    }
}

impl CompletionProvider for CoderLspAdapter {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        trigger: CompletionContext,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyResult<CompletionResponse>> {
        let Some((path, client)) = self.path_and_client(cx) else {
            return Task::ready(Ok(CompletionResponse::Array(vec![])));
        };
        let Ok(uri) = Self::uri_for_path(&path) else {
            return Task::ready(Ok(CompletionResponse::Array(vec![])));
        };
        let position = text.offset_to_position(offset);
        cx.background_spawn(async move {
            client
                .completion(uri, position, Some(trigger))
                .map_err(|e| anyhow!("{e}"))
        })
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, cx: &mut App) -> bool {
        if self.path_and_client(cx).is_none() {
            return false;
        }
        if new_text.is_empty() {
            return false;
        }
        new_text
            .chars()
            .any(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
    }
}

impl HoverProvider for CoderLspAdapter {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyResult<Option<Hover>>> {
        let Some((path, client)) = self.path_and_client(cx) else {
            return Task::ready(Ok(None));
        };
        let Ok(uri) = Self::uri_for_path(&path) else {
            return Task::ready(Ok(None));
        };
        let position = text.offset_to_position(offset);
        cx.background_spawn(async move {
            let hover = client.hover(uri, position).map_err(|e| anyhow!("{e}"))?;
            Ok(hover.map(polish_hover))
        })
    }
}

/// Soften hover markdown for the editor popover: keep fenced code (for highlight)
/// but drop thematic breaks so sections group with space instead of nested rules.
pub(crate) fn polish_hover(mut hover: Hover) -> Hover {
    hover.contents = match hover.contents {
        lsp_types::HoverContents::Scalar(s) => {
            lsp_types::HoverContents::Scalar(polish_marked_string(s))
        }
        lsp_types::HoverContents::Array(items) => {
            lsp_types::HoverContents::Array(items.into_iter().map(polish_marked_string).collect())
        }
        lsp_types::HoverContents::Markup(mut markup) => {
            markup.value = polish_hover_markdown(&markup.value);
            lsp_types::HoverContents::Markup(markup)
        }
    };
    hover
}

pub(crate) fn polish_marked_string(item: lsp_types::MarkedString) -> lsp_types::MarkedString {
    match item {
        lsp_types::MarkedString::String(s) => {
            lsp_types::MarkedString::String(polish_hover_markdown(&s))
        }
        other => other,
    }
}

pub(crate) fn polish_hover_markdown(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for line in md.lines() {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    // Collapse runs of 3+ blank lines left by stripped rules.
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

#[cfg(test)]
mod hover_polish_tests {
    use super::polish_hover_markdown;

    #[test]
    fn strips_thematic_breaks_and_keeps_fences() {
        let md = "```rust\nfoo::Bar\n```\n\n---\n\nDocs here.\n";
        let polished = polish_hover_markdown(md);
        assert!(polished.contains("```rust"));
        assert!(polished.contains("Docs here."));
        assert!(!polished.contains("---"));
        assert!(!polished.contains("\n\n\n"));
    }
}

impl DefinitionProvider for CoderLspAdapter {
    fn definitions(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyResult<Vec<LocationLink>>> {
        let Some((path, client)) = self.path_and_client(cx) else {
            return Task::ready(Ok(vec![]));
        };
        let Ok(uri) = Self::uri_for_path(&path) else {
            return Task::ready(Ok(vec![]));
        };
        let position = text.offset_to_position(offset);
        cx.background_spawn(async move {
            client
                .goto_definition(uri, position)
                .map_err(|e| anyhow!("{e}"))
        })
    }
}

impl CodeActionProvider for CoderLspAdapter {
    fn id(&self) -> SharedString {
        "halo-lsp".into()
    }

    fn code_actions(
        &self,
        state: Entity<EditorState>,
        range: OpsRange<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyResult<Vec<CodeAction>>> {
        let Some((path, client)) = self.path_and_client(cx) else {
            return Task::ready(Ok(vec![]));
        };
        let Ok(uri) = Self::uri_for_path(&path) else {
            return Task::ready(Ok(vec![]));
        };
        let (start, end) = {
            let state = state.read(cx);
            let text = state.text();
            (
                text.offset_to_position(range.start),
                text.offset_to_position(range.end),
            )
        };
        let lsp_range = Range { start, end };
        let canon_path = canonical_path_str(&path);
        let context = CodeActionContext {
            diagnostics: runtime_ref(cx)
                .and_then(|rt| {
                    rt.diagnostics
                        .get(&canon_path)
                        .or_else(|| rt.diagnostics.get(&path))
                        .cloned()
                })
                .unwrap_or_default(),
            only: None,
            trigger_kind: None,
        };
        cx.background_spawn(async move {
            client
                .code_actions(uri, lsp_range, context)
                .map_err(|e| anyhow!("{e}"))
        })
    }

    fn perform_code_action(
        &self,
        state: Entity<EditorState>,
        action: CodeAction,
        _push_to_history: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyResult<()>> {
        let Some(edit) = action.edit.clone() else {
            return Task::ready(Ok(()));
        };
        Bridge::coder_lsp_apply_workspace_edit(&edit, &state, window, cx);
        Task::ready(Ok(()))
    }
}

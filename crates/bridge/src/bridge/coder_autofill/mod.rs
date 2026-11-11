//! Server-free completion for the Coder editor.
//!
//! The engine's completion pipeline is complete — provider trait, popover,
//! arrow/enter/escape routing — but until now the only provider installed on a
//! Coder pane was `CoderLspAdapter`, whose trigger refuses to fire without a
//! live language server. JSON, YAML, TOML, Markdown, HTTP and plain-text
//! buffers therefore got nothing at all, and neither did any file whose server
//! was still indexing or missing from PATH.
//!
//! [`Autofill`] is the provider the panes actually install. It wraps the server
//! provider when there is one and always adds a local source:
//!
//! * **words** already in the buffer, ranked by match strength then frequency,
//! * **keywords** for the buffer's language.
//!
//! Both are filtered against the identifier under the caret, so the menu opens
//! with the same shape whether or not a server is answering. Local items are
//! ranked below the server's and an item the server already returned is never
//! duplicated.
//!
//! It also repairs the server's answer before the menu sees it — see
//! [`normalize`] for the two shapes that used to insert the wrong text.
//!
//! **Not implemented:** snippet placeholders. The engine's inserter
//! (`EditorState::insert_completion`) writes `new_text` verbatim and offers no
//! hook to move the caret afterwards, so a `fn $1()` expansion would leave the
//! caret after the closing brace. Multi-line templates are deliberately absent
//! rather than offered and then dropped in the wrong place.

mod keywords;
mod normalize;
mod rank;
mod words;

use std::rc::Rc;

use celestia_ui::components::CodeEditor;
// `AppContext` brings `background_spawn`; `App::spawn` is inherent.
use gpui::{App, AppContext as _, AsyncApp, Entity, Task, WeakEntity, Window};
use gpui_component::input::{CompletionProvider, Rope};
use lsp_types::{CompletionContext, CompletionResponse, CompletionTriggerKind};

/// Items the local source contributes when it is the only source.
const LOCAL_ONLY_LIMIT: usize = 60;

/// Items the local source adds after a server answer.
///
/// A server list is already ranked and typed; burying it under a wall of
/// document words is worse than a short tail of extras.
const LOCAL_FILL_LIMIT: usize = 20;

/// The completion provider installed on a Coder editor.
pub(crate) struct Autofill {
    /// The editor, for its current language. Weak: the provider is owned by
    /// that editor's own `EditorState`, so a strong handle would be a cycle.
    editor: WeakEntity<CodeEditor>,
    /// The language server adapter, when this pane has one.
    server: Option<Rc<dyn CompletionProvider>>,
}

impl Autofill {
    pub(crate) fn new(
        editor: WeakEntity<CodeEditor>,
        server: Option<Rc<dyn CompletionProvider>>,
    ) -> Self {
        Self { editor, server }
    }

    /// Highlighter language id for the buffer, or `plaintext` once the editor
    /// is gone (which only happens while its own completion is in flight).
    fn language(&self, cx: &App) -> String {
        self.editor
            .upgrade()
            .map(|editor| editor.read(cx).language().to_string())
            .unwrap_or_else(|| "plaintext".to_string())
    }
}

impl CompletionProvider for Autofill {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        trigger: CompletionContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let language = self.language(cx);
        let manual = trigger.trigger_kind == CompletionTriggerKind::INVOKED;

        // The server call has to happen here, on the UI thread, because the
        // provider needs `Window`. It only dispatches: the adapter hands back a
        // task that is already running on a background thread.
        let server = self
            .server
            .as_ref()
            .map(|provider| provider.completions(text, offset, trigger, window, cx));

        // Walking the buffer is cheap but not free, and it runs on every
        // keystroke, so it goes to a background thread. `Rope` is a persistent
        // tree, so the clone below shares its nodes.
        let scan_doc = text.clone();
        let scan = cx
            .background_spawn(async move { words::complete(&scan_doc, offset, &language, manual) });

        let repair_doc = text.clone();
        cx.spawn(async move |_cx: &mut AsyncApp| {
            let mut items = match server {
                Some(task) => match task.await {
                    Ok(CompletionResponse::Array(items)) => items,
                    Ok(CompletionResponse::List(list)) => list.items,
                    // A dead server is not an error the user can act on: fall
                    // through to the local source instead of blanking the menu.
                    Err(_) => Vec::new(),
                },
                None => Vec::new(),
            };
            let server_answered = !items.is_empty();
            normalize::repair(&mut items, &repair_doc, offset);

            let local = scan.await;
            let limit = if server_answered {
                LOCAL_FILL_LIMIT
            } else {
                LOCAL_ONLY_LIMIT
            };
            Ok(CompletionResponse::Array(normalize::merge(
                items, local, limit,
            )))
        })
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _cx: &mut App) -> bool {
        is_trigger_text(new_text)
    }
}

/// Ask on every keystroke that can extend an identifier or open a member /
/// path access.
///
/// This is the trigger set the LSP adapter used, so nothing that used to fire
/// stops firing; the difference is that it no longer depends on a server being
/// alive. Anything else — space, punctuation, newline — answers nothing, which
/// is how the menu closes without a separate dismissal path.
pub(crate) fn is_trigger_text(new_text: &str) -> bool {
    !new_text.is_empty()
        && new_text
            .chars()
            .any(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
}

/// Install [`Autofill`] on `editor`, wrapping `server` when the pane has one.
///
/// Idempotent: the Coder calls it again on every tab switch and every time a
/// server becomes ready.
pub(crate) fn install(
    editor: &Entity<CodeEditor>,
    server: Option<Rc<dyn CompletionProvider>>,
    cx: &mut App,
) {
    let provider = Rc::new(Autofill::new(editor.downgrade(), server));
    let state = editor.read(cx).editor_state();
    state.update(cx, |state, _| {
        state.lsp_mut().completion_provider = Some(provider);
    });
}

#[cfg(test)]
mod tests {
    use super::is_trigger_text;

    #[test]
    fn trigger_accepts_identifier_and_member_keystrokes() {
        for text in ["a", "_", ".", "::", "中", "9"] {
            assert!(is_trigger_text(text), "{text:?} must re-ask");
        }
    }

    #[test]
    fn trigger_ignores_whitespace_and_punctuation() {
        for text in ["", " ", "\n", "(", ",", ";", "-"] {
            assert!(!is_trigger_text(text), "{text:?} must stay silent");
        }
    }
}

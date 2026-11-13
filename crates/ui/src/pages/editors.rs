//! Shared editors for Repeater / Intruder / HTTP history detail.

use std::cell::Cell;

use celestia_ui::components::CodeEditor;
use celestia_ui::components::primitive::input::InputState;
use celestia_ui::components::primitive::select::{SelectOption, SelectState};
use celestia_ui::components::primitive::textarea::TextareaState;
use gpui::*;
use gpui_component::IndexPath;
use halo_bridge::CODER_MAX_PANES;

use crate::pages::http::HttpHistoryList;
use crate::pages::repeater::{EnvVarsForm, HeadersForm};
use crate::pages::source_control::GitChangesList;

/// HTTP methods offered by the Repeater method picker.
pub const REPEATER_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// Sentinel value for the env picker when no environment is active.
pub const REPEATER_ENV_NONE: &str = "";

/// Build a [`CodeEditor`] with the app's house style: flush, no card border,
/// no footer minimap-toggle strip.
///
/// Every host pane in this app draws its own frame (or deliberately has none),
/// so the editor must not draw a second one inside it — otherwise a pane with a
/// border reads as a box in a box. The editor also takes its height from the
/// host instead of the component's fixed 280px preview height, so code meets
/// the pane edges. Hosts that need a frame wrap the editor in
/// [`crate::pages::chrome::code_surface`].
fn code_editor(language: &str, window: &mut Window, cx: &mut App) -> Entity<CodeEditor> {
    let editor = cx.new(|cx| CodeEditor::new(language, window, cx));
    editor.update(cx, |e, cx| {
        e.set_bordered(false, cx);
        e.set_fill_height(true, cx);
        e.set_show_minimap_toggle(false, cx);
    });
    editor
}

/// Data-driven state for the Repeater method input dropdown.
pub type RepeaterMethodSelect = SelectState<Vec<SelectOption<String>>>;

/// Data-driven state for the Repeater environment input dropdown.
pub type RepeaterEnvSelect = SelectState<Vec<SelectOption<String>>>;

/// Data-driven state for the Repeater collection switcher.
pub type RepeaterCollectionSelect = SelectState<Vec<SelectOption<String>>>;

/// Menu options for the method picker, appending `extra` when a loaded request
/// uses a verb the picker does not offer (e.g. `TRACE` from imported traffic).
fn repeater_method_options(extra: Option<&str>) -> Vec<SelectOption<String>> {
    let mut methods: Vec<String> = REPEATER_METHODS.iter().map(|m| (*m).to_string()).collect();
    if let Some(extra) = extra {
        if !methods.iter().any(|m| m.eq_ignore_ascii_case(extra)) {
            methods.push(extra.to_string());
        }
    }
    methods
        .into_iter()
        .map(|method| SelectOption::new(method.clone(), method))
        .collect()
}

/// Menu options for the environment picker: "No Environment" plus each context.
pub fn repeater_env_options<'a>(
    contexts: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<SelectOption<String>> {
    let mut options = vec![SelectOption::new(
        REPEATER_ENV_NONE.to_string(),
        "No Environment",
    )];
    for (id, name) in contexts {
        options.push(SelectOption::new(id.to_string(), name.to_string()));
    }
    options
}

/// Menu options for the collection switcher.
pub fn repeater_collection_options<'a>(
    collections: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<SelectOption<String>> {
    collections
        .into_iter()
        .map(|(id, name)| SelectOption::new(id.to_string(), name.to_string()))
        .collect()
}

/// Per-pane file-tab strip scroll — keeps the active tab visible when it changes.
pub struct CoderTabScroll {
    pub handle: ScrollHandle,
    /// Last active path we scrolled to; skip while unchanged so the user can
    /// freely pan the strip after a reveal.
    pub last_active: Cell<Option<SharedString>>,
}

impl Clone for CoderTabScroll {
    fn clone(&self) -> Self {
        let current = self.last_active.take();
        self.last_active.set(current.clone());
        Self {
            handle: self.handle.clone(),
            last_active: Cell::new(current),
        }
    }
}

impl CoderTabScroll {
    pub fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
            last_active: Cell::new(None),
        }
    }

    /// Scroll the strip so `active` is in view when it differs from the last
    /// reveal. `index` is the tab's position in the strip.
    pub fn reveal_active(&self, active: Option<&str>, index: Option<usize>) {
        let next = active.map(SharedString::from);
        let prev = self.last_active.replace(next.clone());
        if prev != next {
            if let Some(ix) = index {
                self.handle.scroll_to_item(ix);
            }
        }
    }
}

/// Explorer file-tree scroll — keeps the active file row visible when it changes.
pub struct CoderExplorerScroll {
    pub handle: ScrollHandle,
    last_active: Cell<Option<SharedString>>,
    /// Row index waiting for a non-zero viewport after the first layout pass.
    pending_index: Cell<Option<usize>>,
}

impl Clone for CoderExplorerScroll {
    fn clone(&self) -> Self {
        let current = self.last_active.take();
        self.last_active.set(current.clone());
        let pending = self.pending_index.take();
        self.pending_index.set(pending);
        Self {
            handle: self.handle.clone(),
            last_active: Cell::new(current),
            pending_index: Cell::new(pending),
        }
    }
}

impl CoderExplorerScroll {
    fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
            last_active: Cell::new(None),
            pending_index: Cell::new(None),
        }
    }

    /// Scroll so the visible-row at `index` is in view when `active` changes
    /// (or when a prior reveal was deferred until layout).
    ///
    /// `row_height` is the pitch the explorer rows were drawn with, so the
    /// reveal math stays in step with the rendered tree.
    pub fn reveal_active(&self, active: Option<&str>, index: Option<usize>, row_height: Pixels) {
        let next = active.map(SharedString::from);
        let prev = self.last_active.replace(next.clone());
        let path_changed = prev != next;

        let Some(ix) = index.or_else(|| self.pending_index.get()) else {
            return;
        };
        if !path_changed && self.pending_index.get().is_none() {
            return;
        }

        let row_h = row_height;
        let item_top = row_h * ix as f32;
        let item_bottom = item_top + row_h;
        let viewport = self.handle.bounds().size.height;
        if viewport <= px(0.) {
            self.pending_index.set(Some(ix));
            self.handle.set_offset(point(px(0.), -item_top));
            return;
        }
        self.pending_index.set(None);

        let offset_y = self.handle.offset().y;
        let view_top = -offset_y;
        let view_bottom = view_top + viewport;
        if item_top < view_top {
            self.handle.set_offset(point(px(0.), -item_top));
        } else if item_bottom > view_bottom {
            let target = (item_bottom - viewport).max(px(0.));
            self.handle.set_offset(point(px(0.), -target));
        }
    }
}

#[derive(Clone)]
pub struct ToolEditors {
    pub http_search: Entity<InputState>,
    /// Start menu app filter.
    pub launcher_search: Entity<InputState>,
    pub repeater_method: Entity<RepeaterMethodSelect>,
    /// Active environment picker (Repeater request bar).
    pub repeater_env: Entity<RepeaterEnvSelect>,
    /// Active collection switcher (Repeater sidebar).
    pub repeater_collection: Entity<RepeaterCollectionSelect>,
    pub repeater_url: Entity<InputState>,
    /// Request headers key/value form (Headers tab).
    pub repeater_headers: Entity<HeadersForm>,
    /// Request body in a code editor (Body tab).
    pub repeater_request: Entity<CodeEditor>,
    /// Pre-request + test scripts (split by a `---tests---` marker).
    pub repeater_scripts: Entity<CodeEditor>,
    /// Environment variables key/value form.
    pub repeater_env_vars: Entity<EnvVarsForm>,
    /// Raw response display in a code editor.
    pub repeater_response: Entity<CodeEditor>,
    pub tree_filter: Entity<InputState>,
    /// HTTP history detail — raw request wire format.
    pub history_request: Entity<CodeEditor>,
    /// HTTP history detail — raw response wire format.
    pub history_response: Entity<CodeEditor>,
    /// Virtualized traffic rows for the HTTP history table.
    pub http_history_list: Entity<HttpHistoryList>,
    /// Intercept editor for active draft request / response wire format.
    pub intercept_editor: Entity<CodeEditor>,
    /// Intercept scope host input.
    pub intercept_scope_input: Entity<InputState>,
    pub intruder_method: Entity<InputState>,
    pub intruder_url: Entity<InputState>,
    pub intruder_headers: Entity<TextareaState>,
    pub intruder_body: Entity<TextareaState>,
    pub intruder_payloads: Entity<TextareaState>,
    pub intruder_grep_keyword: Entity<InputState>,
    pub intruder_grep_regex: Entity<InputState>,
    pub intruder_session_token: Entity<InputState>,
    pub intruder_session_header: Entity<InputState>,
    pub settings_http_port: Entity<InputState>,
    pub settings_tls_port: Entity<InputState>,
    /// Coder settings → terminal shell path.
    pub coder_settings_shell: Entity<InputState>,
    /// Coder settings → directory new terminal sessions start in.
    pub coder_settings_default_cwd: Entity<InputState>,
    /// Coder settings → workspace ignore globs (comma separated in the field).
    pub coder_settings_ignore_globs: Entity<InputState>,
    pub coder_settings_pi_path: Entity<InputState>,
    pub coder_settings_opencode_path: Entity<InputState>,
    pub coder_settings_antigravity_path: Entity<InputState>,
    pub coder_settings_anthropic_key: Entity<InputState>,
    pub coder_settings_openai_key: Entity<InputState>,
    pub coder_settings_gemini_key: Entity<InputState>,
    pub coder_settings_deepseek_key: Entity<InputState>,
    pub coder_settings_ollama_url: Entity<InputState>,
    pub agent_cursor_api_key: Entity<InputState>,
    /// Coder settings → MCP server name.
    pub coder_settings_mcp_name: Entity<InputState>,
    /// Coder settings → MCP command (local) or URL (remote).
    pub coder_settings_mcp_command: Entity<InputState>,
    /// Coder settings → MCP args (local) or headers (remote).
    pub coder_settings_mcp_args: Entity<InputState>,
    /// Transport selected in the MCP add form. `Rc<Cell<_>>` so repaint
    /// handlers can flip it through the shared editor bag.
    pub mcp_transport: std::rc::Rc<std::cell::Cell<halo_bridge::McpTransport>>,
    pub crawler_target: Entity<InputState>,
    pub crawler_exclude: Entity<InputState>,
    pub crawler_log_filter: Entity<InputState>,
    /// Editor pool for Coder panes (index = pane id).
    pub coder_editors: Vec<Entity<CodeEditor>>,
    /// Horizontal scroll handles for each pane's file-tab strip.
    pub coder_tab_scrolls: Vec<CoderTabScroll>,
    /// Vertical scroll for the Coder explorer file tree (reveal active file).
    pub coder_explorer_scroll: CoderExplorerScroll,
    /// Inline create/rename name field in the Coder explorer tree.
    pub coder_explorer_inline_name: Entity<InputState>,
    /// Commit message for the Source Control panel.
    pub sc_commit_msg: Entity<InputState>,
    /// Inline "new branch" name field in the Source Control panel.
    pub sc_branch_name: Entity<InputState>,
    /// Virtualized staged / unstaged / ignored file list for Source Control.
    pub sc_changes_list: Entity<GitChangesList>,
    /// Search field for the Source Control pull-request inbox.
    pub sc_pr_search: Entity<InputState>,
    /// Focus handle for arrow-key movement in the pull-request list.
    pub sc_pr_focus: FocusHandle,
    /// Focus handle for the agent harness sheet, so Escape dismisses it.
    pub agent_provider_sheet_focus: FocusHandle,
    /// True after this open has taken focus, so paint does not steal it again.
    pub agent_provider_sheet_seen_open: std::rc::Rc<Cell<bool>>,
    /// True while a readiness probe has been scheduled from paint.
    pub agent_provider_probe_scheduled: std::rc::Rc<Cell<bool>>,
    /// Filter field for the Git Graph & Branch Explorer.
    pub sc_graph_filter: Entity<InputState>,
    /// One VT surface per live terminal session, shared by the Coder dock and
    /// the Terminal page. Keyed by session id, so tab switches never rebuild a
    /// grid and split panes each get their own entity.
    pub terminal_surfaces: crate::terminal::TerminalSurfaces,
    /// Composer prompt for the Coder Halo Agent panel.
    ///
    /// Multi-line so Shift+Enter inserts a newline; plain Enter submits
    /// (`submit_on_enter`).
    pub halo_agent_input: Entity<TextareaState>,
    /// Window-local transcript entities and per-chat Celestia scroll state.
    pub halo_agent_transcript: Entity<crate::pages::agent::AgentTranscript>,
    /// Window-local transcript for the subagent drawer (one child at a time).
    pub halo_agent_subagent: Entity<crate::pages::agent::SubagentTranscript>,
    /// Filter field inside the agent's harness/model dropdown.
    pub halo_agent_model_search: Entity<InputState>,
    /// Search field for Coder Quick Open (⌘P / Ctrl+P).
    pub coder_quick_open_input: Entity<InputState>,
    /// Custom Run command in the activity-rail Run/Build panel.
    pub coder_run_cmd_input: Entity<InputState>,
    /// Custom Build command in the activity-rail Run/Build panel.
    pub coder_build_cmd_input: Entity<InputState>,
    // ── Notes ──────────────────────────────────────────────────────────────
    pub notes_search: Entity<InputState>,
    pub notes_editor: Entity<CodeEditor>,
    pub notes_rename: Entity<InputState>,
    pub notes_library_search: Entity<InputState>,
    // ── API Mock ───────────────────────────────────────────────────────────
    pub mock_port: Entity<InputState>,
    pub mock_filter: Entity<InputState>,
    pub mock_method: Entity<InputState>,
    pub mock_path: Entity<InputState>,
    pub mock_status: Entity<InputState>,
    pub mock_body: Entity<CodeEditor>,
    // ── API Override ───────────────────────────────────────────────────────
    pub override_filter: Entity<InputState>,
    pub override_host: Entity<InputState>,
    pub override_method: Entity<InputState>,
    pub override_path: Entity<InputState>,
    pub override_status: Entity<InputState>,
    pub override_body: Entity<CodeEditor>,
    // ── JWT ────────────────────────────────────────────────────────────────
    /// Token input for the Decoder tab.
    pub jwt_token: Entity<CodeEditor>,
    /// Generator form: header / payload JSON.
    pub jwt_gen_header: Entity<CodeEditor>,
    pub jwt_gen_payload: Entity<CodeEditor>,
    /// Generator secret key.
    pub jwt_gen_secret: Entity<InputState>,
    pub mock_chaos_fixed: Entity<InputState>,
    pub mock_chaos_min: Entity<InputState>,
    pub mock_chaos_max: Entity<InputState>,
    pub mock_chaos_rate: Entity<InputState>,
    pub mock_chaos_status: Entity<InputState>,
    pub mock_simulate_path: Entity<InputState>,
    // ── Port Scanner ───────────────────────────────────────────────────────
    pub ps_target: Entity<InputState>,
    pub ps_ports: Entity<InputState>,
    pub ps_timeout: Entity<InputState>,
    pub ps_concurrency: Entity<InputState>,
    pub ps_delay: Entity<InputState>,
    pub ps_jitter: Entity<InputState>,
    // ── Comparer ───────────────────────────────────────────────────────────
    pub cmp_a: Entity<CodeEditor>,
    pub cmp_b: Entity<CodeEditor>,
    // ── File Explorer ──────────────────────────────────────────────────────
    pub fe_search: Entity<InputState>,
    pub r2_search: Entity<InputState>,
    pub r2_folder_name: Entity<InputState>,
    pub r2_bucket_name: Entity<InputState>,
    pub r2_account: Entity<InputState>,
    pub r2_access_key: Entity<InputState>,
    pub r2_secret: Entity<InputState>,
    pub r2_endpoint: Entity<InputState>,
    pub fe_folder_name: Entity<InputState>,
    pub wl_search: Entity<InputState>,
}

impl ToolEditors {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let repeater_headers = cx.new(|_cx| HeadersForm::new());
        repeater_headers.update(cx, |form, cx| {
            form.set_from_map(
                &std::collections::HashMap::from([(
                    "Accept".to_string(),
                    "application/json".to_string(),
                )]),
                window,
                cx,
            );
        });

        let repeater_request = code_editor("text", window, cx);
        repeater_request.update(cx, |e, cx| {
            e.set_text("", window, cx);
        });

        let repeater_scripts = code_editor("javascript", window, cx);
        repeater_scripts.update(cx, |e, cx| {
            e.set_text(
                "// Pre-request script (persisted with endpoint)\n// hb.variables.set(\"key\", \"value\")\n\n---tests---\n\n// Tests (persisted with endpoint)\n// hb.test(\"status 200\", () => expect(hb.response.status).to.equal(200))\n",
                window,
                cx,
            );
        });

        let repeater_env_vars = cx.new(|_cx| EnvVarsForm::new());
        repeater_env_vars.update(cx, |form, cx| {
            form.set_from_json(
                r#"{"host":{"value":"httpbin.org","enabled":true}}"#,
                window,
                cx,
            );
        });

        let repeater_response = code_editor("http", window, cx);
        repeater_response.update(cx, |e, cx| {
            e.set_text("// Send a request to see the response here.", window, cx);
        });

        // Forge panes own their height and draw their own chrome, so the
        // editors run flush inside them — see [`code_editor`].

        let history_request = code_editor("http", window, cx);
        history_request.update(cx, |e, cx| {
            e.set_text(
                "// Select a history row to inspect the request.",
                window,
                cx,
            );
        });

        let history_response = code_editor("http", window, cx);
        history_response.update(cx, |e, cx| {
            e.set_text(
                "// Select a history row to inspect the response.",
                window,
                cx,
            );
        });

        let intercept_editor = code_editor("http", window, cx);
        intercept_editor.update(cx, |e, cx| {
            e.set_text("// Waiting for intercepted requests...", window, cx);
        });

        let intercept_scope_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Add host scope (e.g. *.example.com or *)...")
        });

        let mock_body = code_editor("json", window, cx);
        mock_body.update(cx, |e, cx| {
            e.set_text(
                "{\n  \"message\": \"Hello from Mock API\",\n  \"status\": \"success\"\n}",
                window,
                cx,
            );
        });

        let override_body = code_editor("json", window, cx);
        override_body.update(cx, |e, cx| {
            e.set_text(
                "{\n  \"message\": \"Mocked response via API Override\",\n  \"intercepted\": true\n}",
                window,
                cx,
            );
        });

        Self {
            http_search: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search URL, host, method…")),
            launcher_search: cx.new(|cx| InputState::new(window, cx).placeholder("Search apps…")),
            repeater_method: cx.new(|cx| {
                SelectState::new(
                    repeater_method_options(None),
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                )
            }),
            repeater_env: cx.new(|cx| {
                SelectState::new(
                    repeater_env_options(std::iter::empty()),
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                )
            }),
            repeater_collection: cx.new(|cx| {
                SelectState::new(
                    repeater_collection_options(std::iter::empty()),
                    None,
                    window,
                    cx,
                )
            }),
            repeater_url: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://{{host}}/")
                    .default_value("https://httpbin.org/get")
            }),
            repeater_headers,
            repeater_request,
            repeater_scripts,
            repeater_env_vars,
            repeater_response,
            tree_filter: cx.new(|cx| InputState::new(window, cx).placeholder("Filter requests…")),
            history_request,
            history_response,
            http_history_list: cx.new(|cx| HttpHistoryList::new(cx)),
            intercept_editor,
            intercept_scope_input,
            intruder_method: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("GET")
                    .default_value("GET")
            }),
            intruder_url: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://example.com/?q=§test§")
                    .default_value("https://httpbin.org/get?q=§test§")
            }),
            intruder_headers: cx
                .new(|cx| TextareaState::new(window, cx).placeholder("Header-Name: value")),
            intruder_body: cx
                .new(|cx| TextareaState::new(window, cx).placeholder("Body with §markers§")),
            intruder_payloads: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("one payload per line")
                    .default_value("admin\nroot\nguest")
            }),
            intruder_grep_keyword: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Match keyword in response body")
            }),
            intruder_grep_regex: cx.new(|cx| {
                InputState::new(window, cx).placeholder(r#"Extract regex e.g. value="([^"]+)""#)
            }),
            intruder_session_token: cx
                .new(|cx| InputState::new(window, cx).placeholder("Token name (csrf, session…)")),
            intruder_session_header: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Header to update (X-CSRF-Token)")
            }),
            settings_http_port: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("8888")
                    .default_value("8888")
            }),
            settings_tls_port: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("8443")
                    .default_value("8443")
            }),
            coder_settings_shell: {
                let shell = halo_bridge::CoderSettings::default().shell;
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("/bin/zsh")
                        .default_value(shell)
                })
            },
            coder_settings_default_cwd: cx
                .new(|cx| InputState::new(window, cx).placeholder("Blank = $HOME")),
            coder_settings_ignore_globs: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("target, node_modules, dist, .git")
                    .default_value("target, node_modules, dist, .git")
            }),
            coder_settings_pi_path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Default ($PATH / standard location)")
            }),
            coder_settings_opencode_path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Default ($PATH / standard location)")
            }),
            coder_settings_antigravity_path: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Default (managed install / ANTIGRAVITY_ACP_EXECUTABLE)")
            }),
            coder_settings_anthropic_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("sk-ant-api03-...")
            }),
            coder_settings_openai_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("sk-proj-...")
            }),
            coder_settings_gemini_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("AIzaSy...")
            }),
            coder_settings_deepseek_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("sk-...")
            }),
            coder_settings_ollama_url: cx
                .new(|cx| InputState::new(window, cx).placeholder("http://localhost:11434/v1")),
            agent_cursor_api_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("Cursor user API key (cursor.com/settings → API Keys)")
            }),
            coder_settings_mcp_name: cx
                .new(|cx| InputState::new(window, cx).placeholder("server name")),
            coder_settings_mcp_command: cx.new(|cx| {
                InputState::new(window, cx).placeholder("command (local) or https://url (remote)")
            }),
            coder_settings_mcp_args: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("args space-separated (local) or Key: Value per line (remote)")
            }),
            mcp_transport: std::rc::Rc::new(std::cell::Cell::new(halo_bridge::McpTransport::Stdio)),
            crawler_target: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://example.com")
                    .default_value("https://example.com")
            }),
            crawler_exclude: cx
                .new(|cx| InputState::new(window, cx).placeholder("/logout, /delete, /billing")),
            crawler_log_filter: cx
                .new(|cx| InputState::new(window, cx).placeholder("Filter console…")),
            coder_editors: (0..CODER_MAX_PANES)
                .map(|i| {
                    // Pane owns chrome via the floating bar — no card border or
                    // footer minimap toggle inside the editor.
                    let editor = code_editor("plaintext", window, cx);
                    editor.update(cx, |e, cx| {
                        e.set_text(
                            if i == 0 {
                                "// Open a folder and select a file to edit."
                            } else {
                                "// Split editor"
                            },
                            window,
                            cx,
                        );
                    });
                    editor
                })
                .collect(),
            coder_tab_scrolls: (0..CODER_MAX_PANES)
                .map(|_| CoderTabScroll::new())
                .collect(),
            coder_explorer_scroll: CoderExplorerScroll::new(),
            coder_explorer_inline_name: cx
                .new(|cx| InputState::new(window, cx).placeholder("File or folder name")),
            sc_commit_msg: cx
                .new(|cx| InputState::new(window, cx).placeholder("Message (⌘Enter to commit)")),
            sc_branch_name: cx.new(|cx| {
                InputState::new(window, cx).placeholder("New branch name (Enter to create)")
            }),
            sc_changes_list: cx.new(|cx| GitChangesList::new(cx)),
            sc_pr_search: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search pull requests…")),
            sc_pr_focus: cx.focus_handle(),
            agent_provider_sheet_focus: cx.focus_handle(),
            agent_provider_sheet_seen_open: std::rc::Rc::new(Cell::new(false)),
            agent_provider_probe_scheduled: std::rc::Rc::new(Cell::new(false)),
            sc_graph_filter: cx
                .new(|cx| InputState::new(window, cx).placeholder("Filter branches, tags…")),
            terminal_surfaces: crate::terminal::TerminalSurfaces::new(),
            halo_agent_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Ask anything, edit files, or run commands… (@ files, / skills)")
                    .submit_on_enter(true)
                    .soft_wrap(true)
                    .auto_grow(2, 12)
            }),
            halo_agent_transcript: cx.new(crate::pages::agent::AgentTranscript::new),
            halo_agent_subagent: cx.new(crate::pages::agent::SubagentTranscript::new),
            halo_agent_model_search: cx
                .new(|cx| InputState::new(window, cx).placeholder("Filter models…")),
            coder_quick_open_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search files by name (⌘P)…")),
            coder_run_cmd_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Custom run command…")),
            coder_build_cmd_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Custom build command…")),
            notes_search: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search open tabs & contents…")),
            notes_editor: code_editor("markdown", window, cx),
            notes_rename: cx.new(|cx| InputState::new(window, cx).placeholder("Note title…")),
            notes_library_search: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search all saved notes…")),
            mock_port: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("4000")
                    .default_value("4000")
            }),
            mock_filter: cx.new(|cx| InputState::new(window, cx).placeholder("Filter endpoints…")),
            mock_method: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("GET")
                    .default_value("GET")
            }),
            mock_path: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("/api/resource/:id")
                    .default_value("/api/resource/:id")
            }),
            mock_status: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("200")
                    .default_value("200")
            }),
            mock_body,
            override_filter: cx
                .new(|cx| InputState::new(window, cx).placeholder("Filter hosts and rules…")),
            override_host: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Add host (e.g. api.example.com)…")
            }),
            override_method: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("GET")
                    .default_value("GET")
            }),
            override_path: cx.new(|cx| {
                InputState::new(window, cx).placeholder("/v1/charges/:id or https://host/path")
            }),
            override_status: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("200")
                    .default_value("200")
            }),
            override_body,
            jwt_token: code_editor("plaintext", window, cx),
            jwt_gen_header: code_editor("json", window, cx),
            jwt_gen_payload: code_editor("json", window, cx),
            jwt_gen_secret: cx
                .new(|cx| InputState::new(window, cx).placeholder("Secret key (HMAC)…")),
            mock_chaos_fixed: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("200")
                    .default_value("200")
            }),
            mock_chaos_min: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("100")
                    .default_value("100")
            }),
            mock_chaos_max: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("500")
                    .default_value("500")
            }),
            mock_chaos_rate: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("0.2")
                    .default_value("0.2")
            }),
            mock_chaos_status: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("500")
                    .default_value("500")
            }),
            mock_simulate_path: cx
                .new(|cx| InputState::new(window, cx).placeholder("/api/resource/12")),
            ps_target: cx.new(|cx| InputState::new(window, cx).placeholder("target host or IP")),
            ps_ports: cx
                .new(|cx| InputState::new(window, cx).placeholder("80, 443, 1-4, 8000..8010")),
            ps_timeout: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("800")
                    .default_value("800")
            }),
            ps_concurrency: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("100")
                    .default_value("100")
            }),
            ps_delay: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("300")
                    .default_value("300")
            }),
            ps_jitter: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("200")
                    .default_value("200")
            }),
            cmp_a: code_editor("plaintext", window, cx),
            cmp_b: code_editor("plaintext", window, cx),
            fe_search: cx.new(|cx| InputState::new(window, cx).placeholder("Search files…")),
            r2_search: cx.new(|cx| InputState::new(window, cx).placeholder("Search objects…")),
            r2_folder_name: cx.new(|cx| InputState::new(window, cx).placeholder("Folder name")),
            r2_bucket_name: cx.new(|cx| InputState::new(window, cx).placeholder("New bucket name")),
            r2_account: cx.new(|cx| InputState::new(window, cx).placeholder("Account ID")),
            r2_access_key: cx.new(|cx| InputState::new(window, cx).placeholder("Access Key ID")),
            r2_secret: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder("Leave blank to keep the saved secret")
            }),
            r2_endpoint: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("https://<account>.r2.cloudflarestorage.com")
            }),
            fe_folder_name: cx.new(|cx| InputState::new(window, cx).placeholder("Folder name")),
            wl_search: cx.new(|cx| InputState::new(window, cx).placeholder("Search wordlists…")),
        }
    }

    pub fn coder_editor(&self, pane: u8) -> &Entity<CodeEditor> {
        &self.coder_editors[pane as usize % self.coder_editors.len()]
    }

    pub fn coder_tab_scroll(&self, pane: u8) -> &CoderTabScroll {
        &self.coder_tab_scrolls[pane as usize % self.coder_tab_scrolls.len()]
    }

    /// Point the method picker at `method`, adding it to the menu when the
    /// loaded request uses a verb outside [`REPEATER_METHODS`].
    pub fn set_repeater_method(&self, method: &str, window: &mut Window, cx: &mut App) {
        let trimmed = method.trim();
        let method = if trimmed.is_empty() { "GET" } else { trimmed };
        let value = method.to_string();
        self.repeater_method.update(cx, |state, cx| {
            state.set_items(repeater_method_options(Some(method)), window, cx);
            state.set_selected_value(&value, window, cx);
        });
    }

    /// Refresh the environment picker from the current context list / active id.
    pub fn sync_repeater_env<'a>(
        &self,
        contexts: impl IntoIterator<Item = (&'a str, &'a str)>,
        active_id: Option<&str>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let value = active_id.unwrap_or(REPEATER_ENV_NONE).to_string();
        self.repeater_env.update(cx, |state, cx| {
            state.set_items(repeater_env_options(contexts), window, cx);
            state.set_selected_value(&value, window, cx);
        });
    }

    /// Refresh the collection switcher from the current collection list / active id.
    pub fn sync_repeater_collection<'a>(
        &self,
        collections: impl IntoIterator<Item = (&'a str, &'a str)>,
        active_id: Option<&str>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let options = repeater_collection_options(collections);
        let value = active_id
            .map(str::to_string)
            .or_else(|| options.first().map(|o| o.value().clone()));
        self.repeater_collection.update(cx, |state, cx| {
            state.set_items(options, window, cx);
            if let Some(value) = value.as_ref() {
                state.set_selected_value(value, window, cx);
            }
        });
    }
}

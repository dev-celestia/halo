//! Bridge command surface for the coder language servers.

use super::*;

impl Bridge {
    /// Remember CodeEditor entities so go-to-definition can open other tabs.
    pub fn coder_lsp_bind_editors(editors: &[Entity<CodeEditor>], cx: &mut App) {
        let pairs: Vec<_> = editors
            .iter()
            .take(crate::work::CODER_MAX_PANES)
            .map(|editor| (editor.clone(), editor.read(cx).editor_state()))
            .collect();

        // A pane gets its local completion source here rather than on the first
        // `didOpen`, so an empty pane completes too. Done before the runtime is
        // borrowed below — `install` needs `cx`.
        for (editor, _) in &pairs {
            super::coder_autofill::install(editor, None, cx);
        }

        let rt = runtime(cx);
        rt.bound_path = std::array::from_fn(|_| None);
        for (i, (editor, state)) in pairs.into_iter().enumerate() {
            rt.code_editors[i] = Some(editor);
            rt.editors[i] = Some(state);
        }
    }

    /// Open the completion menu at the caret of the focused Coder pane.
    ///
    /// Bound to ⌃Space. The engine's own `ShowCompletions` action is not part
    /// of the component crate's public surface, so the chord comes through here
    /// and addresses the pane the Coder considers focused.
    pub fn coder_show_completions(window: &mut Window, cx: &mut App) {
        let group = Self::state(cx).read(cx).coder.focused_pane;
        let Some(editor) = Self::coder_lsp_editor_for_pane(group, cx) else {
            return;
        };
        let state = editor.read(cx).editor_state();
        state.update(cx, |state, cx| state.show_completions(window, cx));
    }

    /// CodeEditor bound to `pane` (if `coder_lsp_bind_editors` has run).
    pub fn coder_lsp_editor_for_pane(pane: u8, cx: &App) -> Option<Entity<CodeEditor>> {
        let idx = pane_idx(pane);
        runtime_ref(cx).and_then(|rt| rt.code_editors[idx].clone())
    }

    /// Stop every running language server and clear status.
    pub fn coder_lsp_stop(cx: &mut App) {
        let clients: Vec<Arc<LanguageClient>> = {
            let rt = runtime(cx);
            rt.clients.drain().map(|(_, c)| c).collect()
        };
        for client in clients {
            let _ = client.shutdown_and_exit();
        }
        let rt = runtime(cx);
        rt.starting.clear();
        rt.progress.clear();
        rt.docs.clear();
        rt.diagnostics.clear();
        rt.lint_diagnostics.clear();
        rt.bound_path = std::array::from_fn(|_| None);
        Self::state(cx).update(cx, |s| {
            s.coder.lsp = CoderLspStatus::Idle;
        });
    }

    /// Clear providers for editors bound to `server_id` without touching highlighting.
    fn coder_lsp_server_unavailable(server_id: &str, reason: &str, cx: &mut App) {
        let paths: Vec<String> = {
            let rt = runtime(cx);
            rt.clients.remove(server_id);
            rt.starting.remove(server_id);
            rt.progress.retain(|(sid, _), _| sid != server_id);
            rt.docs
                .iter()
                .filter(|(_, d)| d.server_id == server_id)
                .map(|(p, _)| p.clone())
                .collect()
        };

        for path in &paths {
            for group in 0..crate::work::CODER_MAX_PANES {
                let editor = {
                    let rt = runtime_ref(cx);
                    let Some(rt) = rt else { continue };
                    if !rt.bound_path[group]
                        .as_deref()
                        .is_some_and(|bp| paths_match(bp, path))
                    {
                        continue;
                    }
                    rt.code_editors[group].clone()
                };
                if let Some(editor) = editor {
                    Self::coder_lsp_clear_providers(&editor, cx);
                }
            }
            {
                let rt = runtime(cx);
                rt.docs.remove(path);
            }
        }

        let entry = halo_lsp::catalog_entry(server_id);
        let offer_install =
            reason.contains("not on PATH") && entry.is_some_and(|e| !e.install_commands.is_empty());
        let install_label = entry.map(lsp_install_button_label);

        let message = match entry {
            Some(e)
                if e.id == "rust-analyzer"
                    && reason.contains("not on PATH")
                    && halo_lsp::rust_toolchain_missing() =>
            {
                "Rust isn't installed on this computer — needed for completions and go-to-definition."
                    .into()
            }
            Some(e) if reason.contains("not on PATH") => {
                format!(
                    "{} support isn't installed — needed for completions and go-to-definition.",
                    e.language_labels
                )
            }
            Some(e) if reason.contains("timed out") => {
                format!("{} took too long to start.", e.display_name)
            }
            Some(e) => format!("{} couldn't start.", e.display_name),
            None if reason.contains("not on PATH") => {
                "Language support isn't installed — needed for completions and go-to-definition."
                    .into()
            }
            None => "Language support couldn't start.".into(),
        };

        let alive = runtime_ref(cx).is_some_and(any_client_alive);
        Self::state(cx).update(cx, |s| {
            if offer_install {
                s.coder_lsp_offer_install = Some(server_id.to_string());
                s.coder_lsp_offer_install_label = install_label;
            } else {
                s.coder_lsp_offer_install = None;
                s.coder_lsp_offer_install_label = None;
            }
            s.last_error = Some(message);
            s.last_error_info = false;
            if !alive {
                s.coder.lsp = CoderLspStatus::Unavailable(reason.to_string());
            }
        });
        refresh_lsp_status(cx);
    }

    /// Ensure a language server is running for `language_id` under the current root.
    pub fn coder_lsp_ensure(language_id: &str, cx: &mut App) {
        if !is_registered(language_id) {
            return;
        }
        let Some(root) = Self::state(cx).read(cx).coder.root_path.clone() else {
            return;
        };
        let Some(spec) = server_for_language(language_id) else {
            return;
        };
        let server_id = spec.id.clone();

        // Already running for this workspace + server.
        if let Some(client) = runtime_ref(cx).and_then(|r| r.clients.get(&server_id).cloned()) {
            if paths_match(&client.root_path().display().to_string(), &root) && client.is_alive() {
                return;
            }
        }
        if runtime_ref(cx).is_some_and(|r| r.starting.contains(&server_id)) {
            return;
        }

        if !is_command_available(&spec.command) {
            Self::coder_lsp_server_unavailable(
                &server_id,
                &format!("`{}` not on PATH", spec.command),
                cx,
            );
            return;
        }

        // Drop a dead/wrong-root client for this server only.
        if let Some(old) = runtime(cx).clients.remove(&server_id) {
            let _ = old.shutdown_and_exit();
        }
        runtime(cx).progress.retain(|(sid, _), _| sid != &server_id);

        runtime(cx).starting.insert(server_id.clone());
        refresh_lsp_status(cx);

        let language_id = language_id.to_string();
        let executor = cx.background_executor().clone();

        cx.spawn(async move |cx: &mut AsyncApp| {
            let (tx, rx) = mpsc::channel();
            let spawn_ok = std::thread::Builder::new()
                .name("halo-lsp-start".into())
                .spawn({
                    let root = root.clone();
                    let spec = spec.clone();
                    move || {
                        let _ = tx.send(LanguageClient::start(spec, Path::new(&root)));
                    }
                })
                .is_ok();

            let result = if !spawn_ok {
                Err(LspError::message("failed to spawn lsp starter"))
            } else {
                loop {
                    match rx.try_recv() {
                        Ok(result) => break result,
                        Err(mpsc::TryRecvError::Empty) => {
                            executor.timer(Duration::from_millis(16)).await;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            break Err(LspError::message("lsp starter died"));
                        }
                    }
                }
            };

            let _ = cx.update(|cx| match result {
                Ok(client) => {
                    let client = Arc::new(client);
                    let (diag_tx, diag_rx) = mpsc::channel();
                    client.on_diagnostics(Arc::new(move |params| {
                        let _ = diag_tx.send(params);
                    }));
                    let (prog_tx, prog_rx) = mpsc::channel();
                    client.on_progress(Arc::new(move |params| {
                        let _ = prog_tx.send(params);
                    }));
                    {
                        let rt = runtime(cx);
                        rt.starting.remove(&server_id);
                        rt.clients.insert(server_id.clone(), client);
                    }
                    refresh_lsp_status(cx);
                    Self::coder_lsp_pump_diagnostics(diag_rx, cx);
                    Self::coder_lsp_pump_progress(server_id.clone(), prog_rx, cx);
                    let _ = language_id;
                }
                Err(err) => {
                    runtime(cx).starting.remove(&server_id);
                    Self::coder_lsp_server_unavailable(&server_id, &err.to_string(), cx);
                }
            });
        })
        .detach();
    }

    fn coder_lsp_pump_progress(server_id: String, rx: Receiver<ProgressParams>, cx: &mut App) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            loop {
                let mut batch = Vec::new();
                match rx.try_recv() {
                    Ok(params) => batch.push(params),
                    Err(mpsc::TryRecvError::Empty) => {
                        executor.timer(Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
                while let Ok(params) = rx.try_recv() {
                    batch.push(params);
                    if batch.len() >= 64 {
                        break;
                    }
                }
                let _ = cx.update(|cx| {
                    for params in batch.drain(..) {
                        Self::coder_lsp_apply_progress(&server_id, params, cx);
                    }
                    refresh_lsp_status(cx);
                });
            }
        })
        .detach();
    }

    fn coder_lsp_apply_progress(server_id: &str, params: ProgressParams, cx: &mut App) {
        let ProgressParamsValue::WorkDone(work) = params.value;
        let key = (server_id.to_string(), progress_token_key(&params.token));
        let rt = runtime(cx);
        match work {
            WorkDoneProgress::Begin(b) => {
                rt.progress_seq = rt.progress_seq.wrapping_add(1);
                let seq = rt.progress_seq;
                rt.progress.insert(
                    key,
                    LspProgressTask {
                        title: b.title,
                        message: b.message,
                        percentage: b.percentage,
                        seq,
                    },
                );
            }
            WorkDoneProgress::Report(r) => {
                if let Some(task) = rt.progress.get_mut(&key) {
                    if r.message.is_some() {
                        task.message = r.message;
                    }
                    if r.percentage.is_some() {
                        task.percentage = r.percentage;
                    }
                }
            }
            WorkDoneProgress::End(_) => {
                rt.progress.remove(&key);
            }
        }
    }

    fn coder_lsp_pump_diagnostics(rx: Receiver<PublishDiagnosticsParams>, cx: &mut App) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            loop {
                let mut batch = Vec::new();
                match rx.try_recv() {
                    Ok(params) => batch.push(params),
                    Err(mpsc::TryRecvError::Empty) => {
                        executor.timer(Duration::from_millis(50)).await;
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
                while let Ok(params) = rx.try_recv() {
                    batch.push(params);
                    if batch.len() >= 32 {
                        break;
                    }
                }
                let _ = cx.update(|cx| {
                    for params in batch.drain(..) {
                        Self::coder_lsp_apply_diagnostics(params, cx);
                    }
                });
            }
        })
        .detach();
    }

    fn coder_lsp_apply_diagnostics(params: PublishDiagnosticsParams, cx: &mut App) {
        let Some(raw_path) = uri_to_path(&params.uri).map(|p| p.display().to_string()) else {
            return;
        };
        let path = canonical_path_str(&raw_path);

        {
            let rt = runtime(cx);
            rt.diagnostics
                .insert(path.clone(), params.diagnostics.clone());
        }

        for group in 0..crate::work::CODER_MAX_PANES {
            let (editor, code_editor) = {
                let rt = runtime_ref(cx);
                let Some(rt) = rt else { continue };
                if !rt.bound_path[group]
                    .as_deref()
                    .is_some_and(|bp| paths_match(bp, &path))
                {
                    continue;
                }
                (rt.editors[group].clone(), rt.code_editors[group].clone())
            };
            let Some(editor) = editor else { continue };
            let lang = code_editor
                .as_ref()
                .map(|ce| ce.read(cx).language().to_string())
                .unwrap_or_default();
            let text_rope = editor.read(cx).text().clone();
            let lint_diags = compute_syntax_lint_diagnostics(&lang, &text_rope);
            {
                let rt = runtime(cx);
                rt.lint_diagnostics.insert(path.clone(), lint_diags.clone());
            }
            let lsp_diags = runtime_ref(cx)
                .and_then(|rt| rt.diagnostics.get(&path).cloned())
                .unwrap_or_default();
            editor.update(cx, |state, cx| {
                let text = state.text().clone();
                let merged = merge_lsp_and_lint_diagnostics(&text, &lsp_diags, &lint_diags);
                if let Some(set) = state.diagnostics_mut() {
                    set.reset(&text);
                    set.extend(merged);
                }
                cx.notify();
            });
        }

        refresh_lsp_status(cx);
    }

    /// Run the built-in tree-sitter syntax linter for `path` and apply the
    /// merged (LSP + syntax-linter) diagnostics to `editor` immediately.
    fn coder_lsp_lint_and_apply(
        path: &str,
        editor: &Entity<CodeEditor>,
        prune_fixed_syntax: bool,
        cx: &mut App,
    ) {
        let canon_path = canonical_path_str(path);
        let lang = editor.read(cx).language().to_string();
        let editor_state = editor.read(cx).editor_state();
        let text_rope = editor_state.read(cx).text().clone();
        let lint_diags = compute_syntax_lint_diagnostics(&lang, &text_rope);
        let line_count = text_rope.lines_len() as u32;

        let lsp_diags = {
            let rt = runtime(cx);
            rt.lint_diagnostics
                .insert(canon_path.clone(), lint_diags.clone());
            if prune_fixed_syntax {
                for key in [canon_path.as_str(), path] {
                    if let Some(existing) = rt.diagnostics.get_mut(key) {
                        existing.retain(|d| {
                            if d.range.start.line >= line_count && line_count > 0 {
                                return false;
                            }
                            if is_lsp_syntax_diagnostic(d) {
                                lint_diags.iter().any(|ld| {
                                    ld.range.start.line <= d.range.end.line
                                        && ld.range.end.line >= d.range.start.line
                                })
                            } else {
                                true
                            }
                        });
                    }
                }
            }
            rt.diagnostics
                .get(&canon_path)
                .or_else(|| rt.diagnostics.get(path))
                .cloned()
                .unwrap_or_default()
        };

        editor_state.update(cx, |state, cx| {
            let text = state.text().clone();
            let merged = merge_lsp_and_lint_diagnostics(&text, &lsp_diags, &lint_diags);
            if let Some(set) = state.diagnostics_mut() {
                set.reset(&text);
                set.extend(merged);
            }
            cx.notify();
        });
        refresh_lsp_status(cx);
    }

    /// Open (or re-open) a document in the language server and attach providers.
    pub fn coder_lsp_did_open(
        path: &str,
        language_id: &str,
        text: &str,
        editor: &Entity<CodeEditor>,
        group: u8,
        cx: &mut App,
    ) {
        let group_idx = pane_idx(group);
        let editor_state = editor.read(cx).editor_state();
        {
            let rt = runtime(cx);
            rt.editors[group_idx] = Some(editor_state.clone());
            rt.code_editors[group_idx] = Some(editor.clone());
            rt.bound_path[group_idx] = Some(path.to_string());
        }

        // Immediately run the built-in syntax linter so syntax errors show up
        // on frame 1 even before a language server starts or for non-LSP files.
        Self::coder_lsp_lint_and_apply(path, editor, false, cx);

        if !is_registered(language_id) {
            Self::coder_lsp_clear_providers(editor, cx);
            Self::coder_lsp_lint_and_apply(path, editor, false, cx);
            if matches!(
                Self::state(cx).read(cx).coder.lsp,
                CoderLspStatus::Unavailable(_)
            ) && !runtime_ref(cx).is_some_and(any_client_alive)
            {
                Self::state(cx).update(cx, |s| {
                    s.coder.lsp = CoderLspStatus::Idle;
                    s.last_error = None;
                    s.coder_lsp_offer_install = None;
                    s.coder_lsp_offer_install_label = None;
                });
                refresh_lsp_status(cx);
            }
            return;
        }

        let Some(server_id) = server_id_for_language(language_id).map(str::to_string) else {
            return;
        };

        Self::coder_lsp_ensure(language_id, cx);
        refresh_lsp_status(cx);

        let has_client = runtime_ref(cx)
            .and_then(|r| r.clients.get(&server_id))
            .is_some_and(|c| c.is_alive());
        let is_starting = runtime_ref(cx).is_some_and(|r| r.starting.contains(&server_id));

        if !has_client && !is_starting {
            // Server unavailable for this language — keep highlighting and syntax linter.
            Self::coder_lsp_clear_providers(editor, cx);
            Self::coder_lsp_lint_and_apply(path, editor, false, cx);
            return;
        }

        Self::coder_lsp_install_providers(editor, group_idx as u8, cx);

        let Some(client) = runtime_ref(cx)
            .and_then(|r| r.clients.get(&server_id).cloned())
            .filter(|c| c.is_alive())
        else {
            let path = path.to_string();
            let language_id = language_id.to_string();
            let editor = editor.clone();
            let server_id = server_id.clone();
            let executor = cx.background_executor().clone();
            cx.spawn(async move |cx: &mut AsyncApp| {
                for _ in 0..600 {
                    executor.timer(Duration::from_millis(50)).await;
                    let ready = cx.update(|cx| {
                        runtime_ref(cx)
                            .and_then(|r| r.clients.get(&server_id))
                            .is_some_and(|c| c.is_alive())
                    });
                    if ready {
                        let _ = cx.update(|cx| {
                            let still_bound = runtime_ref(cx).is_some_and(|r| {
                                r.bound_path[group_idx]
                                    .as_deref()
                                    .is_some_and(|bp| paths_match(bp, &path))
                            });
                            if still_bound {
                                let latest_text = editor.read(cx).text(cx);
                                Self::coder_lsp_did_open(
                                    &path,
                                    &language_id,
                                    &latest_text,
                                    &editor,
                                    group_idx as u8,
                                    cx,
                                );
                            }
                        });
                        return;
                    }
                    let give_up = cx.update(|cx| {
                        !runtime_ref(cx).is_some_and(|r| r.starting.contains(&server_id))
                            && !runtime_ref(cx)
                                .and_then(|r| r.clients.get(&server_id))
                                .is_some_and(|c| c.is_alive())
                    });
                    if give_up {
                        let _ = cx.update(|cx| {
                            Self::coder_lsp_clear_providers(&editor, cx);
                        });
                        return;
                    }
                }
                let _ = cx.update(|cx| {
                    Self::coder_lsp_server_unavailable(
                        &server_id,
                        "language server start timed out",
                        cx,
                    );
                });
            })
            .detach();
            return;
        };

        let Ok(uri) = path_to_uri(Path::new(path)) else {
            return;
        };

        let (version, already_open) = {
            let rt = runtime(cx);
            let already_open = rt.docs.contains_key(path);
            let entry = rt.docs.entry(path.to_string()).or_insert_with(|| OpenDoc {
                language_id: language_id.to_string(),
                server_id: server_id.clone(),
                version: AtomicI32::new(0),
            });
            entry.language_id = language_id.to_string();
            entry.server_id = server_id.clone();
            let v = if already_open {
                entry.version.fetch_add(1, Ordering::SeqCst) + 1
            } else {
                entry.version.store(1, Ordering::SeqCst);
                1
            };
            (v, already_open)
        };

        let text = text.to_string();
        let is_rust = language_id == "rust";
        let language_id = language_id.to_string();

        cx.background_spawn({
            let client = client.clone();
            async move {
                if already_open {
                    let _ = client.did_change_full(uri, version, text);
                } else {
                    let _ = client.did_open(uri, &language_id, version, text);
                }
            }
        })
        .detach();

        // rust-analyzer publishes nothing for a file it cannot attach to a
        // Cargo package — a fresh file in a non-Cargo folder, or one at a
        // workspace root outside every member crate. Link such files as a
        // standalone inline rust-project so they still get diagnostics.
        if is_rust && path_is_outside_cargo_packages(Path::new(path)) {
            let standalone = {
                let rt = runtime(cx);
                let entry = rt.standalone_files.entry(server_id.clone()).or_default();
                let path_buf = PathBuf::from(path);
                if entry.contains(&path_buf) {
                    None
                } else {
                    entry.push(path_buf.clone());
                    Some(entry.clone())
                }
            };
            if let Some(files) = standalone {
                let _ = client.set_standalone_project(files);
            }
        }

        Self::coder_lsp_lint_and_apply(path, editor, false, cx);
    }

    pub fn coder_lsp_did_close(path: &str, cx: &mut App) {
        let canon_path = canonical_path_str(path);
        let client = runtime_ref(cx).and_then(|r| client_for_path(r, path));
        let Some(client) = client else {
            let rt = runtime(cx);
            rt.docs.remove(path);
            rt.diagnostics.remove(&canon_path);
            rt.diagnostics.remove(path);
            rt.lint_diagnostics.remove(&canon_path);
            rt.lint_diagnostics.remove(path);
            return;
        };
        let still_open = Self::state(cx)
            .read(cx)
            .coder
            .open_files
            .iter()
            .any(|f| paths_match(&f.path, path));
        if still_open {
            return;
        }
        {
            let rt = runtime(cx);
            rt.docs.remove(path);
            rt.diagnostics.remove(&canon_path);
            rt.diagnostics.remove(path);
            rt.lint_diagnostics.remove(&canon_path);
            rt.lint_diagnostics.remove(path);
            for slot in &mut rt.bound_path {
                if slot.as_deref().is_some_and(|bp| paths_match(bp, path)) {
                    *slot = None;
                }
            }
        }
        refresh_lsp_status(cx);
        if let Ok(uri) = path_to_uri(Path::new(path)) {
            cx.background_spawn(async move {
                let _ = client.did_close(uri);
            })
            .detach();
        }
    }

    pub fn coder_lsp_did_save(path: &str, text: &str, cx: &mut App) {
        let Some(client) = runtime_ref(cx).and_then(|r| client_for_path(r, path)) else {
            return;
        };
        if !runtime_ref(cx).is_some_and(|r| r.docs.contains_key(path)) {
            return;
        }
        let Ok(uri) = path_to_uri(Path::new(path)) else {
            return;
        };
        let text = text.to_string();
        cx.background_spawn(async move {
            let _ = client.did_save(uri, Some(text));
        })
        .detach();
    }

    /// Format `path` via its language server (`textDocument/formatting`). Returns
    /// `None` when no client is available or the server returns no edits.
    pub fn coder_lsp_format_document(path: &str, cx: &App) -> Option<Vec<TextEdit>> {
        let client = runtime_ref(cx).and_then(|r| client_for_path(r, path))?;
        let uri = path_to_uri(Path::new(path)).ok()?;
        let options = lsp_types::FormattingOptions {
            tab_size: u32::from(Self::state(cx).read(cx).coder_settings.tab_size),
            insert_spaces: true,
            ..Default::default()
        };
        client
            .formatting(uri, options)
            .ok()
            .filter(|e| !e.is_empty())
    }

    /// Debounced full-document change notification for the bound path in `group`.
    pub fn coder_lsp_on_editor_change(editor: &Entity<CodeEditor>, group: u8, cx: &mut App) {
        let group_idx = pane_idx(group);
        let path = runtime_ref(cx)
            .and_then(|r| r.bound_path[group_idx].clone())
            .or_else(|| {
                Self::state(cx)
                    .read(cx)
                    .coder
                    .pane_active(group)
                    .map(str::to_string)
            });
        let Some(path) = path else {
            return;
        };

        // Run the built-in tree-sitter syntax linter immediately on every edit
        // so `EditorState`'s per-keystroke diagnostic reset is repopulated on
        // the same frame without waiting for the 200ms LSP debounce.
        Self::coder_lsp_lint_and_apply(&path, editor, true, cx);

        if !runtime_ref(cx).is_some_and(|r| r.docs.contains_key(&path)) {
            return;
        }
        let Some(client) = runtime_ref(cx).and_then(|r| client_for_path(r, &path)) else {
            return;
        };

        let seq = {
            let rt = runtime(cx);
            rt.change_seq = rt.change_seq.wrapping_add(1);
            rt.change_seq
        };
        let Ok(uri) = path_to_uri(Path::new(&path)) else {
            return;
        };
        let editor = editor.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            executor.timer(Duration::from_millis(200)).await;
            let payload = cx.update(|cx| {
                let current = runtime_ref(cx).map(|r| r.change_seq).unwrap_or(0);
                if current != seq {
                    return None;
                }
                let version = {
                    let rt = runtime(cx);
                    let doc = rt.docs.get(&path)?;
                    doc.version.fetch_add(1, Ordering::SeqCst) + 1
                };
                let text = editor.read(cx).text(cx);
                Some((version, text))
            });
            if let Some((version, text)) = payload {
                let _ = client.did_change_full(uri, version, text);
            }
        })
        .detach();
    }

    fn coder_lsp_install_providers(editor: &Entity<CodeEditor>, group: u8, cx: &mut App) {
        let adapter = Rc::new(CoderLspAdapter { group });
        let show_document: ShowDocumentHandler =
            Rc::new(|params, window, cx| Self::coder_lsp_show_document(params, window, cx));

        // Completion is the server's list *plus* the buffer's own words, so a
        // half-indexed or missing server still autocompletes.
        super::coder_autofill::install(editor, Some(adapter.clone()), cx);

        let editor_state = editor.read(cx).editor_state();
        editor_state.update(cx, |state, cx| {
            state.lsp_mut().hover_provider = Some(adapter.clone());
            state.lsp_mut().definition_provider = Some(adapter.clone());
            state.lsp_mut().code_action_providers = vec![adapter];
            state.lsp_mut().show_document = Some(show_document);
            state.refresh(cx);
        });
    }

    fn coder_lsp_clear_providers(editor: &Entity<CodeEditor>, cx: &mut App) {
        // No server for this buffer — the local source is all that is left, and
        // it is exactly what makes JSON / YAML / Markdown / plain text usable.
        super::coder_autofill::install(editor, None, cx);

        let editor_state = editor.read(cx).editor_state();
        editor_state.update(cx, |state, cx| {
            state.lsp_mut().hover_provider = None;
            state.lsp_mut().definition_provider = None;
            state.lsp_mut().code_action_providers.clear();
            state.lsp_mut().show_document = None;
            if let Some(set) = state.diagnostics_mut() {
                set.clear();
            }
            state.refresh(cx);
        });
    }

    pub(super) fn coder_lsp_apply_workspace_edit(
        edit: &WorkspaceEdit,
        current: &Entity<EditorState>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut by_path: HashMap<String, Vec<TextEdit>> = HashMap::new();

        if let Some(changes) = &edit.changes {
            for (uri, edits) in changes {
                if let Some(path) = uri_to_path(uri) {
                    by_path
                        .entry(path.display().to_string())
                        .or_default()
                        .extend(edits.clone());
                }
            }
        }

        if let Some(document_changes) = &edit.document_changes {
            match document_changes {
                lsp_types::DocumentChanges::Edits(edits) => {
                    for TextDocumentEdit {
                        text_document,
                        edits,
                    } in edits
                    {
                        if let Some(path) = uri_to_path(&text_document.uri) {
                            let list = by_path.entry(path.display().to_string()).or_default();
                            for e in edits {
                                match e {
                                    lsp_types::OneOf::Left(te) => list.push(te.clone()),
                                    lsp_types::OneOf::Right(annotated) => {
                                        list.push(annotated.text_edit.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                lsp_types::DocumentChanges::Operations(ops) => {
                    for op in ops {
                        if let lsp_types::DocumentChangeOperation::Edit(TextDocumentEdit {
                            text_document,
                            edits,
                        }) = op
                        {
                            if let Some(path) = uri_to_path(&text_document.uri) {
                                let list = by_path.entry(path.display().to_string()).or_default();
                                for e in edits {
                                    match e {
                                        lsp_types::OneOf::Left(te) => list.push(te.clone()),
                                        lsp_types::OneOf::Right(annotated) => {
                                            list.push(annotated.text_edit.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        for (path, edits) in by_path {
            // Prefer an open editor bound to this path.
            let mut applied = false;
            for group in 0..crate::work::CODER_MAX_PANES {
                let editor_state = {
                    let rt = runtime_ref(cx);
                    let Some(rt) = rt else { continue };
                    if !rt.bound_path[group]
                        .as_deref()
                        .is_some_and(|bp| paths_match(bp, &path))
                    {
                        continue;
                    }
                    rt.editors[group].clone()
                };
                if let Some(editor_state) = editor_state {
                    editor_state.update(cx, |state, cx| {
                        state.apply_lsp_edits(&edits, window, cx);
                    });
                    applied = true;
                }
            }
            if !applied {
                // Apply to the editor that triggered the action if it matches.
                let bound = (0..crate::work::CODER_MAX_PANES).any(|g| {
                    runtime_ref(cx)
                        .and_then(|rt| rt.editors[g].as_ref())
                        .is_some_and(|e| e == current)
                        && runtime_ref(cx)
                            .and_then(|rt| rt.bound_path[g].as_deref())
                            .is_some_and(|p| paths_match(p, &path))
                });
                if bound {
                    current.update(cx, |state, cx| {
                        state.apply_lsp_edits(&edits, window, cx);
                    });
                }
            }
            let _ = path;
        }
    }

    fn coder_lsp_show_document(
        params: &ShowDocumentParams,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if params.external == Some(true) {
            return false;
        }
        let Some(path) = uri_to_path(&params.uri) else {
            return false;
        };
        let path = path.display().to_string();
        let group = Self::state(cx).read(cx).coder.focused_pane;
        let group_idx = pane_idx(group);
        let Some(target) = runtime_ref(cx).and_then(|rt| rt.code_editors[group_idx].clone()) else {
            return false;
        };

        if Self::coder_open_file_in_group(&path, &target, group, window, cx).is_err() {
            return false;
        }

        if let Some(range) = params.selection {
            let editor_state = target.read(cx).editor_state();
            editor_state.update(cx, |state, cx| {
                let start = state.text().position_to_offset(&range.start);
                let end = state.text().position_to_offset(&range.end);
                let _ = (start, end, cx);
            });
        }
        true
    }

    /// Subscribe editor change events for LSP didChange (call once from the UI).
    pub fn coder_lsp_subscribe_editor(editor: &Entity<CodeEditor>, group: u8, cx: &mut App) {
        let editor_state = editor.read(cx).editor_state();
        let editor = editor.clone();
        cx.subscribe(&editor_state, move |_entity, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                Self::coder_on_buffer_change(&editor, group, cx);
                Self::coder_lsp_on_editor_change(&editor, group, cx);
            }
        })
        .detach();
    }

    /// Refresh the Language Servers settings list (PATH detection).
    pub fn coder_lsp_refresh_servers(cx: &mut App) {
        use crate::work::CoderLspServerRow;
        use halo_lsp::{catalog, is_command_available};

        let rows: Vec<CoderLspServerRow> = catalog()
            .iter()
            .map(|entry| CoderLspServerRow {
                id: entry.id.into(),
                display_name: entry.display_name.into(),
                languages: entry.language_labels.into(),
                command: entry.command.into(),
                installed: is_command_available(entry.command),
                install_hint: entry.install_hint.into(),
                can_auto_install: !entry.install_commands.is_empty(),
            })
            .collect();

        Self::state(cx).update(cx, |s| {
            s.coder_lsp_servers = rows;
            if s.coder_lsp_installing_id.is_none() {
                s.coder_lsp_install_message =
                    Some("Checked PATH for known language servers.".into());
            }
        });
    }

    /// Run the catalog install command for `server_id` (e.g. `rust-analyzer`).
    pub fn coder_lsp_install_server(server_id: &str, cx: &mut App) {
        if Self::state(cx).read(cx).coder_lsp_installing_id.is_some() {
            return;
        }

        let server_id = server_id.to_string();
        Self::state(cx).update(cx, |s| {
            s.coder_lsp_installing_id = Some(server_id.clone());
            s.coder_lsp_offer_install = None;
            s.coder_lsp_offer_install_label = None;
            s.last_error = None;
            s.coder_lsp_install_message = Some(format!("Installing {server_id}…"));
            s.apply_ok_msg(format!("Installing {server_id}…"));
        });

        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let (tx, rx) = std::sync::mpsc::channel();
            let spawn_ok = std::thread::Builder::new()
                .name("halo-lsp-install".into())
                .spawn({
                    let server_id = server_id.clone();
                    move || {
                        let _ = tx.send(halo_lsp::install_server(&server_id));
                    }
                })
                .is_ok();

            let result = if !spawn_ok {
                Err(halo_lsp::LspError::message(
                    "failed to spawn install worker",
                ))
            } else {
                loop {
                    match rx.try_recv() {
                        Ok(result) => break result,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            executor.timer(Duration::from_millis(50)).await;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            break Err(halo_lsp::LspError::message("install worker died"));
                        }
                    }
                }
            };

            cx.update(|cx| {
                Self::state(cx).update(cx, |s| {
                    s.coder_lsp_installing_id = None;
                    match &result {
                        Ok(msg) => {
                            s.coder_lsp_offer_install = None;
                            s.coder_lsp_offer_install_label = None;
                            s.coder_lsp_install_message = Some(msg.clone());
                            if matches!(s.coder.lsp, CoderLspStatus::Unavailable(_)) {
                                s.coder.lsp = CoderLspStatus::Idle;
                            }
                            s.apply_ok_msg(msg.clone());
                        }
                        Err(err) => {
                            let msg = err.to_string();
                            s.coder_lsp_install_message = Some(msg.clone());
                            // Re-offer install so the toast can show Install again.
                            s.coder_lsp_offer_install = Some(server_id.clone());
                            s.coder_lsp_offer_install_label =
                                halo_lsp::catalog_entry(&server_id).map(lsp_install_button_label);
                            s.apply_err(crate::error::BridgeError::message(msg));
                        }
                    }
                });
                Self::coder_lsp_refresh_servers(cx);
                if result.is_ok() {
                    Self::coder_lsp_retry_bound_editors(&server_id, cx);
                }
            });
        })
        .detach();
    }

    /// After a successful install, re-attach LSP to open editors for that server.
    fn coder_lsp_retry_bound_editors(server_id: &str, cx: &mut App) {
        use crate::bridge::util::coder_lsp_language_for_path;

        let Some(entry) = halo_lsp::catalog_entry(server_id) else {
            return;
        };

        let retries: Vec<(usize, String, String, Entity<CodeEditor>)> = {
            let Some(rt) = runtime_ref(cx) else {
                return;
            };
            (0..crate::work::CODER_MAX_PANES)
                .filter_map(|group| {
                    let path = rt.bound_path[group].clone()?;
                    let editor = rt.code_editors[group].clone()?;
                    let language_id = coder_lsp_language_for_path(&path);
                    if !entry.language_ids.contains(&language_id) {
                        return None;
                    }
                    Some((group, path, language_id.to_string(), editor))
                })
                .collect()
        };

        for (group, path, language_id, editor) in retries {
            let text = editor.read(cx).text(cx);
            Self::coder_lsp_did_open(&path, &language_id, &text, &editor, group as u8, cx);
        }
    }
}

/// Toast / settings button label, e.g. `"Install Rust"` or `"Install TypeScript"`.
fn lsp_install_button_label(entry: &halo_lsp::LanguageServerCatalogEntry) -> String {
    let primary = entry
        .language_labels
        .split(',')
        .next()
        .unwrap_or(entry.display_name)
        .trim();
    format!("Install {primary}")
}

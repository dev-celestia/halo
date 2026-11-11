//! Editor draft capture/restore per work tab, and preference defaults.

use super::*;

impl HaloApp {
    pub(super) fn capture_editor_draft(&self, kind: WorkKind, cx: &App) -> EditorDraft {
        let input = |entity: &Entity<InputState>| entity.read(cx).value().to_string();
        let area = |entity: &Entity<TextareaState>| entity.read(cx).value().to_string();
        let code = |entity: &Entity<celestia_ui::components::CodeEditor>| entity.read(cx).text(cx);
        match kind {
            WorkKind::HttpHistory => EditorDraft::Http {
                search: input(&self.editors.http_search),
            },
            WorkKind::Intercept => EditorDraft::Intercept {
                raw: code(&self.editors.intercept_editor),
                scope: input(&self.editors.intercept_scope_input),
            },
            WorkKind::Repeater => {
                let headers = self.editors.repeater_headers.read(cx).to_map(cx);
                let body = code(&self.editors.repeater_request);
                EditorDraft::Repeater {
                    method: self
                        .editors
                        .repeater_method
                        .read(cx)
                        .selected_value()
                        .cloned()
                        .unwrap_or_default(),
                    url: input(&self.editors.repeater_url),
                    // Keep drafts as raw HTTP so older in-session drafts still restore.
                    request: headers_map_to_raw(&headers, &body),
                    scripts: code(&self.editors.repeater_scripts),
                    env: self.editors.repeater_env_vars.read(cx).to_json(cx),
                    response: code(&self.editors.repeater_response),
                    tree_filter: input(&self.editors.tree_filter),
                }
            }
            WorkKind::Intruder => EditorDraft::Intruder {
                method: input(&self.editors.intruder_method),
                url: input(&self.editors.intruder_url),
                headers: area(&self.editors.intruder_headers),
                body: area(&self.editors.intruder_body),
                payloads: area(&self.editors.intruder_payloads),
                grep_keyword: input(&self.editors.intruder_grep_keyword),
                grep_regex: input(&self.editors.intruder_grep_regex),
                session_token: input(&self.editors.intruder_session_token),
                session_header: input(&self.editors.intruder_session_header),
            },
            WorkKind::Crawler => EditorDraft::Crawler {
                target: input(&self.editors.crawler_target),
                exclude: input(&self.editors.crawler_exclude),
                log_filter: input(&self.editors.crawler_log_filter),
            },
            WorkKind::Coder => EditorDraft::Coder,
            WorkKind::SourceControl => EditorDraft::SourceControl,
            WorkKind::Agent => EditorDraft::Agent,
            WorkKind::Notes => EditorDraft::Notes {
                content: code(&self.editors.notes_editor),
                search: input(&self.editors.notes_search),
                rename: input(&self.editors.notes_rename),
                library_search: input(&self.editors.notes_library_search),
            },
            WorkKind::ApiMock | WorkKind::ApiOverride => EditorDraft::Mock,
            WorkKind::Jwt => EditorDraft::Jwt,
            WorkKind::PortScanner => EditorDraft::PortScanner,
            WorkKind::Comparer => EditorDraft::Comparer,
            WorkKind::FileExplorer => EditorDraft::FileExplorer,
            WorkKind::R2Storage => EditorDraft::R2Storage,
            WorkKind::Wordlists => EditorDraft::Wordlists,
            WorkKind::SplitView => EditorDraft::SplitView,
            WorkKind::Doctor => EditorDraft::Doctor,
            WorkKind::Diagram => EditorDraft::Diagram,
            WorkKind::Settings => EditorDraft::Settings,
        }
    }

    pub(super) fn apply_editor_draft(
        &mut self,
        draft: &EditorDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match draft {
            EditorDraft::Http { search } => {
                let search = search.clone();
                self.editors.http_search.update(cx, |s, cx| {
                    s.set_value(search, window, cx);
                });
            }
            EditorDraft::Intercept { raw, scope } => {
                let raw = raw.clone();
                let scope = scope.clone();
                self.editors.intercept_editor.update(cx, |e, cx| {
                    e.set_text(raw, window, cx);
                });
                self.editors.intercept_scope_input.update(cx, |s, cx| {
                    s.set_value(scope, window, cx);
                });
                self.last_response_text = None;
            }
            EditorDraft::Repeater {
                method,
                url,
                request,
                scripts,
                env,
                response,
                tree_filter,
            } => {
                let method = method.clone();
                let url = url.clone();
                let tree_filter = tree_filter.clone();
                let request = request.clone();
                let scripts = scripts.clone();
                let env = env.clone();
                let response_text = response.clone();
                self.editors.set_repeater_method(&method, window, cx);
                self.editors.repeater_url.update(cx, |s, cx| {
                    VariableInput::set_value(s, url, window, cx);
                });
                self.editors.tree_filter.update(cx, |s, cx| {
                    s.set_value(tree_filter, window, cx);
                });
                let (headers, body) = split_raw_http(&request);
                self.editors.repeater_headers.update(cx, |form, cx| {
                    form.set_from_map(&headers, window, cx);
                });
                self.editors.repeater_request.update(cx, |e, cx| {
                    e.set_text(body, window, cx);
                });
                self.editors.repeater_scripts.update(cx, |e, cx| {
                    e.set_text(scripts, window, cx);
                });
                self.editors.repeater_env_vars.update(cx, |form, cx| {
                    form.set_from_json(&env, window, cx);
                });
                self.editors.repeater_response.update(cx, |e, cx| {
                    e.set_text(response_text.clone(), window, cx);
                });
                self.last_response_text = Some(response_text);
            }
            EditorDraft::Intruder {
                method,
                url,
                headers,
                body,
                payloads,
                grep_keyword,
                grep_regex,
                session_token,
                session_header,
            } => {
                let method = method.clone();
                let url = url.clone();
                let headers = headers.clone();
                let body = body.clone();
                let payloads = payloads.clone();
                let grep_keyword = grep_keyword.clone();
                let grep_regex = grep_regex.clone();
                let session_token = session_token.clone();
                let session_header = session_header.clone();
                self.editors.intruder_method.update(cx, |s, cx| {
                    s.set_value(method, window, cx);
                });
                self.editors.intruder_url.update(cx, |s, cx| {
                    s.set_value(url, window, cx);
                });
                self.editors.intruder_headers.update(cx, |s, cx| {
                    s.set_value(headers, window, cx);
                });
                self.editors.intruder_body.update(cx, |s, cx| {
                    s.set_value(body, window, cx);
                });
                self.editors.intruder_payloads.update(cx, |s, cx| {
                    s.set_value(payloads, window, cx);
                });
                self.editors.intruder_grep_keyword.update(cx, |s, cx| {
                    s.set_value(grep_keyword, window, cx);
                });
                self.editors.intruder_grep_regex.update(cx, |s, cx| {
                    s.set_value(grep_regex, window, cx);
                });
                self.editors.intruder_session_token.update(cx, |s, cx| {
                    s.set_value(session_token, window, cx);
                });
                self.editors.intruder_session_header.update(cx, |s, cx| {
                    s.set_value(session_header, window, cx);
                });
            }
            EditorDraft::Crawler {
                target,
                exclude,
                log_filter,
            } => {
                let target = target.clone();
                let exclude = exclude.clone();
                let log_filter = log_filter.clone();
                self.editors.crawler_target.update(cx, |s, cx| {
                    s.set_value(target, window, cx);
                });
                self.editors.crawler_exclude.update(cx, |s, cx| {
                    s.set_value(exclude, window, cx);
                });
                self.editors.crawler_log_filter.update(cx, |s, cx| {
                    s.set_value(log_filter, window, cx);
                });
            }
            EditorDraft::Coder => {
                let _ = Bridge::coder_sync_editors(&self.editors.coder_editors, window, cx);
            }
            // Sessions live in the store keyed by session id, not in editors.
            EditorDraft::SourceControl => {}
            EditorDraft::Agent => {}
            EditorDraft::Notes {
                content,
                search,
                rename,
                library_search,
            } => {
                let content = content.clone();
                let search = search.clone();
                let rename = rename.clone();
                let library_search = library_search.clone();
                self.editors.notes_editor.update(cx, |e, cx| {
                    e.set_text(content, window, cx);
                });
                self.editors.notes_search.update(cx, |s, cx| {
                    s.set_value(search, window, cx);
                });
                self.editors.notes_rename.update(cx, |s, cx| {
                    s.set_value(rename, window, cx);
                });
                self.editors.notes_library_search.update(cx, |s, cx| {
                    s.set_value(library_search, window, cx);
                });
            }
            EditorDraft::Mock => {}
            EditorDraft::Jwt => {}
            EditorDraft::PortScanner => {}
            EditorDraft::Comparer => {}
            EditorDraft::FileExplorer => {}
            EditorDraft::R2Storage => {}
            EditorDraft::Wordlists => {}
            EditorDraft::SplitView => {}
            EditorDraft::Doctor => {}
            EditorDraft::Diagram => {}
            EditorDraft::Settings => {}
        }
    }

    pub(super) fn apply_editor_defaults(
        &mut self,
        kind: WorkKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match kind {
            WorkKind::HttpHistory => {
                let search = self
                    .bridge
                    .history_filter
                    .search
                    .clone()
                    .unwrap_or_default();
                self.editors.http_search.update(cx, |s, cx| {
                    s.set_value(search, window, cx);
                });
            }
            WorkKind::Intercept => {
                self.editors.intercept_scope_input.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                });
                self.last_intercept_paused_id = None;
            }
            WorkKind::Repeater => {
                let req = self.bridge.active_request.clone();
                self.editors.set_repeater_method(&req.method, window, cx);
                self.editors.repeater_url.update(cx, |s, cx| {
                    VariableInput::set_value(s, req.url.clone(), window, cx);
                });
                self.editors.repeater_headers.update(cx, |form, cx| {
                    form.set_from_map(
                        &std::collections::HashMap::from([(
                            "Accept".to_string(),
                            "application/json".to_string(),
                        )]),
                        window,
                        cx,
                    );
                });
                self.editors.repeater_request.update(cx, |e, cx| {
                    e.set_text("", window, cx);
                });
                self.editors.tree_filter.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                });
                self.last_response_text = None;
                self.last_forge_endpoint_id = None;
                self.last_forge_context_id = None;
            }
            WorkKind::Intruder => {
                self.editors.intruder_method.update(cx, |s, cx| {
                    s.set_value("GET", window, cx);
                });
                self.editors.intruder_url.update(cx, |s, cx| {
                    s.set_value("https://example.com/", window, cx);
                });
                self.editors.intruder_headers.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                });
                self.editors.intruder_body.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                });
                self.editors.intruder_payloads.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                });
            }
            WorkKind::Crawler => {
                let target = self.bridge.crawler.config.target_url.clone();
                let exclude = self.bridge.crawler.config.exclude_paths.clone();
                self.editors.crawler_target.update(cx, |s, cx| {
                    s.set_value(target, window, cx);
                });
                self.editors.crawler_exclude.update(cx, |s, cx| {
                    s.set_value(exclude, window, cx);
                });
                self.editors.crawler_log_filter.update(cx, |s, cx| {
                    s.set_value("", window, cx);
                });
            }
            WorkKind::Coder => {
                let _ = Bridge::coder_sync_editors(&self.editors.coder_editors, window, cx);
            }
            WorkKind::SourceControl => {
                let _ = Bridge::git_refresh(cx);
            }
            WorkKind::Agent => {}
            WorkKind::Notes => {
                let content = self
                    .bridge
                    .active_note()
                    .map(|n| n.note.clone())
                    .unwrap_or_default();
                let search = self.bridge.notes_search_query.clone();
                let rename = self
                    .bridge
                    .active_note()
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let library_search = self.bridge.notes_library_search.clone();
                self.editors.notes_editor.update(cx, |e, cx| {
                    e.set_text(content, window, cx);
                });
                self.editors.notes_search.update(cx, |s, cx| {
                    s.set_value(search, window, cx);
                });
                self.editors.notes_rename.update(cx, |s, cx| {
                    s.set_value(rename, window, cx);
                });
                self.editors.notes_library_search.update(cx, |s, cx| {
                    s.set_value(library_search, window, cx);
                });
            }
            WorkKind::ApiMock | WorkKind::ApiOverride => {}
            WorkKind::Jwt => {}
            WorkKind::PortScanner => {}
            WorkKind::Comparer => {}
            WorkKind::FileExplorer => {}
            WorkKind::R2Storage => {}
            WorkKind::Wordlists => {}
            WorkKind::SplitView => {}
            WorkKind::Doctor => {}
            WorkKind::Diagram => {}
            WorkKind::Settings => {}
        }
    }
}

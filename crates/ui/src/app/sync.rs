//! Live state sync: pushes bridge snapshots into the shared editors.

use super::*;

impl HaloApp {
    pub(super) fn sync_response_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = response_editor_text(&self.bridge) else {
            return;
        };
        if self.last_response_text.as_ref() == Some(&text) {
            return;
        }
        self.editors.repeater_response.update(cx, |editor, cx| {
            editor.set_text(text.clone(), window, cx);
        });
        self.last_response_text = Some(text);
    }

    pub(super) fn sync_forge_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let endpoint_id = self.bridge.active_request.endpoint_id.clone();
        if endpoint_id.is_some() && self.last_forge_endpoint_id == endpoint_id {
            // still sync context below
        } else if let Some(ep_id) = endpoint_id.clone() {
            let ep = self
                .bridge
                .endpoints
                .iter()
                .find(|e| e.id == ep_id)
                .cloned();
            if let Some(ep) = ep {
                let headers: std::collections::HashMap<String, String> = ep
                    .headers
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                let body = ep.body.clone().unwrap_or_default();
                let scripts = join_scripts(
                    ep.pre_script.as_deref().unwrap_or(""),
                    ep.test_script.as_deref().unwrap_or(""),
                );

                self.editors.set_repeater_method(&ep.method, window, cx);
                self.editors.repeater_url.update(cx, |s, cx| {
                    VariableInput::set_value(s, ep.url.clone(), window, cx);
                });
                self.editors.repeater_headers.update(cx, |form, cx| {
                    form.set_from_map(&headers, window, cx);
                });
                self.editors.repeater_request.update(cx, |e, cx| {
                    e.set_text(body, window, cx);
                });
                self.editors.repeater_scripts.update(cx, |e, cx| {
                    e.set_text(scripts, window, cx);
                });
                self.last_forge_endpoint_id = Some(ep_id);
            }
        }

        let ctx_id = self.bridge.active_context_id.clone();
        if self.last_forge_context_id != ctx_id {
            if let Some(id) = &ctx_id {
                if let Some(ctx) = self.bridge.contexts.iter().find(|c| &c.id == id) {
                    let vars = ctx.variables.clone();
                    self.editors.repeater_env_vars.update(cx, |form, cx| {
                        form.set_from_json(&vars, window, cx);
                    });
                }
            }
            self.last_forge_context_id = ctx_id;
        }

        let env_fingerprint = {
            let mut s = self.bridge.active_context_id.clone().unwrap_or_default();
            for ctx in &self.bridge.contexts {
                s.push('\0');
                s.push_str(&ctx.id);
                s.push('\0');
                s.push_str(&ctx.name);
            }
            s
        };
        if self.last_forge_env_fingerprint.as_ref() != Some(&env_fingerprint) {
            let pairs: Vec<(&str, &str)> = self
                .bridge
                .contexts
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str()))
                .collect();
            self.editors.sync_repeater_env(
                pairs,
                self.bridge.active_context_id.as_deref(),
                window,
                cx,
            );
            self.last_forge_env_fingerprint = Some(env_fingerprint);
        }

        let collection_fingerprint = {
            let mut s = self.bridge.active_collection_id.clone().unwrap_or_default();
            for col in &self.bridge.collections {
                s.push('\0');
                s.push_str(&col.id);
                s.push('\0');
                s.push_str(&col.name);
            }
            s
        };
        if self.last_forge_collection_fingerprint.as_ref() != Some(&collection_fingerprint) {
            let pairs: Vec<(&str, &str)> = self
                .bridge
                .collections
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str()))
                .collect();
            self.editors.sync_repeater_collection(
                pairs,
                self.bridge.active_collection_id.as_deref(),
                window,
                cx,
            );
            self.last_forge_collection_fingerprint = Some(collection_fingerprint);
        }
    }

    pub(super) fn sync_history_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(detail) = self.bridge.selected_detail.as_ref() else {
            return;
        };
        if self.last_history_detail_id.as_ref() == Some(&detail.id) {
            return;
        }
        let req = format_raw_request(detail);
        let res = format_raw_response(detail);
        self.editors.history_request.update(cx, |editor, cx| {
            editor.set_text(req, window, cx);
        });
        self.editors.history_response.update(cx, |editor, cx| {
            editor.set_text(res, window, cx);
        });
        self.last_history_detail_id = Some(detail.id.clone());
    }

    /// Explorer search inputs feed their filters into the bridge.
    pub(super) fn sync_explorer_searches(&mut self, cx: &mut Context<Self>) {
        let search = self.editors.fe_search.read(cx).value().to_string();
        if search != self.last_fe_search {
            self.last_fe_search = search.clone();
            Bridge::file_explorer_set_search(&search, cx);
        }
        let wl_search = self.editors.wl_search.read(cx).value().to_string();
        if wl_search != self.last_wl_search {
            self.last_wl_search = wl_search.clone();
            Bridge::wordlists_set_search(&wl_search, cx);
        }
        let r2_search = self.editors.r2_search.read(cx).value().to_string();
        if r2_search != self.last_r2_search {
            self.last_r2_search = r2_search.clone();
            Bridge::r2_set_search(&r2_search, cx);
        }
    }

    /// Fill the Settings → R2 form from saved credentials once they arrive.
    ///
    /// The secret never comes back from the backend, so that field starts
    /// empty and a blank value means "keep the saved secret".
    pub(super) fn seed_r2_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.r2_form_seeded {
            return;
        }
        let (loaded, loading, credentials) = {
            let state = Bridge::state(cx);
            let state = state.read(cx);
            (
                state.r2.credentials_loaded,
                state.r2.loading_credentials,
                state.r2.credentials.clone(),
            )
        };
        if !loaded {
            if !loading {
                Bridge::r2_refresh_credentials(cx);
            }
            return;
        }
        let Some(credentials) = credentials else {
            return;
        };
        self.editors.r2_account.update(cx, |input, cx| {
            input.set_value(&credentials.account_id, window, cx);
        });
        self.editors.r2_access_key.update(cx, |input, cx| {
            input.set_value(&credentials.access_key_id, window, cx);
        });
        self.editors.r2_endpoint.update(cx, |input, cx| {
            input.set_value(
                credentials.custom_endpoint_url.as_deref().unwrap_or(""),
                window,
                cx,
            );
        });
        self.r2_form_seeded = true;
    }

    /// Push both Comparer editors into the bridge on every keystroke —
    /// diffing is pure computation and the bridge never writes back.
    pub(super) fn sync_comparer_editors(&mut self, cx: &mut Context<Self>) {
        let text_a = self.editors.cmp_a.read(cx).text(cx);
        if text_a != self.last_comparer_a {
            self.last_comparer_a = text_a.clone();
            Bridge::comparer_set_a(&text_a, cx);
        }
        let text_b = self.editors.cmp_b.read(cx).text(cx);
        if text_b != self.last_comparer_b {
            self.last_comparer_b = text_b.clone();
            Bridge::comparer_set_b(&text_b, cx);
        }
    }

    /// Live-decode the JWT token editor: push its text into the bridge on
    /// every keystroke. Decoding is pure computation, so no debounce is
    /// needed; the bridge never writes back into this editor.
    pub(super) fn sync_jwt_decode(&mut self, cx: &mut Context<Self>) {
        let text = self.editors.jwt_token.read(cx).text(cx);
        if text == self.last_jwt_token {
            return;
        }
        self.last_jwt_token = text.clone();
        Bridge::jwt_set_token(&text, cx);
    }

    /// Hydrate chaos inputs from the selected route once per selection.
    pub(super) fn sync_mock_chaos(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chaos = self.bridge.mock.route_chaos();
        if self.last_chaos_route != self.bridge.mock.selected_route_id {
            self.last_chaos_route = self.bridge.mock.selected_route_id.clone();
            let rate = chaos
                .error_rate_bp
                .map(|bp| format!("{:.2}", bp as f64 / 10_000.0))
                .unwrap_or_default();
            self.editors.mock_chaos_fixed.update(cx, |input, cx| {
                input.set_value(chaos.latency_fixed.unwrap_or(200).to_string(), window, cx);
            });
            self.editors.mock_chaos_min.update(cx, |input, cx| {
                input.set_value(chaos.latency_min.unwrap_or(100).to_string(), window, cx);
            });
            self.editors.mock_chaos_max.update(cx, |input, cx| {
                input.set_value(chaos.latency_max.unwrap_or(500).to_string(), window, cx);
            });
            self.editors.mock_chaos_rate.update(cx, |input, cx| {
                input.set_value(rate, window, cx);
            });
            self.editors.mock_chaos_status.update(cx, |input, cx| {
                input.set_value(chaos.error_status.unwrap_or(500).to_string(), window, cx);
            });
        }
    }

    pub(super) fn sync_mock_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.bridge.mock.selected_route_id.clone();
        if self.last_mock_route_id == selected {
            return;
        }
        self.last_mock_route_id = selected.clone();

        let Some(route) = selected
            .as_deref()
            .and_then(|id| self.bridge.mock.routes.iter().find(|r| r.id == id))
            .cloned()
        else {
            return;
        };

        let is_override = self
            .bridge
            .active_work_kind()
            .is_some_and(|kind| kind == WorkKind::ApiOverride);

        let (method, path, status, body) = if is_override {
            (
                &mut self.editors.override_method,
                &mut self.editors.override_path,
                &mut self.editors.override_status,
                &mut self.editors.override_body,
            )
        } else {
            (
                &mut self.editors.mock_method,
                &mut self.editors.mock_path,
                &mut self.editors.mock_status,
                &mut self.editors.mock_body,
            )
        };

        method.update(cx, |input, cx| {
            input.set_value(&route.method, window, cx);
        });
        path.update(cx, |input, cx| {
            input.set_value(&route.path, window, cx);
        });
        status.update(cx, |input, cx| {
            input.set_value(&route.status_code.to_string(), window, cx);
        });
        body.update(cx, |editor, cx| {
            editor.set_text(route.response_body.clone(), window, cx);
        });
    }

    pub(super) fn sync_intercept_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected_id = self.bridge.selected_paused_id.clone();
        if self.last_intercept_paused_id == selected_id {
            return;
        }

        if let Some(ref sel_id) = selected_id {
            if let Some(row) = self.bridge.paused.iter().find(|p| &p.id == sel_id) {
                let text = if row.has_response {
                    row.raw_response
                        .clone()
                        .unwrap_or_else(|| row.raw_request.clone())
                } else {
                    row.raw_request.clone()
                };
                self.editors.intercept_editor.update(cx, |editor, cx| {
                    editor.set_text(text, window, cx);
                });
                self.last_intercept_paused_id = Some(sel_id.clone());
            }
        } else if self.bridge.paused.is_empty() {
            self.editors.intercept_editor.update(cx, |editor, cx| {
                editor.set_text(
                    "// No intercepted request or response currently held.",
                    window,
                    cx,
                );
            });
            self.last_intercept_paused_id = None;
        }
    }
}

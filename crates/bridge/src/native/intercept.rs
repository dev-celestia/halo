//! Intercept (pause / forward / drop) and TLS-scope accessors.

use super::dto::*;
use super::*;

impl BackendIntercept for NativeBackend {
    fn intercept_enabled(&self) -> BridgeResult<bool> {
        Ok(self.runtime.proxy_state.get_mode() == InterceptMode::Enabled)
    }

    fn set_intercept_enabled(&self, enabled: bool) -> BridgeResult<()> {
        self.runtime.proxy_state.set_mode(if enabled {
            InterceptMode::Enabled
        } else {
            InterceptMode::Disabled
        });
        Ok(())
    }

    fn intercept_paused(&self) -> BridgeResult<Vec<PausedRow>> {
        Ok(self
            .runtime
            .proxy_state
            .get_all_paused()
            .into_iter()
            .map(|p| {
                let (host, path) = host_path_from_uri(&p.request.uri);
                let raw_request = crate::http_format::format_raw_request_parts(
                    &p.request.method,
                    &p.request.uri,
                    &p.request.http_version,
                    &p.request.headers,
                    &p.request.body,
                );
                let (status_code, status_text, raw_response) = match &p.response {
                    Some(res) => (
                        Some(res.status_code),
                        Some(res.status_text.clone()),
                        Some(crate::http_format::format_raw_response_parts(
                            res.status_code,
                            &res.status_text,
                            &res.http_version,
                            &res.headers,
                            &res.body,
                        )),
                    ),
                    None => (None, None, None),
                };
                PausedRow {
                    id: p.id.to_string(),
                    method: p.request.method.clone(),
                    url: p.request.uri.clone(),
                    host,
                    path,
                    has_response: p.response.is_some(),
                    status_code,
                    status_text,
                    raw_request,
                    raw_response,
                    timestamp: p.timestamp.to_rfc3339(),
                    tab_id: p.tab_id.clone(),
                }
            })
            .collect())
    }

    fn intercept_forward(&self, id: &str) -> BridgeResult<()> {
        let uuid = Uuid::parse_str(id).map_err(|e| BridgeError::message(e.to_string()))?;
        let paused = self.runtime.proxy_state.get_paused_request(&uuid);
        let Some(paused) = paused else {
            return Err(BridgeError::message("Paused request not found"));
        };
        let ok = if paused.response.is_some() {
            self.runtime
                .proxy_state
                .forward_paused_response(&uuid, None)
        } else {
            self.runtime
                .proxy_state
                .forward_paused_request(&uuid, None, false)
        };
        if ok {
            Ok(())
        } else {
            Err(BridgeError::message("Paused request not found"))
        }
    }

    fn intercept_drop(&self, id: &str) -> BridgeResult<()> {
        let uuid = Uuid::parse_str(id).map_err(|e| BridgeError::message(e.to_string()))?;
        if self.runtime.proxy_state.drop_paused_request(&uuid) {
            Ok(())
        } else {
            Err(BridgeError::message("Paused request not found"))
        }
    }

    fn intercept_forward_all(&self) -> BridgeResult<()> {
        let ids: Vec<_> = self
            .runtime
            .proxy_state
            .get_all_paused()
            .into_iter()
            .map(|p| p.id)
            .collect();
        for id in ids {
            let _ = self.intercept_forward(&id.to_string());
        }
        Ok(())
    }

    fn intercept_drop_all(&self) -> BridgeResult<()> {
        let ids: Vec<_> = self
            .runtime
            .proxy_state
            .get_all_paused()
            .into_iter()
            .map(|p| p.id)
            .collect();
        for id in ids {
            let _ = self.runtime.proxy_state.drop_paused_request(&id);
        }
        Ok(())
    }

    fn intercept_scope(&self) -> BridgeResult<Vec<String>> {
        Ok(self.runtime.proxy_state.get_intercept_scope().1)
    }

    fn intercept_set_scope(&self, tab_id: &str, patterns: &[String]) -> BridgeResult<()> {
        self.runtime
            .proxy_state
            .set_intercept_scope(tab_id.to_string(), patterns.to_vec());
        Ok(())
    }

    fn intercept_add_scope(&self, pattern: &str) -> BridgeResult<Vec<String>> {
        Ok(self.runtime.proxy_state.add_intercept_pattern(pattern))
    }

    fn intercept_remove_scope(&self, pattern: &str) -> BridgeResult<Vec<String>> {
        Ok(self.runtime.proxy_state.remove_intercept_pattern(pattern))
    }

    fn intercept_forward_tab(&self, tab_id: &str) -> BridgeResult<usize> {
        Ok(self.runtime.proxy_state.forward_paused_by_tab(tab_id))
    }

    fn intercept_forward_modified(
        &self,
        id: &str,
        raw_text: &str,
        intercept_response: bool,
    ) -> BridgeResult<()> {
        let uuid = Uuid::parse_str(id).map_err(|e| BridgeError::message(e.to_string()))?;
        let paused = self.runtime.proxy_state.get_paused_request(&uuid);
        let Some(paused) = paused else {
            return Err(BridgeError::message("Paused request not found"));
        };

        if paused.response.is_some() {
            let (status, status_text, headers, body) =
                crate::http_format::parse_raw_response(raw_text);
            let forward_response = halo_backend::commands::intercept::InterceptForwardResponse {
                status,
                status_text,
                headers,
                body,
            };
            halo_backend::commands::intercept::forward_intercepted_response_state(
                &self.runtime.proxy_state,
                id,
                Some(forward_response),
            )
            .map_err(BridgeError::message)?;
        } else {
            let (method, url, headers, body) =
                crate::http_format::parse_raw_request(raw_text, &paused.request.uri);
            let forward_request = halo_backend::commands::intercept::InterceptForwardRequest {
                method,
                url,
                headers,
                body,
            };
            halo_backend::commands::intercept::forward_intercepted_request_state(
                &self.runtime.proxy_state,
                id,
                Some(forward_request),
                Some(intercept_response),
            )
            .map_err(BridgeError::message)?;
        }
        Ok(())
    }

    fn intercept_open_browser(&self) -> BridgeResult<()> {
        let port = halo_backend::proxy::active_proxy_port().unwrap_or(8888);
        self.tokio
            .block_on(halo_backend::commands::intercept::open_intercept_browser(
                self.runtime.app.clone(),
                port,
            ))
            .map_err(BridgeError::message)
    }

    fn intercept_trust_ca(&self) -> BridgeResult<String> {
        self.tokio
            .block_on(halo_backend::commands::intercept::trust_intercept_ca(
                self.runtime.app.clone(),
            ))
            .map_err(BridgeError::message)
    }

    fn intercept_add_host_tab(&self, host: &str) -> BridgeResult<()> {
        let host = host.trim().to_lowercase();
        if host.is_empty() {
            return Err(BridgeError::message("Host is empty"));
        }
        let tab_id = format!("host-{host}");
        self.runtime
            .proxy_state
            .set_intercept_scope(tab_id, vec![host.clone()]);
        self.runtime.proxy_state.set_mode(InterceptMode::Enabled);
        Ok(())
    }
}

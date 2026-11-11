//! API Mock + API Override bridge commands: reload, CRUD, server control.
//!
//! Both pages read from the shared [`MockForgeView`]; "local" routes belong to
//! the API Mock server, everything else is an override rule against proxied
//! hosts.

use super::*;

pub const LOCAL_MOCK_DOMAIN_ID: &str = "local_mock_server";
#[allow(dead_code)]
pub const DEFAULT_MOCK_PORT: u16 = 4000;

/// Render `body`'s `{{param}}` tags against `test_path` matched by `pattern`.
pub fn simulate_template(pattern: &str, test_path: &str, body: &str) -> String {
    let pattern_path = match pattern.split_once('?') {
        Some((p, _)) => p,
        None => pattern,
    };
    let (test_path_only, query_str) = match test_path.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (test_path, None),
    };
    let mut query = std::collections::HashMap::new();
    if let Some(q) = query_str {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            query.insert(k.to_string(), v.to_string());
        }
    }
    // Absolute path: this module is also named `mock_forge`, which shadows the crate.
    let params = ::mock_forge::extract_path_params(pattern_path, test_path_only);
    let matched = ::mock_forge::path_matches(pattern_path, test_path_only);
    if !matched {
        return format!("// Request path does not match route pattern\n{body}");
    }
    ::mock_forge::render_template_string(body, &params, &query, test_path, "GET")
}

impl Bridge {
    /// Reload domains / routes / logs / server status from the backend.
    pub fn load_mock_forge(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let domains = backend.mock_domains().unwrap_or_default();
        let routes = backend.mock_routes().unwrap_or_default();
        let logs = backend.mock_logs().unwrap_or_default();
        let server = backend.mock_server_status().unwrap_or_default();
        Self::state(cx).update(cx, |s| {
            s.mock.domains = domains;
            s.mock.routes = routes;
            s.mock.logs = logs;
            s.mock.server = server;
        });
        Ok(())
    }

    /// Load-on-open for a work tab: open (or focus) the tab and refresh data.
    pub fn open_mock_page(kind: WorkKind, cx: &mut App) -> BridgeResult<()> {
        Self::focus_or_open_work_tab(kind, cx)?;
        Self::load_mock_forge(cx)
    }

    pub fn mock_select_route(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.mock.selected_route_id = id;
            s.mock.selected_log_id = None;
        });
    }

    pub fn mock_select_log(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.mock.selected_log_id = id;
        });
    }

    pub fn mock_add_domain(hostname: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let domain = backend.mock_add_domain(hostname, false).map_err(|e| e)?;
        Self::state(cx).update(cx, |s| {
            s.mock.domains.push(domain);
            s.apply_ok_msg(format!("Added host {hostname}"));
        });
        Ok(())
    }

    pub fn mock_toggle_domain(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.mock_toggle_domain(id)?;
        Self::load_mock_forge(cx)
    }

    pub fn mock_delete_domain(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.mock_delete_domain(id)?;
        Self::state(cx).update(cx, |s| {
            s.mock.domains.retain(|d| d.id != id);
            s.mock.routes.retain(|r| r.domain_id != id);
            if s.mock
                .selected_route_id
                .as_deref()
                .is_some_and(|sel| s.mock.routes.iter().all(|r| r.id != sel))
            {
                s.mock.selected_route_id = None;
            }
            s.apply_ok_msg("Deleted host and its rules".to_string());
        });
        Ok(())
    }

    /// Create an endpoint on the local mock server (API Mock page).
    pub fn mock_add_local_route(
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::mock_add_route(
            LOCAL_MOCK_DOMAIN_ID,
            method,
            path,
            status_code,
            response_body,
            cx,
        )
    }

    /// Create an override rule against a captured host (API Override page).
    pub fn mock_add_override_route(
        domain_id: &str,
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::mock_add_route(domain_id, method, path, status_code, response_body, cx)
    }

    fn mock_add_route(
        domain_id: &str,
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if path.trim().is_empty() {
            return Err(BridgeError::message("Path is empty"));
        }
        let backend = Self::backend(cx)?;
        let chaos = Self::state(cx).read(cx).mock.route_chaos();
        let route =
            backend.mock_add_route(domain_id, method, path, status_code, response_body, chaos)?;
        Self::state(cx).update(cx, |s| {
            s.mock.routes.push(route.clone());
            s.mock.selected_route_id = Some(route.id);
            s.apply_ok_msg(format!("Created {method} {path}"));
        });
        Ok(())
    }

    /// Update the chaos config that rides along with the selected route
    /// (persisted on Save).
    pub fn mock_set_chaos(patch: crate::types::MockChaosDto, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let Some(route) = s
                .mock
                .routes
                .iter_mut()
                .find(|r| Some(r.id.as_str()) == s.mock.selected_route_id.as_deref())
            else {
                return;
            };
            route.chaos = patch;
        });
    }

    /// Toggle the CORS flag used on the next server start.
    pub fn mock_set_cors(cors: bool, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.mock.cors = cors;
        });
    }

    /// Simulate: render the selected route's template against a test path.
    pub fn mock_simulate(test_path: &str, cx: &mut App) {
        let output = Self::state(cx)
            .read(cx)
            .mock
            .selected_route()
            .map(|route| simulate_template(&route.path, test_path, &route.response_body));
        Self::state(cx).update(cx, |s| {
            s.mock.simulate_path = test_path.to_string();
            s.mock.simulate_output = Some(
                output
                    .unwrap_or_else(|| "Select an endpoint to simulate its template.".to_string()),
            );
        });
    }

    /// Save editor contents onto the selected route.
    pub fn mock_update_selected_route(
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let selected = Self::state(cx).read(cx).mock.selected_route_id.clone();
        let Some(id) = selected else {
            return Err(BridgeError::message("No route selected"));
        };
        if path.trim().is_empty() {
            return Err(BridgeError::message("Path is empty"));
        }
        let backend = Self::backend(cx)?;
        let chaos = Self::state(cx).read(cx).mock.route_chaos();
        backend.mock_update_route(&id, method, path, status_code, response_body, chaos)?;
        Self::load_mock_forge(cx)?;
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg("Route saved".to_string());
        });
        Ok(())
    }

    pub fn mock_set_route_enabled(id: &str, enabled: bool, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.mock_set_route_enabled(id, enabled)?;
        Self::state(cx).update(cx, |s| {
            if let Some(route) = s.mock.routes.iter_mut().find(|r| r.id == id) {
                route.enabled = enabled;
            }
        });
        Ok(())
    }

    pub fn mock_delete_route(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.mock_delete_route(id)?;
        Self::state(cx).update(cx, |s| {
            s.mock.routes.retain(|r| r.id != id);
            if s.mock.selected_route_id.as_deref() == Some(id) {
                s.mock.selected_route_id = None;
            }
            s.apply_ok_msg("Deleted route".to_string());
        });
        Ok(())
    }

    pub fn mock_clear_logs(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.mock_clear_logs()?;
        Self::state(cx).update(cx, |s| {
            s.mock.logs.clear();
            s.mock.selected_log_id = None;
        });
        Ok(())
    }

    pub fn mock_start_server(port: u16, cors: bool, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let status = backend.mock_server_start(port, cors)?;
        Self::state(cx).update(cx, |s| {
            s.mock.server = status.clone();
            s.apply_ok_msg(format!(
                "Mock API server started on {}",
                status.url.unwrap_or_default()
            ));
        });
        Ok(())
    }

    pub fn mock_stop_server(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let status = backend.mock_server_stop()?;
        Self::state(cx).update(cx, |s| {
            s.mock.server = status;
            s.apply_ok_msg("Mock API server stopped".to_string());
        });
        Ok(())
    }

    /// Handle a `mock-forge-log` event from the backend.
    pub(super) fn apply_mock_log(state: &mut BridgeState, log: MockLogDto) {
        state.mock.logs.insert(0, log);
        if state.mock.logs.len() > 200 {
            state.mock.logs.truncate(200);
        }
    }
}

use std::collections::HashMap;

use halo_core::tauri::{AppHandle, Emitter, Manager};
use halo_core::types::{ChaosConfig, MockDomain, MockRoute, RequestLog};
use halo_storage::HistoryBridge;
use parking_lot::Mutex;

use crate::matcher::normalize_hostname;

pub const MOCK_LOG_LIMIT: usize = 200;

pub struct MockForgeState {
    pub domains: Mutex<Vec<MockDomain>>,
    pub routes: Mutex<Vec<MockRoute>>,
    pub logs: Mutex<Vec<RequestLog>>,
}

impl Default for MockForgeState {
    fn default() -> Self {
        Self::new()
    }
}

impl MockForgeState {
    pub fn new() -> Self {
        Self {
            domains: Mutex::new(Vec::new()),
            routes: Mutex::new(Vec::new()),
            logs: Mutex::new(Vec::new()),
        }
    }
}

pub fn load_mock_forge_from_db(
    state: &MockForgeState,
    db: &halo_storage::Database,
) -> Result<(), String> {
    let domains = db.get_mock_domains().map_err(|e| e.to_string())?;
    let routes = db.get_mock_routes().map_err(|e| e.to_string())?;
    *state.domains.lock() = domains;
    *state.routes.lock() = routes;
    Ok(())
}

pub fn mock_state(app: &AppHandle) -> Option<halo_core::tauri::State<'_, MockForgeState>> {
    app.try_state::<MockForgeState>()
}

fn with_mock_state<F, R>(app: &AppHandle, f: F) -> Option<R>
where
    F: FnOnce(&MockForgeState) -> R,
{
    mock_state(app).map(|s| f(&s))
}

pub fn mock_forge_get_domains(app: &AppHandle) -> Result<Vec<MockDomain>, String> {
    mock_state(app)
        .map(|s| s.domains.lock().clone())
        .ok_or_else(|| "MockForge state is not managed".to_string())
}

pub fn mock_forge_get_routes(app: &AppHandle) -> Result<Vec<MockRoute>, String> {
    mock_state(app)
        .map(|s| s.routes.lock().clone())
        .ok_or_else(|| "MockForge state is not managed".to_string())
}

pub fn mock_forge_get_logs(app: &AppHandle) -> Vec<RequestLog> {
    mock_state(app)
        .map(|s| s.logs.lock().clone())
        .unwrap_or_default()
}

pub fn mock_forge_clear_logs(app: &AppHandle) {
    if let Some(s) = mock_state(app) {
        s.logs.lock().clear();
    }
}

fn persist_domain(app: &AppHandle, domain: &MockDomain) -> Result<(), String> {
    let history = app.try_state::<HistoryBridge>();
    let Some(history) = history else {
        return Ok(());
    };
    history
        .database()
        .insert_mock_domain(domain)
        .map_err(|e| e.to_string())
}

pub fn mock_forge_add_domain(
    app: &AppHandle,
    hostname: &str,
    ssl: bool,
) -> Result<MockDomain, String> {
    let clean = normalize_hostname(hostname);
    if clean.is_empty() {
        return Err("Hostname is empty".to_string());
    }
    let state = mock_state(app).ok_or_else(|| "MockForge state is not managed".to_string())?;
    let domain = MockDomain {
        id: uuid::Uuid::new_v4().to_string(),
        hostname: clean,
        ssl,
        status: "active".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    persist_domain(app, &domain)?;
    {
        let mut domains = state.domains.lock();
        if domains.iter().any(|d| d.hostname == domain.hostname) {
            return Err(format!("Host {} is already registered", domain.hostname));
        }
        domains.push(domain.clone());
    }
    Ok(domain)
}

pub fn mock_forge_toggle_domain(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = mock_state(app).ok_or_else(|| "MockForge state is not managed".to_string())?;
    {
        let mut domains = state.domains.lock();
        let Some(domain) = domains.iter_mut().find(|d| d.id == id) else {
            return Err("Unknown domain".to_string());
        };
        domain.status = if domain.status == "active" {
            "inactive".to_string()
        } else {
            "active".to_string()
        };
    }

    let history = app.try_state::<HistoryBridge>();
    if let Some(history) = history {
        history
            .database()
            .toggle_mock_domain(id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn mock_forge_delete_domain(app: &AppHandle, id: &str) -> Result<(), String> {
    with_mock_state(app, |s| {
        s.domains.lock().retain(|d| d.id != id);
        s.routes.lock().retain(|r| r.domain_id != id);
    });
    if let Some(history) = app.try_state::<HistoryBridge>() {
        let db = history.database();
        db.delete_mock_domain(id).map_err(|e| e.to_string())?;
        for route in db.get_mock_routes().map_err(|e| e.to_string())? {
            if route.domain_id == id {
                db.delete_mock_route(&route.id).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn persist_route(app: &AppHandle, route: &MockRoute) -> Result<(), String> {
    let Some(history) = app.try_state::<HistoryBridge>() else {
        return Ok(());
    };
    history
        .database()
        .upsert_mock_route(route)
        .map_err(|e| e.to_string())
}

pub fn mock_forge_add_route(app: &AppHandle, mut route: MockRoute) -> Result<MockRoute, String> {
    if route.path.trim().is_empty() {
        return Err("Path is empty".to_string());
    }
    if !route.path.trim().starts_with('/')
        && !route.path.trim().starts_with("http://")
        && !route.path.trim().starts_with("https://")
    {
        route.path = format!("/{}", route.path.trim());
    }
    route.id = uuid::Uuid::new_v4().to_string();
    persist_route(app, &route)?;
    with_mock_state(app, |s| {
        s.routes.lock().push(route.clone());
    });
    Ok(route)
}

#[derive(Debug, Clone, Default)]
pub struct MockRoutePatch {
    pub domain_id: Option<String>,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status_code: Option<u16>,
    pub response_body: Option<String>,
    pub response_headers: Option<HashMap<String, String>>,
    pub enabled: Option<bool>,
    pub chaos: Option<ChaosConfig>,
}

pub fn mock_forge_update_route(
    app: &AppHandle,
    id: &str,
    patch: MockRoutePatch,
) -> Result<(), String> {
    let updated = with_mock_state(app, |s| {
        let mut routes = s.routes.lock();
        let Some(existing) = routes.iter().find(|r| r.id == id) else {
            return Err("Unknown route".to_string());
        };
        let mut merged = existing.clone();
        if let Some(method) = patch.method {
            merged.method = method;
        }
        if let Some(path) = patch.path {
            merged.path = path;
        }
        if let Some(status) = patch.status_code {
            merged.status_code = status;
        }
        if let Some(body) = patch.response_body {
            merged.response_body = body;
        }
        if let Some(headers) = patch.response_headers {
            merged.response_headers = headers;
        }
        if let Some(enabled) = patch.enabled {
            merged.enabled = enabled;
        }
        if let Some(domain_id) = patch.domain_id {
            merged.domain_id = domain_id;
        }
        if let Some(chaos) = patch.chaos {
            merged.chaos = chaos;
        }
        *routes.iter_mut().find(|r| r.id == id).unwrap() = merged.clone();
        Ok(merged)
    })
    .transpose()?;
    if let Some(route) = updated {
        persist_route(app, &route)?;
    }
    Ok(())
}

pub fn mock_forge_delete_route(app: &AppHandle, id: &str) -> Result<(), String> {
    with_mock_state(app, |s| {
        s.routes.lock().retain(|r| r.id != id);
    });
    if let Some(history) = app.try_state::<HistoryBridge>() {
        history
            .database()
            .delete_mock_route(id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn mock_forge_record_log(app: &AppHandle, log: RequestLog) {
    with_mock_state(app, |s| {
        let mut logs = s.logs.lock();
        logs.insert(0, log.clone());
        if logs.len() > MOCK_LOG_LIMIT {
            logs.truncate(MOCK_LOG_LIMIT);
        }
    });
    if let Ok(value) = serde_json::to_value(&log) {
        let _ = app.emit("mock-forge-log", &value);
    }
}

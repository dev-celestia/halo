//! Mock-server domain, route, chaos, and log DTOs plus the shared view.

use serde::{Deserialize, Serialize};

// ── API Mock / API Override (MockForge) ────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockDomainDto {
    pub id: String,
    pub hostname: String,
    pub ssl: bool,
    /// `"active"` | `"inactive"`.
    pub status: String,
    pub created_at: String,
}

/// Chaos injection config carried on every mock route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MockChaosDto {
    /// `none` | `fixed` | `random`.
    pub latency_mode: String,
    pub latency_fixed: Option<u64>,
    pub latency_min: Option<u64>,
    pub latency_max: Option<u64>,
    /// 0.0–1.0; stored as basis points to keep the DTO `Eq`.
    pub error_rate_bp: Option<u32>,
    pub error_status: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockRouteDto {
    pub id: String,
    pub domain_id: String,
    pub method: String,
    /// Path (`/api/users/:id`) or full URL (`https://host/path`) for override rules.
    pub path: String,
    pub status_code: u16,
    pub response_body: String,
    pub response_headers: std::collections::BTreeMap<String, String>,
    pub enabled: bool,
    pub chaos: MockChaosDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockLogDto {
    pub id: String,
    pub route_id: Option<String>,
    pub method: String,
    pub path: String,
    pub status_code: u16,
    pub latency_ms: u64,
    pub timestamp: String,
    /// `"mock_server"` | `"response_override"`.
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MockServerStatusDto {
    pub running: bool,
    pub port: u16,
    pub url: Option<String>,
}

/// Everything the API Mock and API Override pages render.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockForgeView {
    pub domains: Vec<MockDomainDto>,
    pub routes: Vec<MockRouteDto>,
    pub logs: Vec<MockLogDto>,
    pub server: MockServerStatusDto,
    /// CORS toggle for the next server start (toolbar switch).
    pub cors: bool,
    /// Simulate bar: test path + rendered template preview.
    pub simulate_path: String,
    pub simulate_output: Option<String>,
    pub selected_route_id: Option<String>,
    pub selected_log_id: Option<String>,
}

impl Default for MockForgeView {
    fn default() -> Self {
        Self {
            domains: Vec::new(),
            routes: Vec::new(),
            logs: Vec::new(),
            server: MockServerStatusDto::default(),
            cors: true,
            simulate_path: String::new(),
            simulate_output: None,
            selected_route_id: None,
            selected_log_id: None,
        }
    }
}

impl MockForgeView {
    /// Routes served by the local mock server (API Mock page).
    pub fn local_routes(&self) -> Vec<MockRouteDto> {
        self.routes
            .iter()
            .filter(|r| {
                r.domain_id == "local_mock_server"
                    || r.domain_id.is_empty()
                    || r.domain_id == "localhost"
            })
            .cloned()
            .collect()
    }

    /// Domains captured from proxied traffic (API Override page).
    pub fn override_domains(&self) -> Vec<MockDomainDto> {
        self.domains
            .iter()
            .filter(|d| d.id != "local_mock_server")
            .cloned()
            .collect()
    }

    /// Override rules (API Override page).
    pub fn override_routes(&self) -> Vec<MockRouteDto> {
        self.routes
            .iter()
            .filter(|r| {
                r.domain_id != "local_mock_server"
                    && !r.domain_id.is_empty()
                    && r.domain_id != "localhost"
            })
            .cloned()
            .collect()
    }

    pub fn selected_route(&self) -> Option<&MockRouteDto> {
        self.selected_route_id
            .as_deref()
            .and_then(|id| self.routes.iter().find(|r| r.id == id))
    }

    /// Chaos config of the selected route (defaults for a new route).
    pub fn route_chaos(&self) -> MockChaosDto {
        self.selected_route()
            .map(|r| r.chaos.clone())
            .unwrap_or_default()
    }

    pub fn selected_log(&self) -> Option<&MockLogDto> {
        self.selected_log_id
            .as_deref()
            .and_then(|id| self.logs.iter().find(|l| l.id == id))
    }
}

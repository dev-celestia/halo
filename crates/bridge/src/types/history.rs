//! HTTP history, session, intercept-snapshot, and detail-panel DTOs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Live proxy runtime snapshot (replaces Tauri `proxy-status-changed`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub tls_port: Option<u16>,
    pub default_port: u16,
    pub connections: usize,
}

impl Default for ProxyStatus {
    fn default() -> Self {
        Self {
            running: false,
            port: None,
            tls_port: None,
            default_port: 8888,
            connections: 0,
        }
    }
}

/// Lightweight history row for the traffic table (maps from `ProxyLogSummary`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HttpLogRow {
    pub id: String,
    pub session_id: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub content_type: Option<String>,
    pub host: Option<String>,
    pub timestamp: String,
    pub request_size: usize,
    pub response_size: usize,
    pub user_agent: Option<String>,
    pub server_addr: String,
}

/// Client-side / query filter for history (maps to backend `ProxyFilter`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HistoryFilter {
    pub search: Option<String>,
    pub methods: Vec<String>,
    /// Status class filters: 2, 3, 4, 5 (xx ranges). Empty = all.
    pub status_classes: Vec<u16>,
    pub hide_304: bool,
    pub hide_images: bool,
    pub only_api: bool,
    pub errors_only: bool,
    pub session_id: Option<String>,
}

impl HistoryFilter {
    pub fn is_active(&self) -> bool {
        self.search.as_ref().is_some_and(|s| !s.trim().is_empty())
            || !self.methods.is_empty()
            || !self.status_classes.is_empty()
            || self.hide_304
            || self.hide_images
            || self.only_api
            || self.errors_only
            || self.session_id.is_some()
    }
}

/// Full request/response for the detail inspector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HttpLogDetail {
    pub id: String,
    pub timestamp: String,
    pub client_addr: String,
    pub server_addr: String,
    pub method: String,
    pub url: String,
    pub req_http_version: String,
    pub req_headers: HashMap<String, String>,
    pub req_body: String,
    pub req_body_bytes: usize,
    pub res_status: Option<u16>,
    pub res_status_text: Option<String>,
    pub res_http_version: Option<String>,
    pub res_headers: HashMap<String, String>,
    pub res_body: String,
    pub res_body_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HttpSessionDto {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub request_count: usize,
    pub capture_mode: String,
}

/// Proxy intercept master switch plus the active work tab's host scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InterceptSnapshot {
    pub enabled: bool,
    pub paused_count: usize,
    /// Host patterns for the intercept work tab currently projected into the UI.
    pub capture_patterns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PausedRow {
    pub id: String,
    pub method: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub has_response: bool,
    pub status_code: Option<u16>,
    pub status_text: Option<String>,
    pub raw_request: String,
    pub raw_response: Option<String>,
    pub timestamp: String,
    /// Intercept tab that captured this request (`None` = unscoped / legacy).
    pub tab_id: Option<String>,
}

/// Request vs response pane focus in the detail inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DetailSide {
    #[default]
    Request,
    Response,
}

/// Which pane is active in the detail inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DetailTab {
    #[default]
    Raw,
    Headers,
    Body,
    Params,
    Cookies,
}

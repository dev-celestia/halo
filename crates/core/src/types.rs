use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequest {
    pub method: String,
    pub uri: String,
    pub http_version: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    #[serde(default)]
    pub content_decoded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResponse {
    pub status_code: u16,
    pub status_text: String,
    pub http_version: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    #[serde(default)]
    pub content_decoded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRecord {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub request: ProxyRequest,
    pub response: Option<ProxyResponse>,
    pub client_addr: String,
    pub server_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum WebSocketConnectionState {
    Open,
    #[default]
    Closed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WebSocketMessageDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WebSocketMessageType {
    Text,
    Binary,
    Ping,
    Pong,
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConnectionRecord {
    pub id: Uuid,
    #[serde(default)]
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub url: String,
    pub host: String,
    pub path: String,
    pub handshake_request_headers: HashMap<String, String>,
    pub handshake_response_status: Option<u16>,
    pub handshake_response_headers: HashMap<String, String>,
    pub client_addr: String,
    pub server_addr: String,
    pub state: WebSocketConnectionState,
    pub message_count: u32,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessageRecord {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub direction: WebSocketMessageDirection,
    pub message_type: WebSocketMessageType,
    pub payload: Vec<u8>,
    pub payload_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebSocketFilter {
    pub search: Option<String>,
    pub scope: Option<Vec<String>>,
    pub states: Option<Vec<String>>,
    pub session_id: Option<String>,
}

impl WebSocketFilter {
    pub fn host_matches_scope(&self, host: &str) -> bool {
        let Some(ref scope) = self.scope else {
            return true;
        };
        if scope.is_empty() {
            return true;
        }
        for pattern in scope {
            if let Some(domain) = pattern.strip_prefix("*.") {
                if host.ends_with(domain) {
                    return true;
                }
            } else if host.contains(pattern.as_str()) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum InterceptMode {
    #[default]
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptStatus {
    pub mode: InterceptMode,
    pub paused_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PausedRequest {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub client_addr: String,
    pub server_addr: String,
    #[serde(default)]
    pub tab_id: Option<String>,
    pub request: ProxyRequest,
    pub response: Option<ProxyResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyFilter {
    pub search: Option<String>,
    pub path: Option<String>,
    pub methods: Option<Vec<String>>,
    pub status_codes: Option<Vec<u16>>,
    pub scope: Option<Vec<String>>,
    pub session_id: Option<String>,
}

impl ProxyFilter {
    pub fn record_matches_scope(&self, record: &ProxyRecord) -> bool {
        let Some(ref scope) = self.scope else {
            return true;
        };
        if scope.is_empty() {
            return true;
        }

        let mut candidate_domains = Vec::new();

        // 1. Origin header (for CORS requests initiated by app JS)
        if let Some(origin) = record
            .request
            .headers
            .get("origin")
            .or_else(|| record.request.headers.get("Origin"))
        {
            let clean = origin
                .trim_start_matches("http://")
                .trim_start_matches("https://");
            let dom = clean
                .split('/')
                .next()
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("")
                .trim();
            if !dom.is_empty() && dom != "null" && dom != "opaque" {
                candidate_domains.push(dom.to_lowercase());
            }
        }

        // 2. Request URI host
        if record.request.uri.contains("://") {
            if let Some(after_scheme) = record.request.uri.split("://").nth(1) {
                let dom = after_scheme
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .trim();
                if !dom.is_empty() {
                    candidate_domains.push(dom.to_lowercase());
                }
            }
        } else if !record.request.uri.starts_with('/') {
            let dom = record
                .request
                .uri
                .split('/')
                .next()
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("")
                .trim();
            if !dom.is_empty() {
                candidate_domains.push(dom.to_lowercase());
            }
        }

        // 4. Host header
        if let Some(host_hdr) = record
            .request
            .headers
            .get("host")
            .or_else(|| record.request.headers.get("Host"))
        {
            let dom = host_hdr.split(':').next().unwrap_or("").trim();
            if !dom.is_empty() {
                candidate_domains.push(dom.to_lowercase());
            }
        }

        // 5. Server addr
        if !record.server_addr.is_empty() && !record.server_addr.starts_with('/') {
            let dom = record.server_addr.split(':').next().unwrap_or("").trim();
            if !dom.is_empty() {
                candidate_domains.push(dom.to_lowercase());
            }
        }

        for pattern in scope {
            let pattern_lower = pattern.trim().to_lowercase();
            if pattern_lower.is_empty() {
                continue;
            }

            let domain_target = pattern_lower.strip_prefix("*.").unwrap_or(&pattern_lower);

            for dom in &candidate_domains {
                if dom == domain_target
                    || dom.ends_with(&format!(".{}", domain_target))
                    || dom.contains(domain_target)
                {
                    return true;
                }
            }
        }

        false
    }

    pub fn host_matches_scope(&self, host: &str) -> bool {
        let Some(ref scope) = self.scope else {
            return true;
        };
        if scope.is_empty() {
            return true;
        }
        for pattern in scope {
            let pattern_lower = pattern.trim().to_lowercase();
            let domain_target = pattern_lower.strip_prefix("*.").unwrap_or(&pattern_lower);
            if host == domain_target
                || host.ends_with(&format!(".{}", domain_target))
                || host.contains(domain_target)
            {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyRecordMode {
    #[default]
    All,
    TargetScope,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyDbFilterConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub mode: ProxyRecordMode,
    #[serde(default)]
    pub custom_hosts: Vec<String>,
    #[serde(default)]
    pub target_hosts: Vec<String>,
    #[serde(default)]
    pub exclude_hosts: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for ProxyDbFilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: ProxyRecordMode::All,
            custom_hosts: Vec::new(),
            target_hosts: Vec::new(),
            exclude_hosts: Vec::new(),
        }
    }
}

// ── MockForge Models ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockDomain {
    pub id: String,
    pub hostname: String,
    pub ssl: bool,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMatcher {
    #[serde(rename = "headerKey")]
    pub header_key: Option<String>,
    #[serde(rename = "headerValue")]
    pub header_value: Option<String>,
    #[serde(rename = "queryKey")]
    pub query_key: Option<String>,
    #[serde(rename = "queryValue")]
    pub query_value: Option<String>,
    #[serde(rename = "bodyContains")]
    pub body_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosConfig {
    #[serde(rename = "latencyMode")]
    pub latency_mode: String,
    #[serde(rename = "latencyFixed")]
    pub latency_fixed: Option<u64>,
    #[serde(rename = "latencyMin")]
    pub latency_min: Option<u64>,
    #[serde(rename = "latencyMax")]
    pub latency_max: Option<u64>,
    #[serde(rename = "errorRate")]
    pub error_rate: Option<f64>,
    #[serde(rename = "errorStatus")]
    pub error_status: Option<u16>,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            latency_mode: "none".to_string(),
            latency_fixed: None,
            latency_min: None,
            latency_max: None,
            error_rate: None,
            error_status: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParam {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockRoute {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "domainId")]
    pub domain_id: String,
    pub method: String,
    pub path: String,
    #[serde(rename = "statusCode")]
    pub status_code: u16,
    #[serde(rename = "responseBody")]
    pub response_body: String,
    #[serde(rename = "responseHeaders")]
    pub response_headers: HashMap<String, String>,
    #[serde(default)]
    pub matchers: Vec<RequestMatcher>,
    #[serde(default)]
    pub chaos: ChaosConfig,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(rename = "matcherEnabled")]
    #[serde(default = "default_matcher_enabled")]
    pub matcher_enabled: bool,
    #[serde(rename = "requestQueryParams")]
    pub request_query_params: Option<Vec<QueryParam>>,
    #[serde(rename = "requestBody")]
    pub request_body: Option<String>,
}

fn default_matcher_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockServerStatus {
    pub running: bool,
    pub port: u16,
    #[serde(rename = "domainId")]
    pub domain_id: Option<String>,
    #[serde(rename = "corsEnabled")]
    pub cors_enabled: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLog {
    pub id: String,
    #[serde(rename = "domainId")]
    pub domain_id: String,
    #[serde(rename = "routeId")]
    pub route_id: Option<String>,
    pub method: String,
    pub path: String,
    #[serde(rename = "statusCode")]
    pub status_code: u16,
    #[serde(rename = "latencyMs")]
    pub latency_ms: u64,
    pub timestamp: String,
    #[serde(rename = "requestHeaders")]
    pub request_headers: HashMap<String, String>,
    #[serde(rename = "requestBody")]
    pub request_body: Option<String>,
    #[serde(rename = "source", skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

// ── Crawler Models ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlConfig {
    pub target_url: String,
    pub strategy: Option<String>,
    pub max_depth: u32,
    pub max_pages: u32,
    pub same_domain_only: bool,
    pub exclude_paths: Option<String>,
    pub request_delay_ms: u64,
    pub timeout_ms: u64,
    pub enable_ai_insights: bool,
    pub network_settle_ms: Option<u64>,
    #[serde(default = "default_true")]
    pub capture_screenshots: bool,
    #[serde(default = "default_true")]
    pub capture_rendered_html: bool,
    pub resume_from_url: Option<String>,
    pub human_input_fields: Option<HashMap<String, String>>,
    #[serde(default = "default_true")]
    pub headless: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlSession {
    pub id: String,
    pub target_url: String,
    pub status: String,
    pub strategy: String,
    pub max_depth: u32,
    pub max_pages: u32,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlPage {
    pub id: String,
    pub session_id: String,
    pub url: String,
    pub title: Option<String>,
    pub status: String,
    pub depth: u32,
    pub parent_url: Option<String>,
    pub http_status: Option<u16>,
    pub links_found: u32,
    pub forms_found: u32,
    pub discovered_at: String,
    pub visited_at: Option<String>,
    pub ai_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_used_for_analysis: Option<bool>,
    pub interesting: Option<bool>,
    pub screenshot_path: Option<String>,
    pub rendered_html_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIInsight {
    pub id: String,
    pub session_id: String,
    pub page_id: Option<String>,
    pub severity: String,
    pub r#type: String,
    pub title: String,
    pub description: String,
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_used_for_analysis: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_tool_name: Option<String>,
    pub reviewed: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLog {
    pub id: String,
    pub session_id: String,
    pub level: String,
    pub r#type: String,
    pub message: String,
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_used_for_analysis: Option<bool>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_input_request: Option<serde_json::Value>,
}

// ── Collaborator Models ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaboratorServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_key: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaboratorPayload {
    pub id: String,
    pub server_id: String,
    pub identifier: String,
    pub payload_url: String,
    pub name: String,
    pub description: String,
    pub tags: String,
    pub interaction_count: i64,
    pub status: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePayloadRequest {
    pub server_id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaboratorInteraction {
    pub id: String,
    pub payload_id: String,
    pub interaction_type: String,
    pub source_ip: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub headers: Option<String>,
    pub raw_request: Option<String>,
    pub request_body: Option<String>,
    pub server_response: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaboratorDashboardStats {
    pub active_payloads: i64,
    pub interactions_today: i64,
    pub dns_events: i64,
    pub http_events: i64,
    pub https_events: i64,
    pub last_callback: Option<String>,
    pub connected_servers: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInteraction {
    pub id: String,
    pub identifier: String,
    pub interaction_type: String,
    pub source_ip: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub headers: Option<String>,
    pub raw_request: Option<String>,
    pub request_body: Option<String>,
    pub server_response: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerPayloadCreated {
    pub identifier: String,
    pub payload_url: String,
}

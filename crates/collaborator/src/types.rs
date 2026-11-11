use serde::{Deserialize, Serialize};

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

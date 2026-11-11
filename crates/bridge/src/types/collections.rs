//! Repeater, collection, stash, context, chronicle, and active-request DTOs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RepeaterRequestDto {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RepeaterResponseDto {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub time_ms: u128,
    pub final_url: String,
}

/// Top-level workspace of folders + requests in the Forge sidebar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    pub sort_order: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

/// Folder in the Forge sidebar tree; `parent_id` nests it inside another folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StashDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    /// Set on root folders; nested folders inherit membership via ancestors.
    pub collection_id: Option<String>,
    pub sort_order: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

/// Saved request endpoint inside a stash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StashEndpointDto {
    pub id: String,
    pub stash_id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    /// JSON object string of headers, or null.
    pub headers: Option<String>,
    pub body: Option<String>,
    pub body_type: Option<String>,
    pub pre_script: Option<String>,
    pub test_script: Option<String>,
    pub sort_order: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

/// Environment profile with JSON variables (`{"key":{"value":"...","enabled":true}}` or flat map).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextDto {
    pub id: String,
    pub name: String,
    pub variables: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Past Forge execution record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChronicleLogDto {
    pub id: String,
    pub timestamp: String,
    pub method: String,
    pub url: String,
    pub request_headers: Option<String>,
    pub request_body: Option<String>,
    pub response_status: Option<u16>,
    pub response_status_text: Option<String>,
    pub response_headers: Option<String>,
    pub response_body: Option<String>,
    pub duration_ms: Option<i64>,
}

/// Working draft for the Forge request editor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRequestState {
    pub endpoint_id: Option<String>,
    pub stash_id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body: String,
    pub body_type: String,
    pub pre_script: String,
    pub test_script: String,
    pub is_loading: bool,
    pub error: Option<String>,
}

impl Default for ActiveRequestState {
    fn default() -> Self {
        Self {
            endpoint_id: None,
            stash_id: None,
            name: "Untitled".into(),
            method: "GET".into(),
            url: "https://httpbin.org/get".into(),
            body: String::new(),
            body_type: "raw".into(),
            pre_script: String::new(),
            test_script: String::new(),
            is_loading: false,
            error: None,
        }
    }
}

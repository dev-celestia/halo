use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub enum InvokerAttackMode {
    Sniper,
    BatteringRam,
    Pitchfork,
    ClusterBomb,
}

#[derive(Debug, Clone, Deserialize)]
pub enum InvokerPayloadType {
    SimpleList,
    RuntimeFile,
    NumberRange,
}

#[derive(Debug, Clone, Deserialize)]
pub enum InvokerPayloadProcessingStep {
    UrlEncode,
    UrlDecode,
    Base64Encode,
    Base64Decode,
    Md5Hash,
    Sha1Hash,
    Sha256Hash,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokerPayloadPosition {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokerPayloadConfig {
    pub payload_type: InvokerPayloadType,
    pub values: Vec<String>,
    pub file_path: Option<String>,
    pub number_start: Option<i64>,
    pub number_end: Option<i64>,
    pub number_step: Option<i64>,
    pub number_format: Option<String>,
    pub processing: Vec<InvokerPayloadProcessingStep>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokerGrepMatchConfig {
    pub enabled: bool,
    pub keyword: String,
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokerGrepExtractConfig {
    pub enabled: bool,
    pub regex: String,
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokerSessionHandlingConfig {
    pub enabled: bool,
    pub extract_token_name: Option<String>,
    pub extract_from_response: Option<String>,
    pub update_header_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokerHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub follow_redirects: bool,
    pub max_hops: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InvokerAttackConfig {
    pub name: String,
    pub mode: InvokerAttackMode,
    pub base_request: InvokerHttpRequest,
    pub positions: Vec<InvokerPayloadPosition>,
    pub payload_config: InvokerPayloadConfig,
    #[serde(default)]
    pub position_payloads: Option<HashMap<String, InvokerPayloadConfig>>,
    pub concurrency: usize,
    pub delay_ms: u64,
    pub delay_max_ms: Option<u64>,
    pub retries: usize,
    pub grep_match: InvokerGrepMatchConfig,
    #[serde(rename = "grep_extract")]
    pub grep_extract: InvokerGrepExtractConfig,
    pub session_handling: InvokerSessionHandlingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InvokerProgress {
    Update { current: usize, total: usize },
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokerResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub time_ms: u128,
    pub final_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokerAttackResult {
    pub id: String,
    pub payload_values: HashMap<String, String>,
    pub status: Option<u16>,
    pub response_length: Option<usize>,
    pub response_time_ms: Option<u128>,
    pub error: Option<String>,
    pub comment: Option<String>,
    pub response: Option<InvokerResponse>,
    pub grep_match: bool,
    pub grep_extracted: Option<String>,
}

impl InvokerAttackResult {
    pub fn error(error: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            payload_values: HashMap::new(),
            status: None,
            response_length: None,
            response_time_ms: None,
            error: Some(error),
            comment: None,
            response: None,
            grep_match: false,
            grep_extracted: None,
        }
    }
}

#[derive(Default)]
pub struct InvokerState {
    pub cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

//! Intruder attack configuration, drafts, and result DTOs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Attack algorithm for Intruder / Invoker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IntruderAttackMode {
    #[default]
    Sniper,
    BatteringRam,
    Pitchfork,
    ClusterBomb,
}

impl IntruderAttackMode {
    pub const ALL: [Self; 4] = [
        Self::Sniper,
        Self::BatteringRam,
        Self::Pitchfork,
        Self::ClusterBomb,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sniper => "Sniper",
            Self::BatteringRam => "Battering Ram",
            Self::Pitchfork => "Pitchfork",
            Self::ClusterBomb => "Cluster Bomb",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Sniper => "One position at a time; others keep defaults",
            Self::BatteringRam => "Same payload in every marked position",
            Self::Pitchfork => "Lockstep payloads across positions",
            Self::ClusterBomb => "Cartesian product of all payloads",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IntruderPayloadType {
    #[default]
    SimpleList,
    NumberRange,
}

impl IntruderPayloadType {
    pub const ALL: [Self; 2] = [Self::SimpleList, Self::NumberRange];

    pub fn label(self) -> &'static str {
        match self {
            Self::SimpleList => "Simple List",
            Self::NumberRange => "Number Range",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntruderProcessingStep {
    UrlEncode,
    UrlDecode,
    Base64Encode,
    Base64Decode,
    Md5Hash,
    Sha1Hash,
    Sha256Hash,
}

impl IntruderProcessingStep {
    pub const ALL: [Self; 7] = [
        Self::UrlEncode,
        Self::UrlDecode,
        Self::Base64Encode,
        Self::Base64Decode,
        Self::Md5Hash,
        Self::Sha1Hash,
        Self::Sha256Hash,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::UrlEncode => "URL Encode",
            Self::UrlDecode => "URL Decode",
            Self::Base64Encode => "Base64 Encode",
            Self::Base64Decode => "Base64 Decode",
            Self::Md5Hash => "MD5",
            Self::Sha1Hash => "SHA-1",
            Self::Sha256Hash => "SHA-256",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IntruderConfigTab {
    #[default]
    Request,
    Attack,
    Payloads,
}

/// Working draft assembled from editors + Intruder config when starting an attack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntruderDraft {
    pub method: String,
    pub url: String,
    pub headers_text: String,
    pub body: String,
    pub payloads_text: String,
    pub mode: IntruderAttackMode,
    pub payload_type: IntruderPayloadType,
    pub number_start: i64,
    pub number_end: i64,
    pub number_step: i64,
    pub number_format: String,
    pub processing: Vec<IntruderProcessingStep>,
    pub concurrency: usize,
    pub delay_ms: u64,
    pub delay_max_ms: Option<u64>,
    pub retries: usize,
    pub grep_match_enabled: bool,
    pub grep_keyword: String,
    pub grep_case_sensitive: bool,
    pub grep_extract_enabled: bool,
    pub grep_extract_regex: String,
    pub session_enabled: bool,
    pub session_token_name: String,
    pub session_header_name: String,
}

impl Default for IntruderDraft {
    fn default() -> Self {
        Self {
            method: "GET".into(),
            url: String::new(),
            headers_text: String::new(),
            body: String::new(),
            payloads_text: String::new(),
            mode: IntruderAttackMode::Sniper,
            payload_type: IntruderPayloadType::SimpleList,
            number_start: 1,
            number_end: 100,
            number_step: 1,
            number_format: "{}".into(),
            processing: Vec::new(),
            concurrency: 4,
            delay_ms: 0,
            delay_max_ms: None,
            retries: 0,
            grep_match_enabled: false,
            grep_keyword: String::new(),
            grep_case_sensitive: false,
            grep_extract_enabled: false,
            grep_extract_regex: String::new(),
            session_enabled: false,
            session_token_name: String::new(),
            session_header_name: String::new(),
        }
    }
}

/// Live attack config knobs bound in the Intruder UI (outside text editors).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntruderConfig {
    pub mode: IntruderAttackMode,
    pub payload_type: IntruderPayloadType,
    pub concurrency: usize,
    pub delay_ms: u64,
    pub delay_max_ms: Option<u64>,
    pub retries: usize,
    pub number_start: i64,
    pub number_end: i64,
    pub number_step: i64,
    pub number_format: String,
    pub processing: Vec<IntruderProcessingStep>,
    pub grep_match_enabled: bool,
    pub grep_case_sensitive: bool,
    pub grep_extract_enabled: bool,
    pub session_enabled: bool,
    pub config_tab: IntruderConfigTab,
    pub matches_only: bool,
    /// Status class filters: 2, 3, 4, 5. Empty = all.
    pub status_classes: Vec<u16>,
}

impl Default for IntruderConfig {
    fn default() -> Self {
        Self {
            mode: IntruderAttackMode::Sniper,
            payload_type: IntruderPayloadType::SimpleList,
            concurrency: 4,
            delay_ms: 0,
            delay_max_ms: None,
            retries: 0,
            number_start: 1,
            number_end: 100,
            number_step: 1,
            number_format: "{}".into(),
            processing: Vec::new(),
            grep_match_enabled: false,
            grep_case_sensitive: false,
            grep_extract_enabled: false,
            session_enabled: false,
            config_tab: IntruderConfigTab::Request,
            matches_only: false,
            status_classes: Vec::new(),
        }
    }
}

/// One completed Intruder request (streamed via `invoker-result-*`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IntruderResultRow {
    pub id: String,
    pub payload_values: HashMap<String, String>,
    pub status: Option<u16>,
    pub response_length: Option<usize>,
    pub response_time_ms: Option<u128>,
    pub error: Option<String>,
    pub grep_match: bool,
    pub grep_extracted: Option<String>,
    pub response_body: Option<String>,
    pub response_headers: HashMap<String, String>,
    pub final_url: Option<String>,
    pub status_text: Option<String>,
}

impl IntruderResultRow {
    pub fn payload_summary(&self) -> String {
        if self.payload_values.is_empty() {
            return "—".into();
        }
        let mut pairs: Vec<_> = self.payload_values.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        pairs
            .into_iter()
            .map(|(k, v)| {
                if self.payload_values.len() == 1 {
                    v.clone()
                } else {
                    format!("{k}={v}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IntruderStatus {
    pub running: bool,
    pub attack_id: Option<String>,
    pub message: Option<String>,
    pub progress_current: usize,
    pub progress_total: usize,
    pub results: Vec<IntruderResultRow>,
    pub selected_result_id: Option<String>,
    pub config: IntruderConfig,
}

//! Real backend adapter over [`halo_backend::BackendRuntime`].

//! Split by backend area: each submodule holds one `impl Backend for
//! NativeBackend` block mirroring the section order of [`crate::backend`].

mod agent;
mod coder;
mod dto;
mod git;
mod intercept;
mod intruder;
mod mock;
mod proxy;
mod r2;
mod repeater;
mod storage;

use std::collections::HashMap;
use std::sync::Arc;

use halo_backend::commands;
use halo_backend::commands::crawler::{
    crawler_pause_crawl, crawler_resume_crawl, crawler_start_crawl, crawler_stop_crawl,
    delete_crawler_session, list_crawler_insights, list_crawler_logs, list_crawler_pages,
};
use halo_backend::commands::intruder::{
    InvokerAttackConfig, InvokerAttackMode, InvokerGrepExtractConfig, InvokerGrepMatchConfig,
    InvokerHttpRequest, InvokerPayloadConfig, InvokerPayloadPosition, InvokerPayloadProcessingStep,
    InvokerPayloadType, InvokerSessionHandlingConfig, InvokerState, begin_invoker_attack,
    cancel_invoker_attack,
};
use halo_backend::commands::repeater::{RepeaterRequest, send_repeater_request};
use halo_backend::crawler::{CrawlConfig, CrawlerState};
use halo_backend::tauri::Manager;
use halo_backend::{
    BackendRuntime, InterceptMode, ProxyFilter, ProxyLogSummary, ProxyRecord, ProxyState,
};
use parking_lot::Mutex;
use uuid::Uuid;

use crate::backend::{
    Backend, BackendAgent, BackendCoder, BackendGit, BackendHistory, BackendIntercept,
    BackendIntruder, BackendMock, BackendProxy, BackendR2, BackendRepeater, BackendStorage,
};
use crate::error::{BridgeError, BridgeResult};
use crate::types::{
    ChronicleLogDto, CollectionDto, ContextDto, CrawlerCrawlConfig, CrawlerInsightDto,
    CrawlerLogDto, CrawlerPageDto, CrawlerSessionDto, DeleteArtifactResult, HistoryFilter,
    HttpLogDetail, HttpLogRow, HttpSessionDto, IntruderAttackMode, IntruderDraft,
    IntruderPayloadType, IntruderProcessingStep, IntruderStatus, MockChaosDto, MockDomainDto,
    MockLogDto, MockRouteDto, MockServerStatusDto, NoteRecord, PausedRow, PortScanRequestDto,
    PortScanResultDto, ProxyStatus, RepeaterRequestDto, RepeaterResponseDto, ResetLocalDataResult,
    StashDto, StashEndpointDto, StorageInfo,
};

/// Native MITM proxy backend (HTTP / intercept / history live on [`BackendRuntime`]).
impl Backend for NativeBackend {}

pub struct NativeBackend {
    runtime: Arc<BackendRuntime>,
    tokio: tokio::runtime::Runtime,
    active_attack: Mutex<Option<String>>,
}

impl NativeBackend {
    pub fn boot() -> BridgeResult<Self> {
        let runtime = BackendRuntime::boot().map_err(BridgeError::message)?;
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("halo-bridge")
            .build()
            .map_err(|e| BridgeError::message(e.to_string()))?;
        Ok(Self {
            runtime: Arc::new(runtime),
            tokio,
            active_attack: Mutex::new(None),
        })
    }

    pub fn runtime(&self) -> &BackendRuntime {
        &self.runtime
    }

    pub fn proxy_state(&self) -> &ProxyState {
        &self.runtime.proxy_state
    }

    /// Subscribe to in-process AppHandle emits (`proxy-record`, etc.).
    pub fn on_backend_event<F>(&self, f: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        self.runtime.app.on_event(f);
    }

    fn draft_to_attack(draft: &IntruderDraft) -> BridgeResult<InvokerAttackConfig> {
        let payloads: Vec<String> = draft
            .payloads_text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();

        let headers = parse_headers(&draft.headers_text);
        let base_request = InvokerHttpRequest {
            method: if draft.method.trim().is_empty() {
                "GET".into()
            } else {
                draft.method.trim().to_string()
            },
            url: draft.url.clone(),
            headers,
            body: draft.body.clone(),
            follow_redirects: true,
            max_hops: 10,
        };

        let marker_count = halo_backend::commands::intruder::count_markers(&base_request);
        let positions = (0..marker_count)
            .map(|index| InvokerPayloadPosition {
                name: format!("position_{}", index + 1),
                start: 0,
                end: 0,
            })
            .collect();

        let (payload_type, values, number_start, number_end, number_step, number_format) =
            match draft.payload_type {
                IntruderPayloadType::SimpleList => {
                    if payloads.is_empty() {
                        return Err(BridgeError::message(
                            "Add at least one payload (one per line)",
                        ));
                    }
                    (
                        InvokerPayloadType::SimpleList,
                        payloads,
                        None,
                        None,
                        None,
                        None,
                    )
                }
                IntruderPayloadType::NumberRange => (
                    InvokerPayloadType::NumberRange,
                    Vec::new(),
                    Some(draft.number_start),
                    Some(draft.number_end),
                    Some(draft.number_step.max(1)),
                    Some(draft.number_format.clone()),
                ),
            };

        let processing = draft
            .processing
            .iter()
            .map(|step| match step {
                IntruderProcessingStep::UrlEncode => InvokerPayloadProcessingStep::UrlEncode,
                IntruderProcessingStep::UrlDecode => InvokerPayloadProcessingStep::UrlDecode,
                IntruderProcessingStep::Base64Encode => InvokerPayloadProcessingStep::Base64Encode,
                IntruderProcessingStep::Base64Decode => InvokerPayloadProcessingStep::Base64Decode,
                IntruderProcessingStep::Md5Hash => InvokerPayloadProcessingStep::Md5Hash,
                IntruderProcessingStep::Sha1Hash => InvokerPayloadProcessingStep::Sha1Hash,
                IntruderProcessingStep::Sha256Hash => InvokerPayloadProcessingStep::Sha256Hash,
            })
            .collect();

        let mode = match draft.mode {
            IntruderAttackMode::Sniper => InvokerAttackMode::Sniper,
            IntruderAttackMode::BatteringRam => InvokerAttackMode::BatteringRam,
            IntruderAttackMode::Pitchfork => InvokerAttackMode::Pitchfork,
            IntruderAttackMode::ClusterBomb => InvokerAttackMode::ClusterBomb,
        };

        Ok(InvokerAttackConfig {
            name: "ui-attack".into(),
            mode,
            base_request,
            positions,
            payload_config: InvokerPayloadConfig {
                payload_type,
                values,
                file_path: None,
                number_start,
                number_end,
                number_step,
                number_format,
                processing,
            },
            position_payloads: None,
            concurrency: draft.concurrency.clamp(1, 200),
            delay_ms: draft.delay_ms,
            delay_max_ms: draft.delay_max_ms,
            retries: draft.retries,
            grep_match: InvokerGrepMatchConfig {
                enabled: draft.grep_match_enabled,
                keyword: draft.grep_keyword.clone(),
                case_sensitive: draft.grep_case_sensitive,
            },
            grep_extract: InvokerGrepExtractConfig {
                enabled: draft.grep_extract_enabled,
                regex: draft.grep_extract_regex.clone(),
                replacement: None,
            },
            session_handling: InvokerSessionHandlingConfig {
                enabled: draft.session_enabled,
                extract_token_name: non_empty_opt(&draft.session_token_name),
                extract_from_response: None,
                update_header_name: non_empty_opt(&draft.session_header_name),
            },
        })
    }
}

fn non_empty_opt(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_headers(text: &str) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_string(), value.trim().to_string());
        }
    }
    headers
}

pub(crate) fn summary_to_row(r: ProxyLogSummary) -> HttpLogRow {
    HttpLogRow {
        id: r.id,
        session_id: r.session_id,
        method: r.method,
        url: r.url,
        status: r.response_status,
        status_text: r.response_status_text,
        content_type: r.response_content_type,
        host: r.host,
        timestamp: r.timestamp,
        request_size: r.request_body_size,
        response_size: r.response_body_size,
        user_agent: r.user_agent,
        server_addr: r.server_addr,
    }
}

pub(crate) fn record_to_detail(record: ProxyRecord) -> HttpLogDetail {
    let req_body = body_to_display(&record.request.body);
    let (res_status, res_status_text, res_http_version, res_headers, res_body, res_body_bytes) =
        match record.response {
            Some(res) => (
                Some(res.status_code),
                Some(res.status_text),
                Some(res.http_version),
                res.headers,
                body_to_display(&res.body),
                res.body.len(),
            ),
            None => (None, None, None, HashMap::new(), String::new(), 0),
        };

    HttpLogDetail {
        id: record.id.to_string(),
        timestamp: record.timestamp.to_rfc3339(),
        client_addr: record.client_addr,
        server_addr: record.server_addr,
        method: record.request.method,
        url: record.request.uri,
        req_http_version: record.request.http_version,
        req_headers: record.request.headers,
        req_body_bytes: record.request.body.len(),
        req_body,
        res_status,
        res_status_text,
        res_http_version,
        res_headers,
        res_body,
        res_body_bytes,
    }
}

fn body_to_display(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let preview: String = bytes
                .iter()
                .take(512)
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "<binary {} bytes>\n{}",
                bytes.len(),
                if bytes.len() > 512 {
                    format!("{preview} …")
                } else {
                    preview
                }
            )
        }
    }
}

/// Map UI filter → backend `ProxyFilter`, expanding status classes / chips.
pub(crate) fn to_proxy_filter(filter: &HistoryFilter) -> ProxyFilter {
    let mut status_codes: Vec<u16> = Vec::new();
    for class in &filter.status_classes {
        let base = class * 100;
        for code in base..(base + 100) {
            status_codes.push(code);
        }
    }
    if filter.errors_only {
        status_codes.extend(400..600);
    }

    ProxyFilter {
        search: filter
            .search
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        path: None,
        methods: if filter.methods.is_empty() {
            None
        } else {
            Some(filter.methods.clone())
        },
        status_codes: if status_codes.is_empty() {
            None
        } else {
            status_codes.sort_unstable();
            status_codes.dedup();
            Some(status_codes)
        },
        scope: None,
        session_id: filter.session_id.clone(),
    }
}

/// Client-side chip filters that aren't expressible as exact status_codes lists alone.
pub(crate) fn apply_chip_filters(rows: Vec<HttpLogRow>, filter: &HistoryFilter) -> Vec<HttpLogRow> {
    rows.into_iter()
        .filter(|row| {
            if filter.hide_304 && row.status == Some(304) {
                return false;
            }
            if filter.hide_images {
                if let Some(ct) = row.content_type.as_deref() {
                    let ct = ct.to_ascii_lowercase();
                    if ct.starts_with("image/")
                        || ct.contains("font/")
                        || ct.contains("woff")
                        || ct.contains("opentype")
                    {
                        return false;
                    }
                }
            }
            if filter.only_api {
                let ct = row
                    .content_type
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !(ct.contains("json")
                    || ct.contains("xml")
                    || ct.contains("javascript")
                    || ct.contains("text/plain"))
                {
                    return false;
                }
            }
            true
        })
        .collect()
}

//! In-memory stub backend — stands in until Track 2 ports the real proxy.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend::{
    Backend, BackendAgent, BackendCoder, BackendGit, BackendHistory, BackendIntercept,
    BackendIntruder, BackendMock, BackendProxy, BackendR2, BackendRepeater, BackendStorage,
};
use crate::error::{BridgeError, BridgeResult};
use crate::types::{
    ChronicleLogDto, CollectionDto, ContextDto, CrawlerCrawlConfig, CrawlerInsightDto,
    CrawlerLogDto, CrawlerPageDto, CrawlerSessionDto, DeleteArtifactResult, HistoryFilter,
    HttpLogDetail, HttpLogRow, HttpSessionDto, IntruderDraft, IntruderStatus, MockChaosDto,
    MockDomainDto, MockLogDto, MockRouteDto, MockServerStatusDto, NoteRecord, PausedRow,
    PortScanRequestDto, PortScanResultDto, ProxyStatus, RepeaterRequestDto, RepeaterResponseDto,
    ResetLocalDataResult, StashDto, StashEndpointDto, StorageInfo,
};

/// Fake backend that only mutates local status. No sockets, no MITM.
pub struct StubBackend {
    /// Native boot error that forced this fallback, if any. Surfaced verbatim
    /// so a degraded launch reports its cause instead of a generic message.
    boot_error: Option<String>,
    status: Mutex<ProxyStatus>,
    history: Mutex<Vec<HttpLogRow>>,
    sessions: Mutex<Vec<HttpSessionDto>>,
    intercept_enabled: Mutex<bool>,
    scope: Mutex<Vec<String>>,
    active_tab_id: Mutex<String>,
    paused: Mutex<Vec<PausedRow>>,
    attack: Mutex<Option<String>>,
    collections: Mutex<Vec<CollectionDto>>,
    stashes: Mutex<Vec<StashDto>>,
    endpoints: Mutex<Vec<StashEndpointDto>>,
    contexts: Mutex<Vec<ContextDto>>,
    chronicle: Mutex<Vec<ChronicleLogDto>>,
    notes: Mutex<Vec<NoteRecord>>,
    agent_sessions: Mutex<Vec<halo_backend::HaloAgentSessionRecord>>,
    mock_domains: Mutex<Vec<MockDomainDto>>,
    mock_routes: Mutex<Vec<MockRouteDto>>,
    mock_logs: Mutex<Vec<MockLogDto>>,
    mock_server: Mutex<MockServerStatusDto>,
}

fn sample_notes() -> Vec<NoteRecord> {
    vec![
        NoteRecord {
            id: "1".into(),
            name: "Welcome to Notes".into(),
            note: "# Welcome to Notes\n\nNotes is your tabbed markdown notepad.\n\n### Features\n- **Tabbed workspace**: Open and switch between multiple notes\n- **Markdown editor & preview**: Live syntax-highlighted code editor and rendered preview\n- **Library management**: Filter, search, duplicate, and organize all your notes\n- **Drawing studio**: Built-in whiteboard for diagrams and sketches\n- **Promote to memory**: Send notes to AI assistant context\n".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        },
        NoteRecord {
            id: "2".into(),
            name: "Keyboard Shortcuts".into(),
            note: "# Keyboard Shortcuts\n\n- `⌘S` / `Ctrl+S` : Save note\n- `⌘A` / `Ctrl+A` : Select all text\n- Double-click tab to rename\n- Switch view modes in the toolbar: **Editor**, **Split**, or **Preview**\n".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        },
    ]
}

impl StubBackend {
    pub fn new() -> Self {
        Self::build(None)
    }

    /// Stub installed after [`crate::NativeBackend::boot`] failed. `reason` is
    /// shown to the user, so pass the boot error itself, not a paraphrase.
    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::build(Some(reason.into()))
    }

    fn build(boot_error: Option<String>) -> Self {
        Self {
            boot_error,
            status: Mutex::new(ProxyStatus::default()),
            history: Mutex::new(sample_history()),
            sessions: Mutex::new(vec![HttpSessionDto {
                id: "stub-session".into(),
                name: "Default".into(),
                is_active: true,
                request_count: 1,
                capture_mode: "all".into(),
            }]),
            intercept_enabled: Mutex::new(false),
            scope: Mutex::new(vec!["example.com".into()]),
            active_tab_id: Mutex::new("default".into()),
            paused: Mutex::new(Vec::new()),
            attack: Mutex::new(None),
            collections: Mutex::new(vec![CollectionDto {
                id: "stub-collection".into(),
                name: "Default".into(),
                sort_order: Some(0),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }]),
            stashes: Mutex::new(vec![StashDto {
                id: "stub-stash".into(),
                name: "Sample Folder".into(),
                parent_id: None,
                collection_id: Some("stub-collection".into()),
                sort_order: Some(0),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }]),
            endpoints: Mutex::new(vec![StashEndpointDto {
                id: "stub-ep".into(),
                stash_id: "stub-stash".into(),
                name: "List users".into(),
                method: "GET".into(),
                url: "https://{{host}}/api/users".into(),
                headers: Some(r#"{"Accept":"application/json"}"#.into()),
                body: Some(String::new()),
                body_type: Some("none".into()),
                pre_script: Some(String::new()),
                test_script: Some(String::new()),
                sort_order: Some(0),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }]),
            contexts: Mutex::new(vec![ContextDto {
                id: "stub-ctx".into(),
                name: "Local".into(),
                variables: r#"{"host":{"value":"example.com","enabled":true}}"#.into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            }]),
            chronicle: Mutex::new(Vec::new()),
            notes: Mutex::new(sample_notes()),
            agent_sessions: Mutex::new(Vec::new()),
            mock_domains: Mutex::new(Vec::new()),
            mock_routes: Mutex::new(Vec::new()),
            mock_logs: Mutex::new(Vec::new()),
            mock_server: Mutex::new(MockServerStatusDto::default()),
        }
    }

    /// Error for commands the stub cannot serve. Names the boot failure when
    /// there is one — "unavailable in stub mode" alone is not actionable.
    fn agent_unavailable(&self) -> BridgeError {
        match self.boot_error.as_deref() {
            Some(reason) => BridgeError::message(format!(
                "Halo Agent unavailable — the backend failed to start: {reason}"
            )),
            None => BridgeError::message("Halo Agent is unavailable in stub mode"),
        }
    }

    fn r2_unavailable(&self) -> BridgeError {
        match self.boot_error.as_deref() {
            Some(reason) => BridgeError::message(format!(
                "R2 storage unavailable — the backend failed to start: {reason}"
            )),
            None => BridgeError::message("R2 storage requires the native backend"),
        }
    }
}

impl Default for StubBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn sample_history() -> Vec<HttpLogRow> {
    vec![HttpLogRow {
        id: "stub-1".into(),
        session_id: "stub-session".into(),
        method: "GET".into(),
        url: "https://example.com/api/users".into(),
        status: Some(200),
        status_text: Some("OK".into()),
        content_type: Some("application/json".into()),
        host: Some("example.com".into()),
        timestamp: "2026-01-01T00:00:00Z".into(),
        request_size: 0,
        response_size: 1256,
        user_agent: Some("Mozilla/5.0".into()),
        server_addr: "example.com:443".into(),
    }]
}

fn now_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn lock_err<T>(g: std::sync::LockResult<T>) -> T {
    g.unwrap_or_else(|e| e.into_inner())
}

impl Backend for StubBackend {
    fn degraded_reason(&self) -> Option<String> {
        self.boot_error.clone()
    }
}

mod agent;
mod coder;
mod git;
mod history;
mod intercept;
mod intruder;
mod mock;
mod proxy;
mod r2;
mod repeater;
mod storage;
#[cfg(test)]
mod tests;

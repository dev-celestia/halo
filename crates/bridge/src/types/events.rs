//! Backend event stream pushed to the UI.

use super::*;

/// Events the backend pushes toward the UI (in-process; replaces Tauri emit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendEvent {
    ProxyStatusChanged(ProxyStatus),
    ProxyRecord(HttpLogRow),
    InvokerResult(IntruderResultRow),
    InvokerProgress {
        current: usize,
        total: usize,
    },
    InvokerComplete,
    CrawlerSession(CrawlerSessionDto),
    CrawlerPage(CrawlerPageDto),
    CrawlerInsight(CrawlerInsightDto),
    CrawlerLog(CrawlerLogDto),
    CrawlerSessionFailed {
        session_id: String,
        message: String,
    },
    /// Chunk of stdout/stderr from a Coder PTY session.
    CoderPtyOutput {
        session_id: String,
        data: String,
    },
    /// Coder PTY process exited or was killed. `generation` identifies the
    /// spawn that produced the reader, so a restart is not marked dead by the
    /// previous process's exit.
    CoderPtyExit {
        session_id: String,
        generation: u64,
    },
    /// Streaming coding-agent event for the Coder right panel.
    HaloAgentEvent {
        run_id: String,
        chat_id: String,
        event: halo_backend::commands::halo_agent::AgentEvent,
    },
    Log(String),
    HistoryUpdated,
    InterceptUpdated,
    /// One served request from the mock server or an override rule.
    MockLog(MockLogDto),
    /// Mock API server started / stopped.
    MockServerStatus(MockServerStatusDto),
    /// One open port discovered by a running scan.
    PortScanResult {
        scan_id: String,
        row: PortScanResultDto,
    },
    /// Scan progress: Update / Complete / Cancelled.
    PortScanProgress {
        scan_id: String,
        kind: String,
        current: usize,
        total: usize,
    },
    /// Upload progress for one R2 object (0–100).
    R2UploadProgress(halo_backend::commands::r2::R2UploadProgress),
}

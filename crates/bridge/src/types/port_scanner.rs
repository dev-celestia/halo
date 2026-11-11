//! Port-scanner result, request, and view DTOs.

use serde::{Deserialize, Serialize};

// ── Port Scanner ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortScanResultDto {
    pub host: String,
    pub port: u16,
    /// `open` | `closed` | `filtered` | `cancelled`.
    pub state: String,
    pub service: String,
    pub response_time_ms: Option<u64>,
    pub banner: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PortScanRequestDto {
    pub target: String,
    pub ports: Vec<u16>,
    pub timeout_ms: u64,
    pub concurrency: u32,
    pub banner_grab: bool,
    pub stealth_mode: bool,
    pub delay_ms: Option<u64>,
    pub jitter_ms: Option<u64>,
    pub randomize_ports: Option<bool>,
}

/// Everything the Port Scanner page renders. Presets live in
/// `bridge::port_scanner::PORT_PRESETS`; `ports_text` is the raw editor
/// string (the mini-language is parsed at scan time).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortScannerView {
    pub target: String,
    pub preset: String,
    pub ports_text: String,
    pub timeout_ms: String,
    pub concurrency: String,
    pub banner_grab: bool,
    pub stealth_mode: bool,
    pub delay_ms: String,
    pub jitter_ms: String,
    pub randomize_ports: bool,
    pub results: Vec<PortScanResultDto>,
    pub progress_current: usize,
    pub progress_total: usize,
    pub is_running: bool,
    pub has_run: bool,
    pub error: Option<String>,
    /// Scan id currently streaming events; stale events are dropped.
    pub active_scan_id: Option<String>,
}

impl Default for PortScannerView {
    fn default() -> Self {
        Self {
            target: String::new(),
            preset: "quick".into(),
            ports_text: String::new(),
            timeout_ms: "800".into(),
            concurrency: "100".into(),
            banner_grab: true,
            stealth_mode: true,
            delay_ms: "300".into(),
            jitter_ms: "200".into(),
            randomize_ports: true,
            results: Vec::new(),
            progress_current: 0,
            progress_total: 0,
            is_running: false,
            has_run: false,
            error: None,
            active_scan_id: None,
        }
    }
}

impl PortScannerView {
    pub fn open_results(&self) -> Vec<&PortScanResultDto> {
        self.results.iter().filter(|r| r.state == "open").collect()
    }

    pub fn progress_pct(&self) -> u32 {
        if self.progress_total == 0 {
            0
        } else {
            ((self.progress_current as f64 / self.progress_total as f64) * 100.0).min(100.0) as u32
        }
    }
}

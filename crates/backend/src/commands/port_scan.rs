//! Port scanner commands.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use halo_core::tauri::Manager;
use port_scanner::{PortScanRequest, PortScanResult, PortScanState};

use crate::BackendRuntime;

/// Runs a scan to completion. Results also stream as `port-scan-result-{id}`
/// events, which is how the UI paints rows while the scan is in flight.
pub async fn run_port_scan(
    ctx: &BackendRuntime,
    request: PortScanRequest,
    cancel: Arc<AtomicBool>,
) -> Vec<PortScanResult> {
    port_scanner::run_scan(ctx.app.clone(), request, cancel).await
}

/// Blocking scan over a plain port list — the shape a CLI can build without
/// knowing the crate's request struct.
pub async fn scan(
    ctx: &BackendRuntime,
    scan_id: String,
    target: String,
    ports: Vec<u16>,
    timeout_ms: u64,
    concurrency: u32,
    banner_grab: bool,
) -> Vec<PortScanResult> {
    let request = PortScanRequest {
        scan_id: scan_id.clone(),
        target,
        ports,
        timeout_ms,
        concurrency,
        banner_grab,
        scan_type: "connect".into(),
        stealth_mode: false,
        delay_ms: None,
        jitter_ms: None,
        randomize_ports: None,
    };
    let cancel = register_cancel(ctx, &scan_id);
    run_port_scan(ctx, request, cancel).await
}

/// Registers a cancel flag for `scan_id` and returns it.
pub fn register_cancel(ctx: &BackendRuntime, scan_id: &str) -> Arc<AtomicBool> {
    port_scanner::register_cancel_for(&ctx.app, scan_id)
}

pub fn stop_port_scan(ctx: &BackendRuntime, scan_id: &str) -> bool {
    let state = ctx.app.state::<PortScanState>();
    port_scanner::stop_port_scan(state.inner(), scan_id)
}

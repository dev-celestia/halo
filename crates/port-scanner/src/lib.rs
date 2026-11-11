//! TCP connect-scan engine for the Port Scanner page.
//!
//! Mirrors the reference IPC: `scan_ports` runs a concurrent connect scan and
//! streams `port-scan-result-{scan_id}` / `port-scan-progress-{scan_id}`
//! events while it works, returning the final open-port list. Stealth mode
//! adds per-probe delay + jitter and optional port randomization; every probe
//! honors a cancellation flag registered by `stop_port_scan`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use halo_core::tauri::{AppHandle, Emitter, Manager};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

/// Cancellation flags per active scan id.
#[derive(Default)]
pub struct PortScanState {
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl PortScanState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortScanRequest {
    #[serde(rename = "scan_id")]
    pub scan_id: String,
    pub target: String,
    pub ports: Vec<u16>,
    #[serde(rename = "timeout_ms")]
    pub timeout_ms: u64,
    pub concurrency: u32,
    #[serde(rename = "banner_grab")]
    pub banner_grab: bool,
    /// Reference parity: only `connect` scans are implemented.
    #[serde(rename = "scan_type")]
    pub scan_type: String,
    #[serde(rename = "stealth_mode")]
    pub stealth_mode: bool,
    #[serde(rename = "delay_ms")]
    pub delay_ms: Option<u64>,
    #[serde(rename = "jitter_ms")]
    pub jitter_ms: Option<u64>,
    #[serde(rename = "randomize_ports")]
    pub randomize_ports: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortScanResult {
    pub host: String,
    pub port: u16,
    /// `open` | `closed` | `filtered` | `cancelled`.
    pub state: String,
    pub service: String,
    #[serde(rename = "response_time_ms")]
    pub response_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Well-known service names for the results table (`service` column).
pub fn service_name(port: u16) -> String {
    match port {
        21 => "ftp",
        22 => "ssh",
        23 => "telnet",
        25 => "smtp",
        53 => "dns",
        80 | 8080 | 8008 | 8000 => "http",
        110 => "pop3",
        111 => "rpcbind",
        135 => "msrpc",
        139 | 445 => "smb",
        143 | 993 => "imap",
        161 => "snmp",
        389 | 636 => "ldap",
        443 | 8443 => "https",
        465 | 587 => "smtps",
        995 => "pop3s",
        1433 => "mssql",
        1521 => "oracle",
        2049 => "nfs",
        3000 | 5000 | 8888 => "http",
        3306 => "mysql",
        3389 => "rdp",
        5432 => "postgres",
        5900 => "vnc",
        6379 => "redis",
        9200 => "elasticsearch",
        27017 => "mongodb",
        _ => return format!("port-{port}"),
    }
    .to_string()
}

/// Register a cancel flag and spawn the scan onto the tokio runtime. The
/// caller (bridge) blocks on `run_scan` itself; this entry point exists for
/// fire-and-forget callers.
pub fn begin_port_scan(app: AppHandle, state: &PortScanState, request: PortScanRequest) -> String {
    let scan_id = request.scan_id.clone();
    let cancel = register_cancel(state, &scan_id);
    tokio::spawn(async move {
        let _ = run_scan(app, request, cancel).await;
    });
    scan_id
}

fn register_cancel(state: &PortScanState, scan_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    state
        .cancels
        .lock()
        .insert(scan_id.to_string(), flag.clone());
    flag
}

/// Request cancellation; returns true when a matching scan was running.
pub fn stop_port_scan(state: &PortScanState, scan_id: &str) -> bool {
    state
        .cancels
        .lock()
        .get(scan_id)
        .map(|flag| flag.store(true, Ordering::SeqCst))
        .is_some()
}

/// Progress payload for `port-scan-progress-{scan_id}`.
#[derive(Serialize)]
#[serde(tag = "type")]
enum Progress {
    Update { current: usize, total: usize },
    Complete,
    Cancelled,
}

/// Run the full scan, streaming events. Returns the open-port results
/// (authoritative, sorted by port).
pub async fn run_scan(
    app: AppHandle,
    request: PortScanRequest,
    cancel: Arc<AtomicBool>,
) -> Vec<PortScanResult> {
    let scan_id = request.scan_id.clone();
    let target = request.target.trim().to_string();
    let mut ports = request.ports.clone();

    if request.stealth_mode && request.randomize_ports.unwrap_or(false) {
        shuffle(&mut ports);
    }

    fn progress_channel(app: &AppHandle, scan_id: &str, payload: &Progress) {
        let _ = app.emit(format!("port-scan-progress-{scan_id}"), payload);
    }
    fn result_channel(app: &AppHandle, scan_id: &str, result: &PortScanResult) {
        let _ = app.emit(format!("port-scan-result-{scan_id}"), result);
    }

    let total = ports.len();
    let semaphore = Arc::new(Semaphore::new(request.concurrency.clamp(1, 200) as usize));
    let progress_total = total * 1; // single host target per scan for now
    let completed = Arc::new(AtomicUsize::new(0));
    let open: Arc<Mutex<Vec<PortScanResult>>> = Arc::new(Mutex::new(Vec::new()));
    let cancelled = Arc::new(AtomicBool::new(false));

    // Resolve the host once so every probe reuses the address.
    let host_for_addr = if target.contains(':') && !target.starts_with('[') {
        target.clone()
    } else {
        format!("{target}:0")
    };
    let resolved: Option<Vec<std::net::IpAddr>> = tokio::net::lookup_host(&host_for_addr)
        .await
        .ok()
        .map(|addrs| addrs.map(|a| a.ip()).collect());

    let Some(addresses) = resolved else {
        progress_channel(&app, &scan_id, &Progress::Complete);
        return Vec::new();
    };
    let ip = addresses.first().copied();

    let mut handles = Vec::new();
    if let Some(ip) = ip {
        'outer: for port in &ports {
            if cancel.load(Ordering::SeqCst) {
                cancelled.store(true, Ordering::SeqCst);
                break 'outer;
            }
            let permit = semaphore.clone().acquire_owned().await;
            let _permit = match permit {
                Ok(p) => p,
                Err(_) => break 'outer,
            };
            let app = app.clone();
            let addr = SocketAddr::new(ip, *port);
            let host = target.clone();
            let port = *port;
            let timeout = Duration::from_millis(request.timeout_ms.clamp(1, 120_000));
            let banner_grab = request.banner_grab;
            let completed = completed.clone();
            let open = open.clone();
            let cancelled = cancelled.clone();
            let progress_channel_app = app.clone();
            let result_channel_app = app.clone();
            let stealth = request.stealth_mode;
            let delay = request.delay_ms.unwrap_or(0);
            let jitter = request.jitter_ms.unwrap_or(0);
            let cancel = cancel.clone();
            let scan_id = scan_id.clone();

            handles.push(tokio::spawn(async move {
                if stealth && delay > 0 {
                    let jitter = if jitter > 0 {
                        rand::Rng::gen_range(&mut rand::thread_rng(), 0..=jitter)
                    } else {
                        0
                    };
                    tokio::time::sleep(Duration::from_millis(delay + jitter)).await;
                }
                if cancel.load(Ordering::SeqCst) {
                    cancelled.store(true, Ordering::SeqCst);
                    return;
                }

                let started = Instant::now();
                let outcome = tokio::time::timeout(timeout, TcpStream::connect(addr)).await;
                let latency = started.elapsed().as_millis() as u64;

                let mut banner = None;
                let result = match outcome {
                    Ok(Ok(stream)) => {
                        if banner_grab {
                            banner = grab_banner(stream).await;
                        }
                        let row = PortScanResult {
                            host: host.clone(),
                            port,
                            state: "open".into(),
                            service: service_name(port),
                            response_time_ms: Some(latency),
                            banner: banner.clone(),
                            error: None,
                        };
                        result_channel(&result_channel_app, &scan_id, &row);
                        Some(row)
                    }
                    Ok(Err(_)) | Err(_) => None,
                };

                let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
                if done % 32 == 0 || done == progress_total {
                    progress_channel(
                        &progress_channel_app,
                        &scan_id,
                        &Progress::Update {
                            current: done,
                            total: progress_total,
                        },
                    );
                }

                if let Some(row) = result {
                    open.lock().push(row);
                }
            }));
        }
    }

    for handle in handles {
        let _ = handle.await;
    }

    let _ = &cancelled;

    if cancel.load(Ordering::SeqCst) {
        progress_channel(&app, &scan_id, &Progress::Cancelled);
    } else {
        progress_channel(&app, &scan_id, &Progress::Complete);
    }

    {
        let mut open = open.lock();
        open.sort_by_key(|r| r.port);
        open.clone()
    }
}

async fn grab_banner(mut stream: TcpStream) -> Option<String> {
    use tokio::io::AsyncReadExt as _;
    let mut buf = [0u8; 256];
    // Servers like HTTP don't send first; wait briefly then read what's there.
    let read = tokio::time::timeout(Duration::from_millis(400), stream.read(&mut buf)).await;
    match read {
        Ok(Ok(0)) | Ok(Err(_)) | Err(_) => None,
        Ok(Ok(n)) => {
            let text = String::from_utf8_lossy(&buf[..n])
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            (!text.is_empty()).then_some(text)
        }
    }
}

fn shuffle(items: &mut [u16]) {
    // Fisher-Yates with the rand crate already in the dependency tree.
    for i in (1..items.len()).rev() {
        let j = rand::Rng::gen_range(&mut rand::thread_rng(), 0..=i);
        items.swap(i, j);
    }
}

/// Register (or replace) a cancel flag for `scan_id` from outside the crate —
/// used by the bridge before blocking on `run_scan`.
pub fn register_cancel_for(app: &AppHandle, scan_id: &str) -> Arc<AtomicBool> {
    if let Some(state) = app.try_state::<PortScanState>() {
        register_cancel(&state, scan_id)
    } else {
        Arc::new(AtomicBool::new(false))
    }
}

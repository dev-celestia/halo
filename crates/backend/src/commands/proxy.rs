//! Proxy lifecycle commands.
//!
//! Takes `&BackendRuntime` rather than a Tauri `AppHandle`/`State` pair so the
//! same body serves the desktop bridge and `halo-cli`.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use halo_core::tauri::Emitter;
use halo_core::types::ProxyDbFilterConfig;
use halo_traffic::{
    ProxyConfig, ProxyRuntimeStatus, active_proxy_port, begin_proxy_start, cancel_proxy_start,
    default_proxy_port, run, stop,
};

use crate::BackendRuntime;

/// How long to wait for a cold CA generation + bind. 15s: the old 5s window
/// timed out intermittently and left a half-started listener behind.
const START_ATTEMPTS: u32 = 150;
const START_POLL: Duration = Duration::from_millis(100);
const PROBE_TIMEOUT: Duration = Duration::from_millis(150);

pub async fn start_proxy(
    ctx: &BackendRuntime,
    port: u16,
    tls_port: u16,
) -> Result<String, String> {
    if let Some(active_port) = active_proxy_port() {
        emit_status(ctx, true, Some(active_port));
        return Ok(format!("Proxy already running on port {}", active_port));
    }

    *ctx.tls_port.lock() = Some(tls_port);

    let handle = ctx.app.clone();
    begin_proxy_start();
    std::thread::spawn(move || {
        run(
            ProxyConfig {
                port,
                // Fall back to the next free port rather than signalling
                // whatever holds this one (see `utils::ensure_port_free`).
                reuse: true,
                tls_port,
                enabled: true,
                ..ProxyConfig::default()
            },
            handle,
        );
    });

    for _ in 0..START_ATTEMPTS {
        tokio::time::sleep(START_POLL).await;
        if let Some(active_port) = active_proxy_port() {
            if probe(active_port).await {
                emit_status(ctx, true, Some(active_port));
                return Ok(format!(
                    "Proxy started on port {} (HTTP) and {} (HTTPS MITM)",
                    active_port, tls_port
                ));
            }
        }
    }

    cancel_proxy_start();
    let _ = stop();
    *ctx.tls_port.lock() = None;
    Err(format!(
        "Timed out waiting for proxy to start on port {}",
        port
    ))
}

pub async fn stop_proxy(ctx: &BackendRuntime) -> Result<String, String> {
    stop()?;
    ctx.proxy_state.clear_intercept_state();
    *ctx.tls_port.lock() = None;
    emit_status(ctx, false, None);
    Ok("Proxy stopped".to_string())
}

pub async fn get_proxy_status(_ctx: &BackendRuntime) -> Result<ProxyRuntimeStatus, String> {
    let default_port = default_proxy_port();
    let Some(port) = active_proxy_port() else {
        return Ok(ProxyRuntimeStatus {
            running: false,
            port: None,
            default_port,
            connections: 0,
        });
    };

    let running = probe(port).await;
    Ok(ProxyRuntimeStatus {
        running,
        port: running.then_some(port),
        default_port,
        connections: 0,
    })
}

/// TLS port of the running proxy, as requested at start time.
pub fn get_proxy_tls_port(ctx: &BackendRuntime) -> Option<u16> {
    *ctx.tls_port.lock()
}

pub fn get_proxy_db_filter(ctx: &BackendRuntime) -> ProxyDbFilterConfig {
    ctx.proxy_state.get_db_filter_config()
}

pub fn set_proxy_db_filter(ctx: &BackendRuntime, config: ProxyDbFilterConfig) {
    ctx.proxy_state.set_db_filter_config(config);
}

async fn probe(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    halo_core::tauri::async_runtime::spawn_blocking(move || {
        TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok()
    })
    .await
    .unwrap_or(false)
}

fn emit_status(ctx: &BackendRuntime, running: bool, port: Option<u16>) {
    let status = ProxyRuntimeStatus {
        running,
        port,
        default_port: default_proxy_port(),
        connections: 0,
    };
    let _ = ctx.app.emit("proxy-status-changed", &status);
}

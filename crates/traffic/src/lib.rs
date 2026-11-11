pub mod automation;
pub mod ca;
pub mod completion;
pub mod lifecycle;
pub mod mock_forge;
pub mod state;
pub mod types;
pub mod utils;
pub mod websocket;

pub use ca::export_ca_cert_pem;
pub use state::{
    InterceptMode, PausedRequest, ProxyFilter, ProxyRecord, ProxyRequest, ProxyResponse, ProxyState,
};
pub use utils::{encode_body, ensure_port_free};

use halo_core::tauri::{AppHandle, Emitter};
use hexbuffer_proxy::ProxyBuilder;
use parking_lot::Mutex;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicU16, Ordering},
};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::ca::ensure_ca_exists;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ProxyRuntimeStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub default_port: u16,
    pub connections: usize,
}

pub struct ProxyConfig {
    pub port: u16,
    /// Try the next free port when `port` is taken instead of failing.
    ///
    /// This never kills the process holding the port — see
    /// [`crate::proxy::utils::ensure_port_free`].
    pub reuse: bool,
    pub tls_port: u16,
    pub enabled: bool,
    /// Interface to bind. Loopback by default: the proxy has no authentication
    /// and MITMs TLS, so exposing it on a LAN interface is opt-in only.
    pub bind_addr: IpAddr,
}

/// Environment override for the bind address, for the rare case where the
/// proxy must be reachable from another machine (a phone, a VM, a container).
///
/// `HALO_PROXY_BIND=0.0.0.0` is honoured but warned about loudly: an
/// unauthenticated open proxy on a shared network is a remote-SSRF and
/// traffic-capture primitive aimed at the user's own machine.
const BIND_ENV: &str = "HALO_PROXY_BIND";

fn resolve_bind_addr(configured: IpAddr) -> IpAddr {
    match std::env::var(BIND_ENV) {
        Ok(raw) => match raw.trim().parse::<IpAddr>() {
            Ok(addr) => {
                if !addr.is_loopback() {
                    eprintln!(
                        "[proxy] WARNING: {BIND_ENV}={addr} exposes an unauthenticated \
                         TLS-intercepting proxy to the network. Anyone who can reach \
                         this port can relay traffic through this machine and reach \
                         services bound to 127.0.0.1."
                    );
                }
                addr
            }
            Err(e) => {
                eprintln!("[proxy] Ignoring invalid {BIND_ENV}={raw:?}: {e}");
                configured
            }
        },
        Err(_) => configured,
    }
}

static ACTIVE_PROXY_PORT: AtomicU16 = AtomicU16::new(0);
static DEFAULT_PROXY_PORT: AtomicU16 = AtomicU16::new(8888);
static PROXY_SHUTDOWN: OnceLock<Mutex<Option<oneshot::Sender<()>>>> = OnceLock::new();
static PROXY_ENABLED_HANDLE: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();
/// Set by [`cancel_proxy_start`] when a waiter gives up so `run` exits before
/// publishing a port (CA / bind can outlive the start timeout).
static PROXY_START_CANCELLED: AtomicBool = AtomicBool::new(false);

fn proxy_shutdown_sender() -> &'static Mutex<Option<oneshot::Sender<()>>> {
    PROXY_SHUTDOWN.get_or_init(|| Mutex::new(None))
}

fn proxy_enabled_handle() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    PROXY_ENABLED_HANDLE.get_or_init(|| Mutex::new(None))
}

/// Clear the cancel flag at the beginning of a new start attempt.
pub fn begin_proxy_start() {
    PROXY_START_CANCELLED.store(false, Ordering::SeqCst);
}

/// Ask an in-flight [`run`] to exit before it claims [`active_proxy_port`].
pub fn cancel_proxy_start() {
    PROXY_START_CANCELLED.store(true, Ordering::SeqCst);
}

fn start_cancelled() -> bool {
    PROXY_START_CANCELLED.load(Ordering::SeqCst)
}

pub fn set_proxy_enabled(enabled: bool) {
    let handle = proxy_enabled_handle().lock();
    {
        if let Some(ref flag) = *handle {
            flag.store(enabled, Ordering::Relaxed);
        }
    }
}

pub fn is_proxy_enabled() -> bool {
    let handle = proxy_enabled_handle().lock();
    {
        if let Some(ref flag) = *handle {
            return flag.load(Ordering::Relaxed);
        }
    }
    // Fail closed: with no flag registered there is no proxy to enable, and
    // reporting `true` here would silently mean "intercept everything".
    false
}

pub fn active_proxy_port() -> Option<u16> {
    match ACTIVE_PROXY_PORT.load(Ordering::SeqCst) {
        0 => None,
        port => Some(port),
    }
}

pub fn default_proxy_port() -> u16 {
    DEFAULT_PROXY_PORT.load(Ordering::SeqCst)
}

fn resolve_proxy_port(config: &ProxyConfig) -> Result<u16, String> {
    DEFAULT_PROXY_PORT.store(config.port, Ordering::SeqCst);

    if let Some(port) = active_proxy_port() {
        return Ok(port);
    }

    // Never signals the process holding the port: the caller gets the PID list
    // back and decides. `reuse` only means "fall through to the next free port".
    if let Err(error) = ensure_port_free(config.port, false) {
        if !config.reuse {
            return Err(error);
        }

        eprintln!(
            "[proxy] WARN: Could not free port {}: {}. Will try fallback ports.",
            config.port, error
        );
    }

    Ok(config.port)
}

fn bind_proxy_listener(
    runtime: &tokio::runtime::Runtime,
    bind_addr: IpAddr,
    preferred_port: u16,
    allow_fallback: bool,
) -> Result<(u16, TcpListener), String> {
    let max_attempts = if allow_fallback { 20 } else { 1 };

    for offset in 0..max_attempts {
        let Some(port) = preferred_port.checked_add(offset) else {
            break;
        };

        match runtime.block_on(TcpListener::bind(SocketAddr::new(bind_addr, port))) {
            Ok(listener) => {
                if port != preferred_port {
                    eprintln!(
                        "[proxy] WARN: Port {} is unavailable; using fallback port {}",
                        preferred_port, port
                    );
                }

                return Ok((port, listener));
            }
            Err(error) if error.kind() == io::ErrorKind::AddrInUse && allow_fallback => {
                eprintln!(
                    "[proxy] WARN: Port {} is still in use: {} ({:?})",
                    port, error, error
                );
            }
            Err(error) => {
                return Err(format!(
                    "Failed to bind port {}: {} ({:?})",
                    port, error, error
                ));
            }
        }
    }

    Err(format!(
        "Failed to bind proxy listener: no available port found from {} to {}",
        preferred_port,
        preferred_port.saturating_add(max_attempts.saturating_sub(1))
    ))
}

fn clear_proxy_runtime() {
    ACTIVE_PROXY_PORT.store(0, Ordering::SeqCst);

    let mut shutdown = proxy_shutdown_sender().lock();
    {
        *shutdown = None;
    }
    let mut handle = proxy_enabled_handle().lock();
    {
        *handle = None;
    }
}

pub fn stop() -> Result<(), String> {
    // Cancel any start that has not registered a shutdown sender yet (still
    // inside CA generation / bind). Without this, a timed-out start keeps
    // going and the next click fights a half-alive listener.
    cancel_proxy_start();

    let port = active_proxy_port();
    let Some(shutdown) = proxy_shutdown_sender().lock().take() else {
        ACTIVE_PROXY_PORT.store(0, Ordering::SeqCst);
        wait_for_proxy_thread_idle();
        return Ok(());
    };

    shutdown
        .send(())
        .map_err(|_| "Proxy shutdown signal could not be delivered".to_string())?;
    ACTIVE_PROXY_PORT.store(0, Ordering::SeqCst);
    wait_for_proxy_thread_idle();
    if let Some(port) = port {
        wait_port_releasable(port);
    }
    Ok(())
}

/// Block briefly until `run` clears its enabled handle after shutdown.
fn wait_for_proxy_thread_idle() {
    for _ in 0..60 {
        if proxy_enabled_handle().lock().is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Prove the previous listener released `port` so a restart can bind it.
fn wait_port_releasable(port: u16) {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    for _ in 0..40 {
        if std::net::TcpListener::bind(addr).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            port: 8888,
            reuse: false,
            tls_port: 8889,
            enabled: true,
            bind_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
        }
    }
}

/// Installs the process-level rustls crypto provider.
///
/// rustls refuses to guess when a build enables both `ring` and `aws-lc-rs`,
/// and which features end up enabled depends on the crate subset being built —
/// `cargo build -p halo` and `cargo build -p halo-cli` resolve differently, so
/// the proxy must pin the provider instead of relying on feature unification.
/// Idempotent: a second call is a no-op.
pub fn install_crypto_provider() {
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
}

pub fn run(config: ProxyConfig, app_handle: AppHandle) {
    eprintln!("[proxy] ========== Starting Halo Proxy ==========");
    install_crypto_provider();

    if let Some(port) = active_proxy_port() {
        eprintln!("[proxy] Proxy already running on port {}", port);
        return;
    }

    // Register shutdown early so stop/cancel during CA or bind can signal us.
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    {
        let mut shutdown = proxy_shutdown_sender().lock();
        if shutdown.is_some() {
            eprintln!("[proxy] Proxy is already running");
            return;
        }
        *shutdown = Some(shutdown_tx);
    }

    if abort_if_cancelled("before CA setup") {
        return;
    }

    ensure_ca_exists();

    if abort_if_cancelled("after CA setup") {
        return;
    }

    let bind_addr = resolve_bind_addr(config.bind_addr);

    let port = match resolve_proxy_port(&config) {
        Ok(port) => port,
        Err(e) => {
            eprintln!("[proxy] FATAL port {}: {}", config.port, e);
            clear_proxy_runtime();
            return;
        }
    };

    // Create AppHandler instance
    let handler = lifecycle::AppHandler::new(app_handle.clone());

    // Create CA authority
    let authority = match ca::create_proxy_authority() {
        Ok(auth) => auth,
        Err(e) => {
            eprintln!("[proxy] FATAL: Failed to create CA authority: {}", e);
            clear_proxy_runtime();
            return;
        }
    };

    if abort_if_cancelled("after CA authority") {
        return;
    }

    // Start proxy with tokio runtime (blocking)
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[proxy] FATAL: Failed to create tokio runtime: {}", e);
            clear_proxy_runtime();
            return;
        }
    };

    let (port, listener) = match bind_proxy_listener(&runtime, bind_addr, port, config.reuse) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("[proxy] FATAL: {}", error);
            clear_proxy_runtime();
            return;
        }
    };
    // The probe listener only proves the port is bindable; the crate re-binds
    // its own socket. The window between the two is small and the worst case
    // is a failed start, not a security issue.
    drop(listener);

    if abort_if_cancelled("before publishing port") {
        return;
    }

    ACTIVE_PROXY_PORT.store(port, Ordering::SeqCst);
    // Re-check after publish: stop/timeout may have raced the store above.
    if abort_if_cancelled("after publishing port") {
        return;
    }

    let socket_addr = SocketAddr::new(bind_addr, port);

    // Build proxy with hexbuffer-proxy
    let builder = ProxyBuilder::new()
        .with_addr(socket_addr)
        .with_ca(authority)
        .with_request_buffer_size(16384)
        .with_enabled(config.enabled)
        .add_http_handler(hexbuffer_proxy::decoder::DecodeHandler)
        .add_http_handler(handler.clone())
        .with_ws_handler(handler);

    let enabled_flag = builder.enabled_flag();
    {
        let mut handle = proxy_enabled_handle().lock();
        *handle = Some(enabled_flag);
    }

    let proxy = match builder.build() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[proxy] FATAL: Failed to build proxy: {}", e);
            clear_proxy_runtime();
            return;
        }
    };

    if abort_if_cancelled("before listen") {
        return;
    }

    eprintln!(
        "[proxy] Proxy listening on {} (HTTP and HTTPS MITM)",
        socket_addr
    );

    let status = ProxyRuntimeStatus {
        running: true,
        port: Some(port),
        default_port: default_proxy_port(),
        connections: 0,
    };
    let _ = app_handle.emit("proxy-status-changed", &status);

    runtime.block_on(async move {
        tokio::select! {
            res = proxy.start() => {
                if let Err(e) = res {
                    eprintln!("[proxy] Proxy stopped: {} ({:?})", e, e);
                }
            }
            _ = shutdown_rx => {
                eprintln!("[proxy] Graceful shutdown signal received");
            }
        }
    });
    clear_proxy_runtime();

    let stopped_status = ProxyRuntimeStatus {
        running: false,
        port: None,
        default_port: default_proxy_port(),
        connections: 0,
    };
    let _ = app_handle.emit("proxy-status-changed", &stopped_status);
}

fn abort_if_cancelled(stage: &str) -> bool {
    if !start_cancelled() {
        return false;
    }
    eprintln!("[proxy] Start cancelled {stage}");
    clear_proxy_runtime();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard: the proxy has no authentication and MITMs TLS, so it
    /// must never default to a network-reachable address.
    #[test]
    fn default_config_binds_loopback_only() {
        let config = ProxyConfig::default();
        assert!(
            config.bind_addr.is_loopback(),
            "proxy must default to loopback, got {}",
            config.bind_addr
        );
        assert!(!config.reuse, "port reuse must default to off");
    }

    /// `is_proxy_enabled` must fail closed: with no flag registered there is no
    /// proxy, and `true` here would mean "intercept everything".
    #[test]
    fn enabled_flag_fails_closed() {
        // Note: this only holds while no proxy has been started in-process.
        if proxy_enabled_handle().lock().is_none() {
            assert!(!is_proxy_enabled());
        }
    }
}

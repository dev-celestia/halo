//! Halo backend facade and runtime orchestrator.
//!
//! Subsystems are decomposed into focused crates:
//! - [`halo_core`] (`crates/core`): foundational security, paths, event bus, and types.
//! - [`halo_storage`] (`crates/storage`): SQLite database, repositories, chunked payload store, and history bridge.
//! - [`halo_traffic`] (`crates/traffic`): MITM proxy server, dynamic CA authority, and interception pipeline.
//! - [`halo_coder`] (`crates/coder`): editor persistence, git client, PTY terminal, backups, and AI harnesses.
//! - [`halo_crawler`] (`crates/crawler`): web spider runner, DOM/form extraction, and crawler automation.
//! - Tool crates: Intruder, Repeater, Mock Forge, Intercept, Port Scanner, Collaborator, Maintenance.

pub mod commands;

pub use collaborator;
pub use halo_core::automation;
pub use halo_core::fs_secure;
pub use halo_core::paths;
pub use halo_core::tauri;
pub use halo_crawler as crawler;
pub use halo_crawler as browser;
pub use halo_storage::db;
pub use halo_storage::history;
pub use halo_traffic as proxy;

pub use halo_core::paths::{get_shared_app_dir, get_shared_db_path};
pub use halo_core::types::*;
pub use halo_storage::db::repository::{
    ChronicleLogRecord, CollectionRecord, ContextRecord, DocumentRecord, HaloAgentMessageRecord,
    HaloAgentSessionRecord, NoteRecord, PaginatedResponse, StashEndpointRecord, StashRecord,
    TreeNode, TreePath,
};
pub use halo_storage::history::{
    HistoryBridge, ProxyLogSummary, WebSocketConnectionDetail, WebSocketConnectionSummary,
};
pub use halo_traffic::ca::export_ca_cert_pem;
pub use halo_traffic::state::ProxyState;
pub use halo_traffic::utils::ensure_port_free;
pub use halo_traffic::{
    ProxyConfig, active_proxy_port, begin_proxy_start, cancel_proxy_start, default_proxy_port, run,
    stop,
};

use std::sync::Arc;

use halo_core::tauri::{AppHandle, Manager};
use halo_storage::PayloadStore;
use halo_storage::db::repository::Database;

/// Boots managed proxy + SQLite history state on an [`AppHandle`].
///
/// This is the context every [`commands`] function takes. It owns no runtime:
/// callers supply the Tokio runtime (`NativeBackend` has one, `halo-cli` gets
/// one from `#[tokio::main]`), which is what lets the same command bodies serve
/// both the desktop app and a headless process.
pub struct BackendRuntime {
    pub app: AppHandle,
    pub proxy_state: Arc<ProxyState>,
    pub history: Arc<HistoryBridge>,
    /// TLS port of the running proxy. The proxy crate only tracks the HTTP
    /// port (`active_proxy_port`), so the requested HTTPS port lives here.
    pub tls_port: parking_lot::Mutex<Option<u16>>,
    /// Intruder attack id currently running, if any.
    pub active_attack: parking_lot::Mutex<Option<String>>,
}

impl BackendRuntime {
    pub fn boot() -> Result<Self, String> {
        let app = AppHandle::new();

        halo_core::paths::harden_app_data_permissions();
        halo_traffic::ca::harden_ca_permissions();
        maintenance::prune_runtime_storage();

        let proxy_state = ProxyState::new();
        let app_dir = get_shared_app_dir();
        let db_path = get_shared_db_path();
        let database = Database::new(db_path.clone())
            .map_err(|e| format!("open database {}: {e}", db_path.display()))?;
        database
            .init()
            .map_err(|e| format!("init database {}: {e}", db_path.display()))?;
        let payload_dir = app_dir.join("payloads");
        let payload_store = PayloadStore::new(payload_dir);
        let history = HistoryBridge::from_database_and_payload_store(database, payload_store);
        if history
            .list_http_sessions()
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            let _ = history.create_http_session(
                "Default",
                Some("Auto-created session"),
                Some("all"),
                None,
                None,
                Some("persistent"),
            );
        }

        app.manage(proxy_state);
        app.manage(history);
        app.manage(intruder::InvokerState::default());
        app.manage(port_scanner::PortScanState::default());
        app.manage(halo_crawler::CrawlerState::default());
        app.manage(halo_coder::pty::CoderPtyState::default());
        app.manage(halo_agent::HaloAgentState::default());

        Ok(Self {
            proxy_state: app.state::<ProxyState>().arc(),
            history: app.state::<HistoryBridge>().arc(),
            app,
            tls_port: parking_lot::Mutex::new(None),
            active_attack: parking_lot::Mutex::new(None),
        })
    }
}

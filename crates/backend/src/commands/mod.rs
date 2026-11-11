//! The single command surface.
//!
//! Every backend operation is a function here, taking [`BackendRuntime`] and an
//! ambient Tokio runtime. Both callers reach the same bodies:
//!
//! - the desktop app, through `halo_bridge::NativeBackend` (`tokio.block_on`);
//! - `halo-cli`, from `#[tokio::main]`.
//!
//! Nothing in this module may depend on GPUI — that is what keeps the CLI
//! headless.

pub mod history;
pub mod library;
pub mod notes;
pub mod port_scan;
pub mod proxy;
pub mod r2;
pub mod sessions;
pub mod settings;

// Groups whose command bodies already live in their feature crate.
pub use halo_agent;
pub use halo_coder::backup as coder_backup;
pub use halo_coder::editor as coder;
pub use halo_git as coder_git;
pub use halo_coder::pty as coder_pty;
pub use halo_coder::shell;

pub use halo_crawler::commands as browser;
pub use halo_crawler::commands as crawler;

pub use intercept;
pub use intercept::cert;
pub use intruder;
pub use maintenance as storage;
pub use mock_forge;
pub use port_scanner;
pub use repeater;

/// Runs a blocking SQLite/`HistoryBridge` call off the async runtime so slow
/// queries and VACUUMs can't stall Tokio workers or the UI event loop.
pub(crate) async fn run_blocking<T, F>(task: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    halo_core::tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| format!("background task failed: {}", e))?
}

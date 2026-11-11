//! Pi Coding Agent native JSON-L RPC harness driver.
//!
//! Spawns `pi --mode rpc` scoped to a workspace directory and communicates
//! via Newline-Delimited JSON (JSON-L) over stdin and stdout.
//!
//! Zero extra protocols or daemon layers — direct child process lifecycle.

pub mod models;
pub mod probe;
pub mod run;
pub mod types;

mod process;
mod protocol;
mod supervisor;

pub use models::{default_models, migrate_model_id};
pub use probe::{is_installed, list_models, probe_environment, resolve_executable};
pub use run::PiRun;
pub use types::{
    PiDoneStatus, PiEvent, PiImage, PiModelInfo, PiProbeResult, PiRunOptions, REQUIRED_VERSION,
};

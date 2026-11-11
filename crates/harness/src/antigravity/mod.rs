//! Google Antigravity ACP harness — `agy_acp_server` over JSON-RPC stdio.
//!
//! This is Halo's Antigravity harness, independent of Pi.
//! This is Google's official ACP registry agent (`antigravity-acp`).

mod auth;
mod install;
mod jsonrpc;
mod normalize;
mod probe;
mod run;
mod types;

pub use auth::{
    AntigravityAuthEvent, is_signed_in as auth_is_signed_in, logout as auth_logout,
    run_login as run_auth_login, token_path as auth_token_path,
};
pub use install::{PINNED_VERSION, ensure_installed};
pub use probe::{
    ENV_OVERRIDE, default_models, gemini_home, is_installed, probe_environment, resolve_executable,
};
pub use run::AntigravityRun;
pub use types::{
    AntigravityDoneStatus, AntigravityEvent, AntigravityModelInfo, AntigravityProbeResult,
    AntigravityRunOptions,
};

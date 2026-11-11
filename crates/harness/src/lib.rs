//! # halo-harness
//!
//! AI coding agent harnesses for Halo.
//!
//! Native drivers: Pi (`pi --mode rpc` JSON-L), OpenCode (`opencode serve` HTTP/SSE),
//! and Antigravity (`agy_acp_server` ACP stdio).

pub mod approval;
pub mod antigravity;
pub mod browser;
pub mod desktop;
pub mod mcp;
pub mod opencode;
pub mod pi;
pub mod usage;

pub use usage::{ContextMeter, context_window_for};

pub use mcp::{McpKeyValue, McpServerConfig, McpTransport, opencode_config_with_mcp};

pub use pi::{
    PiDoneStatus, PiEvent, PiImage, PiModelInfo, PiProbeResult, PiRun, PiRunOptions,
    REQUIRED_VERSION, default_models, is_installed, list_models as pi_list_models, migrate_model_id,
    probe_environment, resolve_executable,
};

pub use opencode::{
    OpenCodeDoneStatus, OpenCodeEvent, OpenCodeModelInfo, OpenCodeProbeResult, OpenCodeRun,
    OpenCodeRunOptions,
};
pub use opencode::{
    default_models as opencode_default_models, install_or_upgrade as opencode_install_or_upgrade,
    is_installed as opencode_is_installed, list_models as opencode_list_models,
    probe_environment as opencode_probe_environment,
    resolve_executable as opencode_resolve_executable,
};

pub use antigravity::{
    AntigravityAuthEvent, auth_is_signed_in as antigravity_acp_is_signed_in,
    auth_logout as antigravity_acp_logout, run_auth_login as antigravity_acp_run_login,
};
pub use antigravity::{
    AntigravityDoneStatus, AntigravityEvent, AntigravityModelInfo, AntigravityProbeResult,
    AntigravityRun, AntigravityRunOptions,
};
pub use antigravity::{
    default_models as antigravity_default_models, ensure_installed as antigravity_ensure_installed,
    is_installed as antigravity_is_installed, probe_environment as antigravity_probe_environment,
    resolve_executable as antigravity_resolve_executable,
};

//! OpenCode native harness — `opencode serve` over HTTP + SSE.
//!
//! Deliberately not ACP (`opencode acp`): the ACP layer settles turns on
//! uncorrelated idle events.

mod discover;
mod install;
mod probe;
mod run;
mod sse;
mod types;

pub use discover::discover_skills;
pub use install::install_or_upgrade;
pub use probe::{
    MIN_VERSION, STATE_BLOCKED_MARKER, default_models, is_installed, list_models, parse_semver,
    probe_environment, resolve_executable,
};
pub use run::OpenCodeRun;
pub use types::{
    OpenCodeDoneStatus, OpenCodeEvent, OpenCodeModelInfo, OpenCodeProbeResult, OpenCodeRunOptions,
    OpenCodeSkill, model_ref,
};

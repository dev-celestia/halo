//! Boots the headless backend and carries the output preference.

use anyhow::Context as _;
use halo_backend::BackendRuntime;

pub struct Ctx {
    pub runtime: BackendRuntime,
    pub json: bool,
}

impl Ctx {
    pub fn boot(json: bool) -> anyhow::Result<Self> {
        let runtime = BackendRuntime::boot().map_err(anyhow::Error::msg)?;
        Ok(Self { runtime, json })
    }

    pub fn ctx(&self) -> &BackendRuntime {
        &self.runtime
    }
}

/// Directory for CLI-owned runtime files (the detached-proxy state file). Kept
/// beside the app data so `storage reset` and this live under one root.
pub fn run_dir() -> anyhow::Result<std::path::PathBuf> {
    let dir = halo_backend::paths::get_shared_app_dir().join("cli");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

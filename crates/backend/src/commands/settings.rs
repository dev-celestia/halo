//! App-wide settings: storage maintenance and the MITM CA.

use maintenance::{DeleteArtifactResult, ResetLocalDataResult, StorageInfo};

use crate::BackendRuntime;
use crate::commands::proxy;

pub fn get_storage_info(ctx: &BackendRuntime) -> Result<StorageInfo, String> {
    maintenance::get_storage_info(&ctx.history)
}

pub fn delete_storage_artifact(
    ctx: &BackendRuntime,
    artifact: &str,
) -> Result<DeleteArtifactResult, String> {
    maintenance::delete_storage_artifact(&ctx.history, artifact)
}

/// Stops the proxy first — deleting the DB under a live listener leaves the
/// proxy writing into a file that no longer exists.
pub async fn reset_all_app_data(ctx: &BackendRuntime) -> Result<ResetLocalDataResult, String> {
    let _ = proxy::stop_proxy(ctx).await;
    maintenance::reset_all_app_data(&ctx.history)
}

// ── CA certificate ─────────────────────────────────────────────────

pub fn get_ca_cert() -> Result<String, String> {
    intercept::cert::get_ca_cert()
}

pub fn export_ca_cert(path: &str) -> Result<String, String> {
    intercept::cert::export_ca_cert_to(path).map(|p| p.display().to_string())
}

pub fn regenerate_ca_cert() -> Result<(), String> {
    intercept::cert::regenerate_ca_cert()
}

pub fn trust_ca() -> Result<String, String> {
    intercept::cert::trust_ca()
}

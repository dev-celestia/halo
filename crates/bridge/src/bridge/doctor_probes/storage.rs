//! Database and payload-storage health probe.

use super::*;

pub(crate) fn probe_storage_health(backend: &dyn Backend) -> DoctorItem {
    let info = backend.get_storage_info().ok();
    let (status, version, path, hint) = if let Some(info) = info {
        let total_bytes = info.database_size_bytes
            + info.crawler_artifacts_size_bytes
            + info.regression_artifacts_size_bytes
            + info.log_file_size_bytes;
        let size_mb = total_bytes as f64 / (1024.0 * 1024.0);
        let db_mb = info.database_size_bytes as f64 / (1024.0 * 1024.0);
        (
            DoctorStatus::Ready,
            Some(format!("{size_mb:.1} MB (DB: {db_mb:.1} MB)")),
            Some(info.database_path.clone()),
            format!(
                "Database operational ({db_mb:.1} MB at {}), hot-exit backup store ready.",
                info.database_path
            ),
        )
    } else {
        (
            DoctorStatus::Missing,
            None,
            None,
            "Could not read local SQLite database status.".to_string(),
        )
    };

    DoctorItem {
        id: "system:storage".to_string(),
        category: DoctorCategory::System,
        name: "Database & Session Storage".to_string(),
        description: "SQLite traffic database, local history, and session state persistence"
            .to_string(),
        command: "sqlite3".to_string(),
        status,
        version,
        resolved_path: path,
        hint,
        can_auto_install: true,
        action_label: "Optimize DB".to_string(),
        doc_url: None,
    }
}

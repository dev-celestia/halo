//! Storage info, CA management, and notes accessors.

use super::*;

impl BackendStorage for NativeBackend {
    fn get_storage_info(&self) -> BridgeResult<StorageInfo> {
        let info = halo_backend::commands::storage::get_storage_info(&self.runtime.history)
            .map_err(BridgeError::message)?;
        Ok(StorageInfo {
            app_data_dir: info.app_data_dir,
            database_path: info.database_path,
            crawler_artifacts_path: info.crawler_artifacts_path,
            database_size_bytes: info.database_size_bytes,
            crawler_artifacts_size_bytes: info.crawler_artifacts_size_bytes,
            regression_artifacts_size_bytes: info.regression_artifacts_size_bytes,
            log_file_size_bytes: info.log_file_size_bytes,
        })
    }

    fn delete_storage_artifact(&self, artifact: &str) -> BridgeResult<DeleteArtifactResult> {
        let result = halo_backend::commands::storage::delete_storage_artifact(
            &self.runtime.history,
            artifact,
        )
        .map_err(BridgeError::message)?;
        Ok(DeleteArtifactResult {
            bytes_deleted: result.bytes_deleted,
            label: result.label,
        })
    }

    fn reset_all_app_data(&self) -> BridgeResult<ResetLocalDataResult> {
        let _ = self.stop_proxy();
        let result = halo_backend::commands::storage::reset_all_app_data(&self.runtime.history)
            .map_err(BridgeError::message)?;
        Ok(ResetLocalDataResult {
            artifact_dir: result.artifact_dir,
            files_deleted: result.files_deleted,
            bytes_deleted: result.bytes_deleted,
            pages_updated: result.pages_updated,
            intercept_browser_profile_removed: result.intercept_browser_profile_removed,
            ca_file_removed: result.ca_file_removed,
        })
    }

    fn get_ca_cert(&self) -> BridgeResult<String> {
        halo_backend::commands::cert::get_ca_cert().map_err(BridgeError::message)
    }

    fn export_ca_cert(&self, path: &str) -> BridgeResult<String> {
        let path =
            halo_backend::commands::cert::export_ca_cert_to(path).map_err(BridgeError::message)?;
        Ok(path.display().to_string())
    }

    fn regenerate_ca_cert(&self) -> BridgeResult<()> {
        halo_backend::commands::cert::regenerate_ca_cert().map_err(BridgeError::message)
    }

    fn trust_ca(&self) -> BridgeResult<String> {
        halo_backend::commands::cert::trust_ca().map_err(BridgeError::message)
    }

    // ── Notes ──────────────────────────────────────────────────────────────
    fn notes_list(&self) -> BridgeResult<Vec<NoteRecord>> {
        self.runtime
            .history
            .list_notes()
            .map_err(BridgeError::message)
    }

    fn note_upsert(&self, note: &NoteRecord) -> BridgeResult<NoteRecord> {
        self.runtime
            .history
            .upsert_note(note)
            .map_err(BridgeError::message)
    }

    fn note_delete(&self, id: &str) -> BridgeResult<()> {
        self.runtime
            .history
            .delete_note(id)
            .map(|_| ())
            .map_err(BridgeError::message)
    }

    fn notes_delete(&self, ids: &[String]) -> BridgeResult<usize> {
        self.runtime
            .history
            .delete_notes(ids)
            .map_err(BridgeError::message)
    }

    fn notes_import(&self, notes: &[NoteRecord]) -> BridgeResult<usize> {
        self.runtime
            .history
            .import_notes(notes)
            .map_err(BridgeError::message)
    }
}

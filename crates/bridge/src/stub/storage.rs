//! Stub storage info, CA, and notes accessors.

use super::*;

impl BackendStorage for StubBackend {
    fn get_storage_info(&self) -> BridgeResult<StorageInfo> {
        Ok(StorageInfo {
            app_data_dir: "/tmp/halo-stub".into(),
            database_path: "/tmp/halo-stub/halo.db".into(),
            crawler_artifacts_path: "/tmp/halo-stub/crawler-artifacts".into(),
            database_size_bytes: 1_024 * 256,
            crawler_artifacts_size_bytes: 0,
            regression_artifacts_size_bytes: 0,
            log_file_size_bytes: 128,
        })
    }

    fn delete_storage_artifact(&self, artifact: &str) -> BridgeResult<DeleteArtifactResult> {
        let label = match artifact {
            "database" => "SQL Database & Payloads",
            "log_file" => "Log File",
            "crawler_artifacts" | "browser_artifacts" => "Crawler Artifacts",
            "regression_artifacts" => "Regression Artifacts",
            other => return Err(BridgeError::message(format!("Unknown artifact: {other}"))),
        };
        Ok(DeleteArtifactResult {
            bytes_deleted: 0,
            label: label.into(),
        })
    }

    fn reset_all_app_data(&self) -> BridgeResult<ResetLocalDataResult> {
        let _ = self.stop_proxy();
        lock_err(self.history.lock()).clear();
        Ok(ResetLocalDataResult {
            artifact_dir: "/tmp/halo-stub/crawler-artifacts".into(),
            files_deleted: 0,
            bytes_deleted: 0,
            pages_updated: 0,
            intercept_browser_profile_removed: false,
            ca_file_removed: false,
        })
    }

    fn get_ca_cert(&self) -> BridgeResult<String> {
        Ok("-----BEGIN CERTIFICATE-----\nSTUB\n-----END CERTIFICATE-----\n".into())
    }

    fn export_ca_cert(&self, path: &str) -> BridgeResult<String> {
        Ok(path.to_string())
    }

    fn regenerate_ca_cert(&self) -> BridgeResult<()> {
        Ok(())
    }

    fn trust_ca(&self) -> BridgeResult<String> {
        Ok("Stub: CA trust skipped".into())
    }

    // ── Notes ──────────────────────────────────────────────────────────────
    fn notes_list(&self) -> BridgeResult<Vec<NoteRecord>> {
        Ok(lock_err(self.notes.lock()).clone())
    }

    fn note_upsert(&self, note: &NoteRecord) -> BridgeResult<NoteRecord> {
        let mut list = lock_err(self.notes.lock());
        if let Some(existing) = list.iter_mut().find(|n| n.id == note.id) {
            *existing = note.clone();
            Ok(existing.clone())
        } else {
            list.push(note.clone());
            Ok(note.clone())
        }
    }

    fn note_delete(&self, id: &str) -> BridgeResult<()> {
        let mut list = lock_err(self.notes.lock());
        list.retain(|n| n.id != id);
        Ok(())
    }

    fn notes_delete(&self, ids: &[String]) -> BridgeResult<usize> {
        let mut list = lock_err(self.notes.lock());
        let before = list.len();
        list.retain(|n| !ids.contains(&n.id));
        Ok(before - list.len())
    }

    fn notes_import(&self, notes: &[NoteRecord]) -> BridgeResult<usize> {
        let mut list = lock_err(self.notes.lock());
        let mut imported = 0;
        for note in notes {
            if !list.iter().any(|n| n.id == note.id) {
                list.push(note.clone());
                imported += 1;
            }
        }
        Ok(imported)
    }
}

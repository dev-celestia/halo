//! Coder IDE filesystem, tabs, settings, agent catalogs, backups, and shell layout.

use super::*;

impl BackendCoder for NativeBackend {
    fn coder_list_dir(&self, path: &str) -> BridgeResult<Vec<crate::work::CoderDirEntry>> {
        halo_backend::commands::coder::list_dir(path)
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|e| crate::work::CoderDirEntry {
                        name: e.name,
                        path: e.path,
                        is_dir: e.is_dir,
                    })
                    .collect()
            })
            .map_err(BridgeError::message)
    }

    fn coder_read_file(&self, path: &str) -> BridgeResult<String> {
        halo_backend::commands::coder::read_file(path).map_err(BridgeError::message)
    }

    fn coder_ensure_image_file(&self, path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::ensure_image_file(path).map_err(BridgeError::message)
    }

    fn coder_write_file(&self, path: &str, contents: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::write_file(path, contents).map_err(BridgeError::message)
    }

    fn coder_create_file(&self, path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::create_file(path).map_err(BridgeError::message)
    }

    fn coder_create_dir(&self, path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::create_dir(path).map_err(BridgeError::message)
    }

    fn coder_rename_path(&self, from: &str, to: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::rename_path(from, to).map_err(BridgeError::message)
    }

    fn coder_delete_path(&self, path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::delete_path(path).map_err(BridgeError::message)
    }

    fn coder_trash_path(&self, path: &str) -> BridgeResult<String> {
        halo_backend::commands::coder::move_to_trash(path).map_err(BridgeError::message)
    }

    fn coder_purge_trash(&self, trash_path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::purge_trash_entry(trash_path).map_err(BridgeError::message)
    }

    fn coder_copy_path(&self, from: &str, to: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::copy_path(from, to).map_err(BridgeError::message)
    }

    fn coder_unique_sibling(&self, path: &str) -> BridgeResult<String> {
        halo_backend::commands::coder::unique_sibling_path(path).map_err(BridgeError::message)
    }

    fn coder_saved_root(&self) -> BridgeResult<Option<String>> {
        let prefs = halo_backend::commands::coder::load_prefs();
        let Some(root) = prefs.root_path else {
            return Ok(None);
        };
        if std::path::Path::new(&root).is_dir() {
            Ok(Some(root))
        } else {
            Ok(None)
        }
    }

    fn coder_save_root(&self, root_path: Option<&str>) -> BridgeResult<()> {
        halo_backend::commands::coder::save_root_path(root_path).map_err(BridgeError::message)
    }

    fn coder_recent_projects(&self) -> BridgeResult<Vec<String>> {
        Ok(halo_backend::commands::coder::recent_projects())
    }

    fn coder_remove_recent_project(&self, path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder::remove_recent_project(path).map_err(BridgeError::message)
    }

    fn coder_load_open_tabs(
        &self,
        root: &str,
    ) -> BridgeResult<halo_backend::commands::coder::CoderPersistedTabs> {
        Ok(halo_backend::commands::coder::load_open_tabs(root))
    }

    fn coder_save_open_tabs(
        &self,
        root: &str,
        tabs: &halo_backend::commands::coder::CoderPersistedTabs,
    ) -> BridgeResult<()> {
        halo_backend::commands::coder::save_open_tabs(root, tabs).map_err(BridgeError::message)
    }

    fn coder_load_settings(
        &self,
    ) -> BridgeResult<Option<halo_backend::commands::coder::CoderPersistedSettings>> {
        Ok(halo_backend::commands::coder::load_settings())
    }

    fn coder_save_settings(
        &self,
        settings: &halo_backend::commands::coder::CoderPersistedSettings,
    ) -> BridgeResult<()> {
        halo_backend::commands::coder::save_settings(settings).map_err(BridgeError::message)
    }

    fn coder_load_agent_selection(&self) -> BridgeResult<(Option<String>, Option<String>)> {
        Ok(halo_backend::commands::coder::load_agent_selection())
    }

    fn coder_load_agent_models(&self) -> BridgeResult<std::collections::HashMap<String, String>> {
        Ok(halo_backend::commands::coder::load_agent_models())
    }

    fn coder_save_agent_selection(&self, harness: &str, model: Option<&str>) -> BridgeResult<()> {
        halo_backend::commands::coder::save_agent_selection(harness, model)
            .map_err(BridgeError::message)
    }

    fn coder_load_agent_model_catalogs(
        &self,
    ) -> BridgeResult<(
        std::collections::HashMap<String, Vec<halo_backend::commands::halo_agent::ModelInfo>>,
        std::collections::HashMap<String, u64>,
    )> {
        Ok(halo_backend::commands::coder::load_agent_model_catalogs())
    }

    fn coder_save_agent_model_catalog(
        &self,
        harness: &str,
        models: &[halo_backend::commands::halo_agent::ModelInfo],
    ) -> BridgeResult<()> {
        halo_backend::commands::coder::save_agent_model_catalog(harness, models)
            .map_err(BridgeError::message)
    }

    fn coder_write_backup(&self, workspace: &str, path: &str, content: &str) -> BridgeResult<()> {
        halo_backend::commands::coder_backup::write_backup(workspace, path, content)
            .map_err(BridgeError::message)
    }

    fn coder_delete_backup(&self, workspace: &str, path: &str) -> BridgeResult<()> {
        halo_backend::commands::coder_backup::delete_backup(workspace, path)
            .map_err(BridgeError::message)
    }

    fn coder_load_backup(
        &self,
        workspace: &str,
        path: &str,
        disk_content: &str,
    ) -> BridgeResult<Option<halo_backend::commands::coder_backup::BackupRestore>> {
        halo_backend::commands::coder_backup::load_backup(workspace, path, disk_content)
            .map_err(BridgeError::message)
    }

    fn coder_push_history(&self, path: &str, previous_content: &str) -> BridgeResult<()> {
        halo_backend::commands::coder_backup::push_history(path, previous_content)
            .map_err(BridgeError::message)
    }

    fn coder_list_history(
        &self,
        path: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_backup::HistoryEntry>> {
        halo_backend::commands::coder_backup::list_history(path).map_err(BridgeError::message)
    }

    fn coder_read_history(&self, path: &str, id: &str) -> BridgeResult<String> {
        halo_backend::commands::coder_backup::read_history(path, id).map_err(BridgeError::message)
    }

    fn shell_load_layout(&self) -> BridgeResult<halo_backend::commands::shell::ShellLayout> {
        Ok(halo_backend::commands::shell::load_layout())
    }

    fn shell_save_layout(
        &self,
        layout: &halo_backend::commands::shell::ShellLayout,
    ) -> BridgeResult<()> {
        halo_backend::commands::shell::save_layout(layout).map_err(BridgeError::message)
    }
}

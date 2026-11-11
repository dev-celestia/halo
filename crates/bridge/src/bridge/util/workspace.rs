//! Coder workspace snapshots: project detection, listing conversion, tab bookkeeping.

use super::*;

pub(crate) fn coder_detect_project_kind(root: &str) -> Option<crate::work::CoderProjectKind> {
    let root = std::path::Path::new(root);
    let has_rust = root.join("Cargo.toml").is_file();
    let has_node = root.join("package.json").is_file();
    match (has_rust, has_node) {
        (true, true) => Some(crate::work::CoderProjectKind::Both),
        (true, false) => Some(crate::work::CoderProjectKind::Rust),
        (false, true) => Some(crate::work::CoderProjectKind::Node),
        (false, false) => None,
    }
}

pub(crate) fn coder_work_from_listing(
    root: String,
    entries: Vec<crate::work::CoderDirEntry>,
) -> CoderWork {
    let project_kind = coder_detect_project_kind(&root);
    CoderWork {
        root_path: Some(root),
        project_kind,
        tree: entries
            .into_iter()
            .map(|e| CoderTreeEntry {
                path: e.path,
                name: e.name,
                is_dir: e.is_dir,
                children: None,
            })
            .collect(),
        ..CoderWork::default()
    }
}

/// Copy Agent chat/rail surface across project open/close so sessions survive
/// workspace switches (Agent sidebar groups chats by `session.root_path`).
pub(crate) fn coder_copy_agent_surface(dst: &mut CoderWork, src: &CoderWork) {
    dst.agent_running = src.agent_running;
    dst.agent_run_id = src.agent_run_id.clone();
    dst.agent_harness = src.agent_harness.clone();
    dst.agent_model = src.agent_model.clone();
    // Without the per-harness picks, the first catalog landing after a project
    // switch would replace the selection with the catalog's first row.
    dst.agent_model_by_harness = src.agent_model_by_harness.clone();
    dst.agent_harness_installed = src.agent_harness_installed;
    dst.agent_pi_setup_hint = src.agent_pi_setup_hint.clone();
    dst.agent_pi_needs_upgrade = src.agent_pi_needs_upgrade;
    dst.agent_state_blocked = src.agent_state_blocked;
    dst.agent_installing = src.agent_installing.clone();
    dst.agent_install_error = src.agent_install_error.clone();
    dst.agent_harness_catalog = src.agent_harness_catalog.clone();
    dst.agent_harness_readiness = src.agent_harness_readiness.clone();
    dst.agent_harness_readiness_loading = src.agent_harness_readiness_loading;
    dst.agent_harness_readiness_at = src.agent_harness_readiness_at;
    dst.agent_harness_readiness_epoch = src.agent_harness_readiness_epoch;
    dst.agent_provider_sheet_open = src.agent_provider_sheet_open;
    dst.agent_provider_sheet_dismissed = src.agent_provider_sheet_dismissed;
    dst.agent_install_error_harness = src.agent_install_error_harness.clone();
    dst.agent_model_catalog = src.agent_model_catalog.clone();
    // Dropping the per-harness cache made every project switch re-probe
    // (`pi --list-models` / `opencode models`) and flash the picker's
    // "Loading … models" skeleton for several seconds.
    dst.agent_model_cache = src.agent_model_cache.clone();
    dst.agent_catalog_fetched_at = src.agent_catalog_fetched_at.clone();
    dst.agent_catalog_loading = src.agent_catalog_loading;
    dst.agent_catalog_refresh_epoch = src.agent_catalog_refresh_epoch;
    dst.agent_project_loading = src.agent_project_loading.clone();
    dst.agent_project_switch_epoch = src.agent_project_switch_epoch;
    dst.agent_sessions = src.agent_sessions.clone();
    dst.agent_active_session_id = src.agent_active_session_id.clone();
    dst.agent_history_open = src.agent_history_open;
    dst.agent_rail_collapsed = src.agent_rail_collapsed.clone();
    dst.agent_context_open = src.agent_context_open;
    dst.agent_skills = src.agent_skills.clone();
    dst.agent_skills_loading = src.agent_skills_loading;
    dst.agent_skills_error = src.agent_skills_error.clone();
    dst.agent_skills_epoch = src.agent_skills_epoch;
    dst.agent_enabled_skills = src.agent_enabled_skills.clone();
    dst.agent_enabled_mcp = src.agent_enabled_mcp.clone();
    dst.agent_browser = src.agent_browser.clone();
    dst.agent_browser_loading = src.agent_browser_loading;
    dst.agent_desktop = src.agent_desktop.clone();
    dst.agent_desktop_loading = src.agent_desktop_loading;
    dst.agent_dock_todo_open = src.agent_dock_todo_open;
    dst.agent_dock_queue_open = src.agent_dock_queue_open;
    dst.agent_dock_pending_id = src.agent_dock_pending_id.clone();
    dst.agent_messages = src.agent_messages.clone();
    dst.agent_streaming_part = src.agent_streaming_part.clone();
    dst.agent_draft = src.agent_draft.clone();
    dst.agent_todos = src.agent_todos.clone();
    dst.agent_todo_expanded = src.agent_todo_expanded;
    dst.agent_todo_dismissed = src.agent_todo_dismissed;
    dst.agent_context_expanded = src.agent_context_expanded.clone();
    dst.agent_queue = src.agent_queue.clone();
    dst.agent_file_changes = src.agent_file_changes.clone();
    dst.agent_picker_open = src.agent_picker_open;
    dst.agent_picker_filter = src.agent_picker_filter.clone();
    dst.agent_recent_models = src.agent_recent_models.clone();
    dst.agent_approval_picker_open = src.agent_approval_picker_open;
    dst.agent_plan_mode = src.agent_plan_mode;
    dst.agent_approval_mode = src.agent_approval_mode;
    dst.agent_completion = src.agent_completion.clone();
    dst.agent_attachments = src.agent_attachments.clone();
    dst.agent_pending_input = src.agent_pending_input.clone();
    dst.agent_subagents = src.agent_subagents.clone();
    dst.agent_open_subagent = src.agent_open_subagent.clone();
    dst.agent_context_usage = src.agent_context_usage.clone();
}

/// Activate `path` in the given editor pane.
pub(crate) fn coder_activate_in_group(coder: &mut CoderWork, path: &str, group: u8) {
    let pane_id = if coder.layout.contains_leaf(group) {
        group
    } else {
        coder.focused_pane
    };
    coder.seed_primary_tabs_if_empty();
    if let Some(pane) = coder.pane_mut(pane_id) {
        if !pane.tabs.iter().any(|p| p == path) {
            pane.tabs.push(path.to_string());
        }
        pane.active = Some(path.to_string());
    }
    coder.focused_pane = pane_id;
}

/// Drop `path` from a pane's tab strip. Returns the next active path for that pane.
pub(crate) fn coder_remove_tab_from_group(
    coder: &mut CoderWork,
    path: &str,
    group: u8,
) -> Option<String> {
    let pane_id = group;
    let (next, empty) = if let Some(pane) = coder.pane_mut(pane_id) {
        pane.tabs.retain(|p| p != path);
        if pane.active.as_deref() == Some(path) {
            pane.active = pane.tabs.last().cloned();
        }
        (pane.active.clone(), pane.tabs.is_empty())
    } else {
        (None, true)
    };

    if empty && coder.is_split() {
        let _ = coder.close_pane(pane_id);
    }

    coder.prune_orphan_open_files();
    next
}

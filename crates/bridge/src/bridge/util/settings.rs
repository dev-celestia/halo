//! Coder settings and persisted-tab conversions.

use super::*;

/// Map on-disk IDE settings into runtime [`CoderSettings`].
///
/// Empty `font_id` / `shell` fall back to the same defaults the UI uses when
/// prefs have never been written.
pub(crate) fn coder_settings_from_persisted(
    persisted: halo_backend::commands::coder::CoderPersistedSettings,
) -> CoderSettings {
    let mut settings = CoderSettings::default();
    if !persisted.font_id.trim().is_empty() {
        settings.font_id = persisted.font_id;
    }
    settings.font_size = persisted.font_size;
    settings.tab_size = persisted.tab_size;
    settings.word_wrap = persisted.word_wrap;
    settings.line_numbers = persisted.line_numbers;
    settings.minimap = persisted.minimap;
    settings.autosave = persisted.autosave;
    settings.trim_trailing_whitespace = persisted.trim_trailing_whitespace;
    settings.format_on_save = persisted.format_on_save;
    settings.line_ending = match persisted.line_ending.as_str() {
        "crlf" => CoderLineEnding::Crlf,
        _ => CoderLineEnding::Lf,
    };
    if !persisted.shell.trim().is_empty() {
        settings.shell = persisted.shell;
    }
    settings.terminal_font_size = persisted.terminal_font_size;
    settings.scrollback_lines = persisted.scrollback_lines;
    settings.confirm_close_running = persisted.confirm_close_running;
    settings.default_cwd = persisted.default_cwd.trim().to_string();
    settings.agent_approval_mode = match persisted
        .agent_approval_mode
        .as_deref()
        .and_then(crate::AgentApprovalMode::parse)
    {
        Some(mode) => mode,
        None if persisted.agent_auto_approve => crate::AgentApprovalMode::FullAccess,
        None => crate::AgentApprovalMode::Ask,
    };
    settings.agent_sound_enabled = persisted.agent_sound_enabled;
    settings.agent_max_turns = persisted.agent_max_turns;
    settings.agent_include_open_files = persisted.agent_include_open_files;
    settings.agent_include_git_diff = persisted.agent_include_git_diff;
    settings.computer_use = persisted.computer_use;
    settings.computer_use_headed = persisted.computer_use_headed;
    settings.pi_executable_path = persisted.pi_executable_path;
    settings.opencode_executable_path = persisted.opencode_executable_path;
    settings.antigravity_executable_path = persisted.antigravity_executable_path;
    settings.mcp_servers = persisted.mcp_servers;
    settings.anthropic_api_key = persisted.anthropic_api_key;
    settings.openai_api_key = persisted.openai_api_key;
    settings.gemini_api_key = persisted.gemini_api_key;
    settings.deepseek_api_key = persisted.deepseek_api_key;
    settings.ollama_base_url = persisted.ollama_base_url;
    settings.sign_off = persisted.sign_off;
    settings.auto_fetch_minutes = persisted.auto_fetch_minutes;
    settings.confirm_force_push = persisted.confirm_force_push;
    settings.confirm_system_changes = persisted.confirm_system_changes;
    settings.restore_last_project = persisted.restore_last_project;
    settings.show_hidden_files = persisted.show_hidden_files;
    settings.ignore_globs = persisted.ignore_globs;
    settings
}

pub(crate) fn coder_settings_to_persisted(
    settings: &CoderSettings,
) -> halo_backend::commands::coder::CoderPersistedSettings {
    halo_backend::commands::coder::CoderPersistedSettings {
        font_id: settings.font_id.clone(),
        font_size: settings.font_size,
        tab_size: settings.tab_size,
        word_wrap: settings.word_wrap,
        line_numbers: settings.line_numbers,
        minimap: settings.minimap,
        autosave: settings.autosave,
        trim_trailing_whitespace: settings.trim_trailing_whitespace,
        format_on_save: settings.format_on_save,
        line_ending: settings.line_ending.id().into(),
        shell: settings.shell.clone(),
        terminal_font_size: settings.terminal_font_size,
        scrollback_lines: settings.scrollback_lines,
        confirm_close_running: settings.confirm_close_running,
        default_cwd: settings.default_cwd.clone(),
        agent_approval_mode: Some(settings.agent_approval_mode.id().into()),
        agent_auto_approve: matches!(
            settings.agent_approval_mode,
            crate::AgentApprovalMode::FullAccess
        ),
        agent_sound_enabled: settings.agent_sound_enabled,
        agent_max_turns: settings.agent_max_turns,
        agent_include_open_files: settings.agent_include_open_files,
        agent_include_git_diff: settings.agent_include_git_diff,
        computer_use: settings.computer_use,
        computer_use_headed: settings.computer_use_headed,
        pi_executable_path: settings.pi_executable_path.clone(),
        opencode_executable_path: settings.opencode_executable_path.clone(),
        antigravity_executable_path: settings.antigravity_executable_path.clone(),
        mcp_servers: settings.mcp_servers.clone(),
        anthropic_api_key: settings.anthropic_api_key.clone(),
        openai_api_key: settings.openai_api_key.clone(),
        gemini_api_key: settings.gemini_api_key.clone(),
        deepseek_api_key: settings.deepseek_api_key.clone(),
        ollama_base_url: settings.ollama_base_url.clone(),
        sign_off: settings.sign_off,
        auto_fetch_minutes: settings.auto_fetch_minutes,
        confirm_force_push: settings.confirm_force_push,
        confirm_system_changes: settings.confirm_system_changes,
        restore_last_project: settings.restore_last_project,
        show_hidden_files: settings.show_hidden_files,
        ignore_globs: settings.ignore_globs.clone(),
    }
}

pub(crate) fn coder_tabs_snapshot(
    coder: &CoderWork,
) -> halo_backend::commands::coder::CoderPersistedTabs {
    use halo_backend::commands::coder::{
        CoderPersistedLayout, CoderPersistedPane, CoderPersistedTabs,
    };

    fn layout_to_persisted(layout: &CoderLayout) -> CoderPersistedLayout {
        match layout {
            CoderLayout::Leaf(id) => CoderPersistedLayout::Leaf { id: *id },
            CoderLayout::Split {
                axis,
                first,
                second,
            } => CoderPersistedLayout::Split {
                axis: match axis {
                    CoderSplitAxis::Horizontal => "horizontal".into(),
                    CoderSplitAxis::Vertical => "vertical".into(),
                },
                first: Box::new(layout_to_persisted(first)),
                second: Box::new(layout_to_persisted(second)),
            },
        }
    }

    let panes: Vec<CoderPersistedPane> = coder
        .panes
        .iter()
        .map(|p| CoderPersistedPane {
            id: p.id,
            tabs: p.tabs.clone(),
            active: p.active.clone(),
        })
        .collect();

    // Keep legacy fields populated from pane 0 / pane 1 for older readers.
    let primary = coder.pane(0);
    let secondary = coder.pane(1);
    CoderPersistedTabs {
        primary_tabs: primary.map(|p| p.tabs.clone()).unwrap_or_default(),
        active_path: primary.and_then(|p| p.active.clone()),
        secondary_tabs: secondary.map(|p| p.tabs.clone()).unwrap_or_default(),
        secondary_active: secondary.and_then(|p| p.active.clone()),
        split_axis: match &coder.layout {
            CoderLayout::Split {
                axis: CoderSplitAxis::Horizontal,
                ..
            } => Some("horizontal".into()),
            CoderLayout::Split {
                axis: CoderSplitAxis::Vertical,
                ..
            } => Some("vertical".into()),
            CoderLayout::Leaf(_) => None,
        },
        focused_group: coder.focused_pane.min(1),
        panes,
        layout: Some(layout_to_persisted(&coder.layout)),
        focused_pane: coder.focused_pane,
    }
}

/// Load a path into an open-file buffer (text or image). Skips missing paths.
///
/// When `workspace` is set, a hot-exit backup overlays disk content if present.
/// The bool is true when a backup was restored and the on-disk file also changed.
pub(crate) fn coder_load_open_file_entry(
    backend: &dyn Backend,
    path: &str,
    workspace: Option<&str>,
) -> Option<(CoderOpenFile, bool)> {
    if !std::path::Path::new(path).is_file() {
        return None;
    }
    if coder_is_image_path(path) {
        if backend.coder_ensure_image_file(path).is_err() {
            return None;
        }
        return Some((
            CoderOpenFile {
                path: path.to_string(),
                kind: CoderFileKind::Image,
                baseline: String::new(),
                content: String::new(),
                dirty: false,
                zoom_pct: 100,
            },
            false,
        ));
    }
    let disk = backend.coder_read_file(path).ok()?;
    let (content, dirty, disk_changed) =
        super::coder_backup::coder_apply_backup_restore(backend, workspace, path, &disk);
    Some((
        CoderOpenFile {
            path: path.to_string(),
            kind: CoderFileKind::Text,
            baseline: disk,
            content,
            dirty,
            zoom_pct: 100,
        },
        disk_changed,
    ))
}

/// Prefer `README.md` at the project root when no editor tabs are open.
pub(crate) fn coder_seed_default_open_file(coder: &mut CoderWork, backend: &dyn Backend) {
    let has_tabs = !coder.open_files.is_empty() || coder.panes.iter().any(|p| !p.tabs.is_empty());
    if has_tabs {
        return;
    }
    let Some(path) = coder_find_readme_path(coder) else {
        return;
    };
    let workspace = coder.root_path.as_deref();
    let Some((file, _)) = coder_load_open_file_entry(backend, &path, workspace) else {
        return;
    };
    coder.open_files.push(file);
    coder_activate_in_group(coder, &path, 0);
}

/// Locate a root-level README markdown file (exact `README.md` preferred).
fn coder_find_readme_path(coder: &CoderWork) -> Option<String> {
    let mut fallback = None;
    for entry in &coder.tree {
        if entry.is_dir || !entry.name.eq_ignore_ascii_case("README.md") {
            continue;
        }
        if entry.name == "README.md" {
            return Some(entry.path.clone());
        }
        fallback.get_or_insert_with(|| entry.path.clone());
    }
    fallback
}

/// Apply persisted tab paths onto a freshly listed [`CoderWork`].
pub(crate) fn coder_apply_persisted_tabs(
    coder: &mut CoderWork,
    tabs: &halo_backend::commands::coder::CoderPersistedTabs,
    backend: &dyn Backend,
) {
    use halo_backend::commands::coder::CoderPersistedLayout;

    let workspace = coder.root_path.clone();
    let mut open_files: Vec<CoderOpenFile> = Vec::new();
    let mut ensure = |path: &str| {
        if open_files.iter().any(|f| f.path == path) {
            return true;
        }
        match coder_load_open_file_entry(backend, path, workspace.as_deref()) {
            Some((file, _)) => {
                open_files.push(file);
                true
            }
            None => false,
        }
    };

    fn layout_from_persisted(node: &CoderPersistedLayout) -> CoderLayout {
        match node {
            CoderPersistedLayout::Leaf { id } => CoderLayout::Leaf(*id),
            CoderPersistedLayout::Split {
                axis,
                first,
                second,
            } => CoderLayout::Split {
                axis: if axis == "vertical" {
                    CoderSplitAxis::Vertical
                } else {
                    CoderSplitAxis::Horizontal
                },
                first: Box::new(layout_from_persisted(first)),
                second: Box::new(layout_from_persisted(second)),
            },
        }
    }

    if let (Some(layout_node), true) = (&tabs.layout, !tabs.panes.is_empty()) {
        let mut panes: Vec<CoderPane> = Vec::new();
        for persisted in &tabs.panes {
            let pane_tabs: Vec<String> = persisted
                .tabs
                .iter()
                .filter(|p| ensure(p))
                .cloned()
                .collect();
            let active = persisted
                .active
                .as_ref()
                .filter(|p| pane_tabs.iter().any(|t| t == *p))
                .cloned()
                .or_else(|| pane_tabs.last().cloned());
            panes.push(CoderPane {
                id: persisted.id,
                tabs: pane_tabs,
                active,
            });
        }
        if panes.is_empty() {
            panes.push(CoderPane::new(0));
        }
        let layout = layout_from_persisted(layout_node);
        // Drop panes that aren't reachable from the layout.
        let live: std::collections::HashSet<u8> = layout.leaf_ids().into_iter().collect();
        panes.retain(|p| live.contains(&p.id));
        for id in &live {
            if !panes.iter().any(|p| p.id == *id) {
                panes.push(CoderPane::new(*id));
            }
        }
        coder.open_files = open_files;
        coder.panes = panes;
        coder.layout = layout;
        coder.focused_pane = if live.contains(&tabs.focused_pane) {
            tabs.focused_pane
        } else {
            coder.layout.first_leaf()
        };
        coder_seed_default_open_file(coder, backend);
        return;
    }

    // Legacy two-pane migration.
    let primary_tabs: Vec<String> = tabs
        .primary_tabs
        .iter()
        .filter(|p| ensure(p))
        .cloned()
        .collect();
    let secondary_tabs: Vec<String> = tabs
        .secondary_tabs
        .iter()
        .filter(|p| ensure(p))
        .cloned()
        .collect();

    let active_path = tabs
        .active_path
        .as_ref()
        .filter(|p| primary_tabs.iter().any(|t| t == *p))
        .cloned()
        .or_else(|| primary_tabs.last().cloned());
    let secondary_active = tabs
        .secondary_active
        .as_ref()
        .filter(|p| secondary_tabs.iter().any(|t| t == *p))
        .cloned()
        .or_else(|| secondary_tabs.last().cloned());

    coder.open_files = open_files;
    let mut panes = vec![CoderPane {
        id: 0,
        tabs: primary_tabs,
        active: active_path,
    }];
    if secondary_tabs.is_empty() {
        coder.layout = CoderLayout::Leaf(0);
        coder.focused_pane = 0;
    } else {
        panes.push(CoderPane {
            id: 1,
            tabs: secondary_tabs,
            active: secondary_active,
        });
        let axis = match tabs.split_axis.as_deref() {
            Some("vertical") => CoderSplitAxis::Vertical,
            _ => CoderSplitAxis::Horizontal,
        };
        coder.layout = CoderLayout::Split {
            axis,
            first: Box::new(CoderLayout::Leaf(0)),
            second: Box::new(CoderLayout::Leaf(1)),
        };
        coder.focused_pane = if tabs.focused_group == 1 { 1 } else { 0 };
    }
    coder.panes = panes;
    coder_seed_default_open_file(coder, backend);
}

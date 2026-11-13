//! Chat sessions history, archiving, transcript clearing, and artifact opening.

use super::super::*;

impl Bridge {
    pub fn halo_agent_toggle_history(cx: &mut App) -> BridgeResult<()> {
        let opening = !Self::state(cx).read(cx).coder.agent_history_open;
        Self::state(cx).update(cx, |s| {
            s.coder.agent_ensure_session();
            s.coder.agent_history_open = !s.coder.agent_history_open;
            if s.coder.agent_history_open {
                s.coder.agent_picker_open = false;
                s.coder.agent_approval_picker_open = false;
                s.coder.agent_completion = None;
            }
            s.commit_active_tab();
        });
        if opening {
            let _ = Self::halo_agent_prune_missing_projects(cx);
        }
        Ok(())
    }

    pub fn halo_agent_close_history(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_history_open = false;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Resolve a workspace-relative or absolute artifact path and open it in the editor.
    pub fn halo_agent_open_artifact(
        path: &str,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let root = Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .clone()
            .unwrap_or_default();
        let candidate = std::path::Path::new(path);
        let abs = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else if root.is_empty() {
            candidate.to_path_buf()
        } else {
            std::path::Path::new(&root).join(candidate)
        };
        let abs = abs.to_string_lossy().to_string();
        Self::coder_open_file(&abs, editors, window, cx)
    }

    /// Start a blank session; other chats (and their runs) keep going.
    pub fn halo_agent_new_chat(cx: &mut App) -> BridgeResult<()> {
        let previous = Self::state(cx)
            .read(cx)
            .coder
            .agent_active_session_id
            .clone();
        Self::state(cx).update(cx, |s| {
            s.coder.agent_flush_active_session();
            s.coder.agent_new_chat();
            // Brand-new slots inherit the global settings default.
            let new_id = s.coder.agent_active_session_id.clone();
            if new_id != previous {
                let mode = s.coder_settings.agent_approval_mode;
                if let Some(session) = s.coder.agent_sessions.iter_mut().find(|s| s.id == new_id) {
                    session.approval_mode = mode;
                }
                s.coder.agent_approval_mode = mode;
            }
            s.coder.agent_approval_picker_open = false;
            s.commit_active_tab();
        });
        if let Some(session) = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .find(|s| s.id == previous)
            .cloned()
        {
            Self::halo_agent_persist_session_async(session, cx);
        }
        Ok(())
    }

    /// Open `path` (if needed) and start a blank chat bound to that project.
    pub fn halo_agent_new_chat_for_project(path: &str, cx: &mut App) -> BridgeResult<()> {
        if !path.is_empty() {
            Self::halo_agent_prepare_project(path, false, cx)?;
        }
        Self::halo_agent_new_chat(cx)
    }

    /// Persist Accordion open indices for the Agent sessions rail.
    pub fn halo_agent_set_rail_open_sections(
        open_ixs: &[usize],
        keys: &[String],
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_set_rail_open_sections(open_ixs, keys);
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Switch the Agent rail to a project: expand instantly, sync chat from
    /// in-memory sessions, and hydrate the workspace off the UI thread.
    /// List order is unchanged — the selected project stays where it was.
    pub fn halo_agent_focus_project(path: &str, cx: &mut App) -> BridgeResult<()> {
        Self::halo_agent_prepare_project(path, true, cx)
    }

    /// Expand `path` in the Agent rail and open its workspace without blocking
    /// paint. When `sync_chat` is true, activate that project's latest chat
    /// (or a blank one) immediately from in-memory sessions.
    fn halo_agent_prepare_project(path: &str, sync_chat: bool, cx: &mut App) -> BridgeResult<()> {
        if path.is_empty() {
            return Ok(());
        }
        if !std::path::Path::new(path).is_dir() {
            return Err(BridgeError::message(format!(
                "Project folder is missing: {path}"
            )));
        }

        let already_open = Self::state(cx).read(cx).coder.root_path.as_deref() == Some(path);
        let needs_hydrate = !already_open
            || Self::state(cx)
                .read(cx)
                .coder
                .agent_project_loading
                .is_some()
            || Self::state(cx).read(cx).coder.tree.is_empty();

        // Instant UI: expand the section, stamp the root, switch chat — never
        // wait on list_dir / tabs / git before the next frame.
        Self::state(cx).update(cx, |s| {
            s.coder.agent_rail_collapsed.retain(|k| k != path);
            if !already_open {
                s.coder.agent_flush_active_session();
                s.coder.root_path = Some(path.to_string());
                s.coder.agent_project_loading = Some(path.to_string());
            }
            if sync_chat {
                s.coder.agent_sync_chat_to_root();
            }
            s.commit_active_tab();
        });

        if !needs_hydrate {
            if let Ok(backend) = Self::backend(cx) {
                if let Ok(projects) = backend.coder_recent_projects() {
                    Self::state(cx).update(cx, |s| s.coder_projects = projects);
                }
            }
            return Ok(());
        }

        Self::coder_open_project_async(path, cx)
    }

    pub fn halo_agent_select_session(id: &str, cx: &mut App) -> BridgeResult<()> {
        let (previous, harness_before) = {
            let state = Self::state(cx).read(cx);
            (
                state.coder.agent_active_session_id.clone(),
                state.coder.agent_harness.clone(),
            )
        };
        let session_root = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.root_path.clone());
        if let Some(path) = session_root.as_deref() {
            let current = Self::state(cx).read(cx).coder.root_path.clone();
            if current.as_deref() != Some(path) {
                // Expand + hydrate without syncing to "latest" — we select `id` next.
                let _ = Self::halo_agent_prepare_project(path, false, cx);
            }
        }
        let (harness_after, catalog_empty) = Self::state(cx).update(cx, |s| {
            // Only clear archive flag when needed; never reshuffle on select.
            if s.coder
                .agent_sessions
                .iter()
                .any(|sess| sess.id == id && sess.archived)
            {
                s.coder.agent_unarchive_session(id);
            }
            let _ = s.coder.agent_select_session(id);
            s.commit_active_tab();
            (
                s.coder.agent_harness.clone(),
                s.coder.agent_model_catalog.is_empty(),
            )
        });
        if harness_after != harness_before || catalog_empty {
            if harness_after == "antigravity" {
                Self::antigravity_acp_refresh_auth(cx);
            }
            let _ = Self::halo_agent_ensure_catalogs(cx);
            let _ = Self::halo_agent_refresh_skills(cx);
        }
        for sid in [previous, id.to_string()] {
            if let Some(session) = Self::state(cx)
                .read(cx)
                .coder
                .agent_sessions
                .iter()
                .find(|s| s.id == sid)
                .cloned()
            {
                Self::halo_agent_persist_session_async(session, cx);
            }
        }
        Ok(())
    }

    pub fn halo_agent_archive_session(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            if let Some(work) = s.coder_work_for_chat_mut(id) {
                work.agent_archive_session(id);
            }
            s.commit_active_tab();
        });
        let _ = backend.halo_agent_interrupt_chat(id);
        let _ = backend.halo_agent_forget_harness_session(id);
        if let Some(session) = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .cloned()
        {
            Self::halo_agent_persist_session_async(session, cx);
        }
        Ok(())
    }

    pub fn halo_agent_unarchive_session(id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_unarchive_session(id);
            s.commit_active_tab();
        });
        if let Some(session) = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .find(|s| s.id == id)
            .cloned()
        {
            Self::halo_agent_persist_session_async(session, cx);
        }
        Ok(())
    }

    pub fn halo_agent_delete_session(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            if let Some(work) = s.coder_work_for_chat_mut(id) {
                work.agent_delete_session(id);
            }
            s.commit_active_tab();
        });
        let _ = backend.halo_agent_interrupt_chat(id);
        let _ = backend.halo_agent_forget_harness_session(id);
        // The chat's Chrome outlives the chat unless we close it: the daemon
        // only reaps on an hour of idleness. Its profile would outlive it too,
        // cookies and all, so a deleted chat forgets rather than just closes.
        let session = halo_backend::commands::halo_agent::browser::session_for(id);
        let _ = Self::halo_agent_browser_forget(cx, &session);
        Self::halo_agent_delete_persisted(id, cx);
        Ok(())
    }

    /// Drop missing folders from recent projects and delete agent chats whose
    /// `root_path` no longer exists on disk.
    pub fn halo_agent_prune_missing_projects(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let projects = backend.coder_recent_projects()?;
        Self::state(cx).update(cx, |s| {
            s.coder_projects = projects;
        });

        let missing: Vec<String> = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .filter_map(|session| session.root_path.clone())
            .filter(|path| !path.is_empty() && !std::path::Path::new(path).is_dir())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        for path in missing {
            Self::halo_agent_remove_project_sessions(&path, cx)?;
        }
        Ok(())
    }

    /// Remove a project from the Agent sessions rail: forget the recent entry
    /// and permanently delete every chat bound to that folder. Files on disk
    /// are left untouched.
    pub fn halo_agent_remove_project(path: &str, cx: &mut App) -> BridgeResult<()> {
        if path.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        backend.coder_remove_recent_project(path)?;

        let is_current = Self::state(cx).read(cx).coder.root_path.as_deref() == Some(path);
        if is_current {
            // Close workspace without re-adding the project to recents.
            let _ = Self::coder_close_project(cx);
        }

        Self::halo_agent_remove_project_sessions(path, cx)?;

        if let Ok(projects) = backend.coder_recent_projects() {
            Self::state(cx).update(cx, |s| s.coder_projects = projects);
        }
        Ok(())
    }

    fn halo_agent_remove_project_sessions(path: &str, cx: &mut App) -> BridgeResult<()> {
        let ids: Vec<String> = Self::state(cx)
            .read(cx)
            .coder
            .agent_sessions
            .iter()
            .filter(|s| s.root_path.as_deref() == Some(path))
            .map(|s| s.id.clone())
            .collect();

        for id in ids {
            Self::halo_agent_delete_session(&id, cx)?;
        }

        Self::state(cx).update(cx, |s| {
            s.coder.agent_rail_collapsed.retain(|k| k != path);
            s.sync_agent_into_coder_tabs();
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Wipe the active chat transcript (keeps the session slot in history).
    pub fn halo_agent_clear(cx: &mut App) -> BridgeResult<()> {
        let chat_id = Self::state(cx)
            .read(cx)
            .coder
            .agent_active_session_id
            .clone();
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            if let Some(work) = s.coder_work_for_chat_mut(&chat_id) {
                if let Some(session) = work.agent_sessions.iter_mut().find(|x| x.id == chat_id) {
                    session.clear_chat();
                }
            }
            s.finish_agent_batch(&[chat_id.clone()], true);
        });
        let _ = backend.halo_agent_interrupt_chat(&chat_id);
        let _ = backend.halo_agent_forget_harness_session(&chat_id);
        // Cleared chats stay in the rail but drop durable content.
        Self::halo_agent_delete_persisted(&chat_id, cx);
        Ok(())
    }
}

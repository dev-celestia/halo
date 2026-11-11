//! Workspace and project lifecycle: open / close / restore / persist.

use super::*;

impl Bridge {
    pub fn coder_open_folder(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let dialog = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add Project".into()),
        });

        Self::state(cx).update(cx, |s| {
            s.last_error = None;
            s.last_message = Some("Choose a project folder…".into());
        });

        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let selected = match dialog.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => {
                    let _ = cx.update(|cx| {
                        Self::state(cx).update(cx, |s| s.last_message = None);
                    });
                    return;
                }
                Ok(Err(err)) => {
                    let _ = cx.update(|cx| {
                        Self::state(cx).update(cx, |s| {
                            s.apply_err(BridgeError::message(format!(
                                "Open folder dialog failed: {err}"
                            )));
                        });
                    });
                    return;
                }
                Err(_) => {
                    let _ = cx.update(|cx| {
                        Self::state(cx).update(cx, |s| s.last_message = None);
                    });
                    return;
                }
            };

            let Some(root) = selected else {
                let _ = cx.update(|cx| {
                    Self::state(cx).update(cx, |s| s.last_message = None);
                });
                return;
            };
            let root_str = root.display().to_string();

            // `list_dir` confines to the saved workspace root. Persist the new
            // folder first so listing a sibling project is not rejected as
            // "outside the open project".
            let previous_root = backend.coder_saved_root().ok().flatten();
            if let Err(err) = backend.coder_save_root(Some(&root_str)) {
                let _ = cx.update(|cx| {
                    Self::state(cx).update(cx, |s| s.apply_err(err));
                });
                return;
            }

            let (tx, rx) = std::sync::mpsc::channel();
            let spawn_ok = std::thread::Builder::new()
                .name("halo-coder-open".into())
                .spawn({
                    let backend = backend.clone();
                    let root_str = root_str.clone();
                    move || {
                        let _ = tx.send(backend.coder_list_dir(&root_str));
                    }
                })
                .is_ok();

            let result = if !spawn_ok {
                Err(BridgeError::message("failed to spawn coder worker"))
            } else {
                loop {
                    match rx.try_recv() {
                        Ok(result) => break result,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            executor.timer(Duration::from_millis(8)).await;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            break Err(BridgeError::message("coder worker died"));
                        }
                    }
                }
            };

            let _ = cx.update(|cx| {
                Self::coder_persist_open_tabs(cx);
                Self::coder_lsp_stop(cx);
                match result {
                    Ok(entries) => {
                        let persisted = Self::backend(cx)
                            .ok()
                            .and_then(|b| b.coder_load_open_tabs(&root_str).ok())
                            .unwrap_or_default();
                        let backend = Self::backend(cx).ok();
                        Self::state(cx).update(cx, |s| {
                            s.coder.agent_flush_active_session();
                            let prev = s.coder.clone();
                            coder_clear_undo_stacks(&mut s.coder, backend.as_deref());
                            let mut work = coder_work_from_listing(root_str.clone(), entries);
                            if let Some(backend) = backend.as_ref() {
                                coder_apply_persisted_tabs(&mut work, &persisted, backend.as_ref());
                            }
                            coder_copy_agent_surface(&mut work, &prev);
                            work.agent_rail_collapsed.retain(|k| k != &root_str);
                            work.agent_sync_chat_to_root();
                            s.coder = work;
                            s.commit_active_tab();
                        });
                        if let Ok(backend) = Self::backend(cx) {
                            if let Ok(projects) = backend.coder_recent_projects() {
                                Self::state(cx).update(cx, |s| s.coder_projects = projects);
                            }
                        }
                        let _ = Self::git_refresh(cx);
                    }
                    Err(err) => {
                        if let Ok(backend) = Self::backend(cx) {
                            let _ = backend.coder_save_root(previous_root.as_deref());
                        }
                        Self::state(cx).update(cx, |s| s.apply_err(err));
                    }
                }
            });
        })
        .detach();
        Ok(())
    }

    /// Switch the active Coder workspace to an already-known project folder.
    pub fn coder_open_project(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let path = path.to_string();
        if !std::path::Path::new(&path).is_dir() {
            return Err(BridgeError::message(format!(
                "Project folder is missing: {path}"
            )));
        }
        if Self::state(cx).read(cx).coder.root_path.as_deref() == Some(path.as_str())
            && Self::state(cx)
                .read(cx)
                .coder
                .agent_project_loading
                .is_none()
        {
            // Refresh the list without reordering — selection keeps position.
            if let Ok(projects) = backend.coder_recent_projects() {
                Self::state(cx).update(cx, |s| s.coder_projects = projects);
            }
            return Ok(());
        }
        Self::coder_persist_open_tabs(cx);
        Self::coder_lsp_stop(cx);
        // Persist before list_dir — confinement uses the saved root, so listing
        // a sibling project while the old root is still saved always fails.
        let previous_root = backend.coder_saved_root().ok().flatten();
        backend.coder_save_root(Some(&path))?;
        let entries = match backend.coder_list_dir(&path) {
            Ok(entries) => entries,
            Err(err) => {
                let _ = backend.coder_save_root(previous_root.as_deref());
                return Err(err);
            }
        };
        let persisted = backend.coder_load_open_tabs(&path).unwrap_or_default();
        Self::coder_apply_opened_project(path, entries, persisted, cx);
        Ok(())
    }

    /// Hydrate explorer/tabs/git for `path` off the UI thread.
    ///
    /// Agent project switches call this after an optimistic root + chat sync so
    /// the rail expands immediately; list_dir / open-tabs IO cannot freeze paint.
    pub(in crate::bridge) fn coder_open_project_async(
        path: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let path = path.to_string();
        if !std::path::Path::new(&path).is_dir() {
            Self::state(cx).update(cx, |s| s.coder.agent_project_loading = None);
            return Err(BridgeError::message(format!(
                "Project folder is missing: {path}"
            )));
        }
        if Self::state(cx).read(cx).coder.root_path.as_deref() == Some(path.as_str())
            && Self::state(cx)
                .read(cx)
                .coder
                .agent_project_loading
                .is_none()
            && !Self::state(cx).read(cx).coder.tree.is_empty()
        {
            if let Ok(projects) = backend.coder_recent_projects() {
                Self::state(cx).update(cx, |s| s.coder_projects = projects);
            }
            return Ok(());
        }

        let previous_root = backend.coder_saved_root().ok().flatten();
        // Persist + stop LSP on the UI thread (fast); list_dir is the slow part.
        Self::coder_persist_open_tabs(cx);
        Self::coder_lsp_stop(cx);
        backend.coder_save_root(Some(&path))?;

        let epoch = Self::state(cx).update(cx, |s| {
            s.coder.agent_project_loading = Some(path.clone());
            s.coder.agent_project_switch_epoch = s.coder.agent_project_switch_epoch.wrapping_add(1);
            s.coder.agent_project_switch_epoch
        });

        Self::run_blocking(
            cx,
            {
                let backend = backend.clone();
                let path = path.clone();
                move || {
                    let entries = backend.coder_list_dir(&path)?;
                    let persisted = backend.coder_load_open_tabs(&path).unwrap_or_default();
                    Ok((path, entries, persisted))
                }
            },
            move |result, cx| {
                let still_current =
                    Self::state(cx).read(cx).coder.agent_project_switch_epoch == epoch;
                if !still_current {
                    return;
                }
                match result {
                    Ok((path, entries, persisted)) => {
                        Self::coder_apply_opened_project(path, entries, persisted, cx);
                    }
                    Err(err) => {
                        if let Ok(backend) = Self::backend(cx) {
                            let _ = backend.coder_save_root(previous_root.as_deref());
                        }
                        Self::state(cx).update(cx, |s| {
                            s.coder.agent_project_loading = None;
                            if let Some(prev) = previous_root {
                                s.coder.root_path = Some(prev);
                                s.coder.agent_sync_chat_to_root();
                            }
                            s.apply_err(err);
                            s.commit_active_tab();
                        });
                    }
                }
            },
        );
        Ok(())
    }

    /// Apply a listed project onto `coder` state (shared by sync + async open).
    fn coder_apply_opened_project(
        path: String,
        entries: Vec<crate::work::CoderDirEntry>,
        persisted: halo_backend::commands::coder::CoderPersistedTabs,
        cx: &mut App,
    ) {
        let backend = Self::backend(cx).ok();
        Self::state(cx).update(cx, |s| {
            s.coder.agent_flush_active_session();
            let prev = s.coder.clone();
            coder_clear_undo_stacks(&mut s.coder, backend.as_deref());
            let mut work = coder_work_from_listing(path.clone(), entries);
            if let Some(backend) = backend.as_ref() {
                coder_apply_persisted_tabs(&mut work, &persisted, backend.as_ref());
            }
            coder_copy_agent_surface(&mut work, &prev);
            work.agent_rail_collapsed.retain(|k| k != &path);
            work.agent_project_loading = None;
            work.agent_sync_chat_to_root();
            s.coder = work;
            // Prefer the live Coder tab; if another tool is active, still stamp
            // every Coder tab so a later `load_active_tab` cannot resurrect the
            // previous root.
            if s.active_work_kind() == Some(WorkKind::Coder) {
                s.commit_active_tab();
            } else {
                for tab in &mut s.work_tabs {
                    if let WorkData::Coder(slot) = &mut tab.data {
                        *slot = s.coder.clone();
                    }
                }
            }
        });
        if let Ok(backend) = Self::backend(cx) {
            if let Ok(projects) = backend.coder_recent_projects() {
                Self::state(cx).update(cx, |s| s.coder_projects = projects);
            }
        }
        let _ = Self::git_refresh(cx);
        Self::pulls_on_project_changed(cx);
        // Skill roots are project-relative; a new project has a new catalog.
        let _ = Self::halo_agent_refresh_skills(cx);
    }

    /// Close the active Coder project: persist tabs, tear down LSP/terminals,
    /// clear the saved root, and reset workspace state. Recent projects are kept.
    pub fn coder_close_project(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        if Self::state(cx).read(cx).coder.root_path.is_none() {
            return Ok(());
        }

        Self::coder_persist_open_tabs(cx);

        let term_ids: Vec<String> = Self::state(cx)
            .read(cx)
            .coder
            .terminals
            .iter()
            .map(|t| t.id.clone())
            .collect();
        for id in &term_ids {
            let _ = backend.coder_pty_kill(id);
        }

        let _ = Self::coder_stop_run(cx);
        Self::coder_lsp_stop(cx);
        backend.coder_save_root(None)?;

        Self::state(cx).update(cx, |s| {
            s.coder.agent_flush_active_session();
            let prev = s.coder.clone();
            coder_clear_undo_stacks(&mut s.coder, Some(backend.as_ref()));
            let mut work = CoderWork::default();
            coder_copy_agent_surface(&mut work, &prev);
            work.agent_refresh_active_session();
            s.coder = work;
            // Prefer the live Coder tab; if another tool is active, still stamp
            // every Coder tab so a later `load_active_tab` cannot resurrect the
            // previous root.
            if s.active_work_kind() == Some(WorkKind::Coder) {
                s.commit_active_tab();
            } else {
                for tab in &mut s.work_tabs {
                    if let WorkData::Coder(slot) = &mut tab.data {
                        *slot = s.coder.clone();
                    }
                }
            }
            s.apply_ok_msg("Closed project".into());
        });

        if let Ok(projects) = backend.coder_recent_projects() {
            Self::state(cx).update(cx, |s| s.coder_projects = projects);
        }
        Self::state(cx).update(cx, |s| {
            s.pulls.items.clear();
            s.pulls.selected = None;
            s.pulls.error = None;
            s.pulls.loading = false;
            s.pulls_epoch = s.pulls_epoch.wrapping_add(1);
        });
        // No project → no project skills.
        let _ = Self::halo_agent_refresh_skills(cx);
        Ok(())
    }

    /// Drop a folder from the recent-projects list. Files on disk stay.
    ///
    /// If that folder is the open workspace, close it first. Agent chat history
    /// for the path is left alone (use [`Self::halo_agent_remove_project`] to
    /// clear sessions too).
    pub fn coder_remove_recent_project(path: &str, cx: &mut App) -> BridgeResult<()> {
        if path.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let is_current = Self::state(cx).read(cx).coder.root_path.as_deref() == Some(path);
        if is_current {
            let _ = Self::coder_close_project(cx);
        }
        backend.coder_remove_recent_project(path)?;
        if let Ok(projects) = backend.coder_recent_projects() {
            Self::state(cx).update(cx, |s| {
                s.coder_projects = projects;
                s.apply_ok_msg("Removed project from recents".into());
            });
        }
        Ok(())
    }

    /// Reload the last opened folder into the active Coder workspace when unset.
    pub fn coder_restore_workspace(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let has_root = Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .as_ref()
            .is_some_and(|p| std::path::Path::new(p).is_dir());
        if has_root {
            // Root already restored (e.g. install seed) — still hydrate tabs if empty.
            let needs_tabs = {
                let coder = &Self::state(cx).read(cx).coder;
                coder.panes.iter().all(|p| p.tabs.is_empty()) && coder.open_files.is_empty()
            };
            if needs_tabs {
                Self::coder_restore_open_tabs(cx)?;
            }
            let _ = Self::git_refresh(cx);
            return Ok(());
        }

        let Some(root) = backend.coder_saved_root()? else {
            return Ok(());
        };
        let entries = backend.coder_list_dir(&root)?;
        let persisted = backend.coder_load_open_tabs(&root).unwrap_or_default();
        Self::state(cx).update(cx, |s| {
            s.coder.agent_flush_active_session();
            let prev = s.coder.clone();
            let mut work = coder_work_from_listing(root, entries);
            coder_apply_persisted_tabs(&mut work, &persisted, backend.as_ref());
            coder_copy_agent_surface(&mut work, &prev);
            work.agent_refresh_active_session();
            s.coder = work;
            s.commit_active_tab();
        });
        let _ = Self::git_refresh(cx);
        Ok(())
    }

    /// Write the current project's open tabs to disk.
    pub fn coder_persist_open_tabs(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let Some(root) = Self::state(cx).read(cx).coder.root_path.clone() else {
            return;
        };
        let tabs = coder_tabs_snapshot(&Self::state(cx).read(cx).coder);
        let _ = backend.coder_save_open_tabs(&root, &tabs);
    }

    /// Reload persisted tabs for the current root into `coder` state (buffers from disk).
    pub fn coder_restore_open_tabs(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let Some(root) = Self::state(cx).read(cx).coder.root_path.clone() else {
            return Ok(());
        };
        let persisted = backend.coder_load_open_tabs(&root)?;
        let empty = if !persisted.panes.is_empty() {
            persisted.panes.iter().all(|p| p.tabs.is_empty())
        } else {
            persisted.primary_tabs.is_empty() && persisted.secondary_tabs.is_empty()
        };
        Self::state(cx).update(cx, |s| {
            if empty {
                // No remembered tabs — open README.md when present.
                coder_seed_default_open_file(&mut s.coder, backend.as_ref());
            } else {
                coder_apply_persisted_tabs(&mut s.coder, &persisted, backend.as_ref());
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Push active buffers into every live editor pane after a restore.
    pub fn coder_sync_editors(
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_lsp_bind_editors(editors, cx);
        let panes: Vec<(u8, Option<String>)> = {
            let coder = &Self::state(cx).read(cx).coder;
            coder
                .layout
                .leaf_ids()
                .into_iter()
                .map(|id| (id, coder.pane_active(id).map(str::to_string)))
                .collect()
        };
        for (pane_id, active) in panes {
            let Some(editor) = editors.get(pane_id as usize) else {
                continue;
            };
            if let Some(path) = active.as_deref() {
                let _ = Self::coder_load_tab_into_group(path, editor, pane_id, window, cx);
            } else {
                editor.update(cx, |e, cx| {
                    e.set_language("plaintext", cx);
                    e.set_text("// Open a folder and select a file to edit.", window, cx);
                });
            }
        }
        Ok(())
    }
}

//! Explorer toggling and opening files into editor groups.

use super::*;

impl Bridge {
    pub fn coder_toggle_dir(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let path = path.to_string();

        let action = Self::state(cx).update(cx, |s| {
            if s.coder.expanded.iter().any(|p| p == &path) {
                s.coder.expanded.retain(|p| p != &path);
                s.commit_active_tab();
                CoderToggleAction::Collapse
            } else {
                let already_loaded = coder_find_entry_mut(&mut s.coder.tree, &path)
                    .map(|e| e.children.is_some())
                    .unwrap_or(false);
                if already_loaded {
                    s.coder.expanded.push(path.clone());
                    s.commit_active_tab();
                    CoderToggleAction::Collapse
                } else {
                    CoderToggleAction::Load
                }
            }
        });

        if matches!(action, CoderToggleAction::Collapse) {
            return Ok(());
        }

        let entries = backend.coder_list_dir(&path)?;
        Self::state(cx).update(cx, |s| {
            if let Some(entry) = coder_find_entry_mut(&mut s.coder.tree, &path) {
                entry.children = Some(
                    entries
                        .into_iter()
                        .map(|e| CoderTreeEntry {
                            path: e.path,
                            name: e.name,
                            is_dir: e.is_dir,
                            children: None,
                        })
                        .collect(),
                );
            }
            if !s.coder.expanded.iter().any(|p| p == &path) {
                s.coder.expanded.push(path);
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Expand (and lazy-load) ancestor folders so `path` is visible in the
    /// explorer. Selection follows [`CoderWork::focused_active_path`].
    pub fn coder_reveal_in_explorer(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let Some(root) = Self::state(cx).read(cx).coder.root_path.clone() else {
            return Ok(());
        };
        let ancestors = coder_ancestor_dirs(&root, path);
        for dir in ancestors {
            let (needs_load, exists) = Self::state(cx).update(cx, |s| {
                match coder_find_entry_mut(&mut s.coder.tree, &dir) {
                    Some(entry) => {
                        let loaded = entry.children.is_some();
                        if loaded && !s.coder.expanded.iter().any(|p| p == &dir) {
                            s.coder.expanded.push(dir.clone());
                            s.commit_active_tab();
                        }
                        (!loaded, true)
                    }
                    // Hidden / ignored / not yet linked — cannot expand further.
                    None => (false, false),
                }
            });
            if !exists {
                break;
            }
            if !needs_load {
                continue;
            }
            let Ok(entries) = backend.coder_list_dir(&dir) else {
                break;
            };
            Self::state(cx).update(cx, |s| {
                if let Some(entry) = coder_find_entry_mut(&mut s.coder.tree, &dir) {
                    entry.children = Some(
                        entries
                            .into_iter()
                            .map(|e| CoderTreeEntry {
                                path: e.path,
                                name: e.name,
                                is_dir: e.is_dir,
                                children: None,
                            })
                            .collect(),
                    );
                }
                if !s.coder.expanded.iter().any(|p| p == &dir) {
                    s.coder.expanded.push(dir);
                }
                s.commit_active_tab();
            });
        }
        Ok(())
    }

    /// Flush every live pane's editor into shared buffers.
    fn coder_flush_all_editors(editors: &[Entity<CodeEditor>], cx: &mut App) {
        let leaf_ids = Self::state(cx).read(cx).coder.layout.leaf_ids();
        for id in leaf_ids {
            if let Some(editor) = editors.get(id as usize) {
                Self::coder_flush_group_editor(editor, id, cx);
            }
        }
    }

    /// Open `path` in the focused editor pane.
    pub fn coder_open_file(
        path: &str,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_flush_all_editors(editors, cx);
        let group = Self::state(cx).read(cx).coder.focused_pane;
        let editor = editors
            .get(group as usize)
            .ok_or_else(|| BridgeError::message("Editor pane unavailable"))?;
        match Self::coder_open_file_in_group(path, editor, group, window, cx) {
            Ok(()) => Ok(()),
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    /// Split the focused pane horizontally (if needed) and open `path` in the new pane.
    pub fn coder_open_file_to_side(
        path: &str,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_flush_all_editors(editors, cx);
        let focus = Self::state(cx).read(cx).coder.focused_pane;
        let new_id = Self::state(cx).update(cx, |s| {
            s.coder.seed_primary_tabs_if_empty();
            if s.coder.is_split() && s.coder.layout.contains_leaf(1) {
                // Prefer existing pane 1 when migrating from the old dual layout.
                s.coder.focused_pane = 1;
                Ok(1u8)
            } else {
                s.coder
                    .split_pane(focus, CoderSplitAxis::Horizontal)
                    .map_err(BridgeError::message)
            }
        })?;
        let editor = editors
            .get(new_id as usize)
            .ok_or_else(|| BridgeError::message("Editor pane unavailable"))?;
        match Self::coder_open_file_in_group(path, editor, new_id, window, cx) {
            Ok(()) => Ok(()),
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn coder_open_file_in_group(
        path: &str,
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let path = path.to_string();

        Self::coder_flush_group_editor(editor, group, cx);
        Self::state(cx).update(cx, |s| {
            // Showing editor tabs takes the top slot — leave Halo Agent.
            if s.coder.agent_open {
                s.coder.agent_open = false;
                s.commit_active_tab();
            }
            if s.coder.layout.contains_leaf(group) {
                s.coder.focused_pane = group;
            }
        });

        if Self::state(cx)
            .read(cx)
            .coder
            .open_files
            .iter()
            .any(|f| f.path == path)
        {
            Self::coder_ensure_tab_in_group(&path, group, cx);
            let result = Self::coder_load_tab_into_group(&path, editor, group, window, cx);
            Self::coder_persist_open_tabs(cx);
            return result;
        }

        if coder_is_image_path(&path) {
            backend.coder_ensure_image_file(&path)?;
            Self::state(cx).update(cx, |s| {
                s.coder.open_files.push(CoderOpenFile {
                    path: path.clone(),
                    kind: CoderFileKind::Image,
                    baseline: String::new(),
                    content: String::new(),
                    dirty: false,
                    zoom_pct: 100,
                });
                coder_activate_in_group(&mut s.coder, &path, group);
                s.commit_active_tab();
                s.last_error = None;
            });
            let _ = Self::coder_reveal_in_explorer(&path, cx);
            Self::coder_persist_open_tabs(cx);
            return Ok(());
        }

        let disk = backend.coder_read_file(&path)?;
        let workspace = Self::state(cx).read(cx).coder.root_path.clone();
        let (contents, dirty, disk_changed) = super::coder_backup::coder_apply_backup_restore(
            backend.as_ref(),
            workspace.as_deref(),
            &path,
            &disk,
        );
        let highlighter = coder_highlighter_for_path(&path);
        let lsp_language = coder_lsp_language_for_path(&path);
        Self::state(cx).update(cx, |s| {
            s.coder.open_files.push(CoderOpenFile {
                path: path.clone(),
                kind: CoderFileKind::Text,
                baseline: disk,
                content: contents.clone(),
                dirty,
                zoom_pct: 100,
            });
            coder_activate_in_group(&mut s.coder, &path, group);
            s.commit_active_tab();
            s.last_error = None;
            if disk_changed {
                s.apply_ok_msg(format!(
                    "{path} changed on disk — restored your unsaved edits"
                ));
            }
        });
        editor.update(cx, |e, cx| {
            e.set_language(highlighter, cx);
            e.set_text(contents.clone(), window, cx);
        });
        Self::coder_lsp_did_open(&path, lsp_language, &contents, editor, group, cx);
        let _ = Self::coder_reveal_in_explorer(&path, cx);
        Self::coder_persist_open_tabs(cx);
        Ok(())
    }
}

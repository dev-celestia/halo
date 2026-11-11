//! Explorer file operations: reload, copy, create, rename, delete.

use super::*;

impl Bridge {
    /// Reload one directory listing in the explorer (root or expanded folder).
    pub fn coder_reload_dir(dir: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let dir = dir.to_string();
        let entries = backend.coder_list_dir(&dir)?;
        let children: Vec<CoderTreeEntry> = entries
            .into_iter()
            .map(|e| CoderTreeEntry {
                path: e.path,
                name: e.name,
                is_dir: e.is_dir,
                children: None,
            })
            .collect();

        Self::state(cx).update(cx, |s| {
            let prefix = format!("{dir}{}", std::path::MAIN_SEPARATOR);
            s.coder
                .expanded
                .retain(|p| p == &dir || !p.starts_with(&prefix));

            if s.coder.root_path.as_deref() == Some(dir.as_str()) {
                s.coder.tree = children;
            } else if let Some(entry) = coder_find_entry_mut(&mut s.coder.tree, &dir) {
                entry.children = Some(children);
            }
            s.commit_active_tab();
        });
        let _ = Self::git_refresh(cx);
        Ok(())
    }

    pub(in crate::bridge) fn coder_parent_dir(path: &str) -> String {
        std::path::Path::new(path)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| path.to_string())
    }

    pub(super) fn coder_reload_parent(path: &str, cx: &mut App) -> BridgeResult<()> {
        let parent = Self::coder_parent_dir(path);
        let root = Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .clone()
            .unwrap_or_default();
        let reload = if parent.is_empty() || parent == "." {
            root
        } else {
            parent
        };
        Self::coder_reload_dir(&reload, cx)
    }

    pub fn coder_copy_name(path: &str, cx: &mut App) -> BridgeResult<()> {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(name));
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg("Copied name".into());
        });
        Ok(())
    }

    pub fn coder_copy_path(path: &str, cx: &mut App) -> BridgeResult<()> {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg("Copied path".into());
        });
        Ok(())
    }

    pub fn coder_copy_relative_path(path: &str, cx: &mut App) -> BridgeResult<()> {
        let root = Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .clone()
            .unwrap_or_default();
        let relative = std::path::Path::new(path)
            .strip_prefix(&root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.to_string());
        cx.write_to_clipboard(ClipboardItem::new_string(relative));
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg("Copied relative path".into());
        });
        Ok(())
    }

    /// Validate a single path component for inline create/rename.
    fn coder_validate_inline_name(name: &str) -> BridgeResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(BridgeError::message("Name cannot be empty"));
        }
        if name == "." || name == ".." {
            return Err(BridgeError::message("Invalid name"));
        }
        if name.contains('/') || name.contains('\\') {
            return Err(BridgeError::message("Name cannot contain path separators"));
        }
        Ok(())
    }

    /// Expand (and lazy-load) `parent_dir` so an inline create row is visible.
    fn coder_ensure_dir_expanded(parent_dir: &str, cx: &mut App) -> BridgeResult<()> {
        let root = Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .clone()
            .unwrap_or_default();
        if parent_dir.is_empty() || parent_dir == root {
            return Ok(());
        }
        // Reveal ancestors then ensure the parent itself is expanded.
        let _ = Self::coder_reveal_in_explorer(parent_dir, cx);
        let already_expanded = Self::state(cx)
            .read(cx)
            .coder
            .expanded
            .iter()
            .any(|p| p == parent_dir);
        if already_expanded {
            let loaded = Self::state(cx).update(cx, |s| {
                coder_find_entry_mut(&mut s.coder.tree, parent_dir)
                    .map(|e| e.children.is_some())
                    .unwrap_or(false)
            });
            if loaded {
                return Ok(());
            }
        }
        // Force-expand even if currently collapsed (toggle would collapse).
        let backend = Self::backend(cx)?;
        let path = parent_dir.to_string();
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

    fn coder_start_inline(
        parent_dir: &str,
        kind: CoderExplorerInlineKind,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let parent = if parent_dir.is_empty() {
            Self::state(cx)
                .read(cx)
                .coder
                .root_path
                .clone()
                .unwrap_or_default()
        } else {
            parent_dir.to_string()
        };
        if parent.is_empty() {
            return Err(BridgeError::message("No workspace folder open"));
        }
        Self::coder_ensure_dir_expanded(&parent, cx)?;
        Self::state(cx).update(cx, |s| {
            s.last_error = None;
            s.last_message = None;
            s.coder.explorer_inline = Some(CoderExplorerInline {
                parent_dir: parent,
                kind,
            });
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Begin an inline new-file row under `parent_dir` (no FS write until commit).
    pub fn coder_new_file(parent_dir: &str, cx: &mut App) -> BridgeResult<()> {
        Self::coder_start_inline(parent_dir, CoderExplorerInlineKind::CreateFile, cx)
    }

    /// Begin an inline new-folder row under `parent_dir` (no FS write until commit).
    pub fn coder_new_folder(parent_dir: &str, cx: &mut App) -> BridgeResult<()> {
        Self::coder_start_inline(parent_dir, CoderExplorerInlineKind::CreateFolder, cx)
    }

    /// Begin an inline rename on the given path.
    pub fn coder_rename(path: &str, cx: &mut App) -> BridgeResult<()> {
        let from = path.to_string();
        let parent = Self::coder_parent_dir(&from);
        Self::coder_start_inline(
            &parent,
            CoderExplorerInlineKind::Rename { from },
            cx,
        )
    }

    /// Cancel the active explorer inline create/rename (no FS change).
    pub fn coder_explorer_inline_cancel(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.explorer_inline = None;
            s.last_message = None;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Commit the active explorer inline create/rename with `name`.
    pub fn coder_explorer_inline_commit(
        name: &str,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_validate_inline_name(name)?;
        let name = name.trim().to_string();
        let Some(inline) = Self::state(cx).read(cx).coder.explorer_inline.clone() else {
            return Ok(());
        };
        let parent = inline.parent_dir.clone();
        let path = std::path::Path::new(&parent)
            .join(&name)
            .display()
            .to_string();

        match inline.kind {
            CoderExplorerInlineKind::CreateFile => {
                let backend = Self::backend(cx)?;
                backend.coder_create_file(&path)?;
                Self::state(cx).update(cx, |s| {
                    s.coder.explorer_inline = None;
                });
                let _ = Self::coder_reload_dir(&parent, cx);
                let backend_opt = Self::backend(cx).ok();
                Self::state(cx).update(cx, |s| {
                    coder_push_undo(
                        &mut s.coder,
                        CoderFsOp::Create {
                            path: path.clone(),
                            is_dir: false,
                        },
                        backend_opt.as_deref(),
                    );
                    s.apply_ok_msg(format!("Created {path}"));
                    s.commit_active_tab();
                });
                let _ = Self::coder_open_file(&path, editors, window, cx);
            }
            CoderExplorerInlineKind::CreateFolder => {
                let backend = Self::backend(cx)?;
                backend.coder_create_dir(&path)?;
                Self::state(cx).update(cx, |s| {
                    s.coder.explorer_inline = None;
                });
                let _ = Self::coder_reload_dir(&parent, cx);
                let backend_opt = Self::backend(cx).ok();
                Self::state(cx).update(cx, |s| {
                    coder_push_undo(
                        &mut s.coder,
                        CoderFsOp::Create {
                            path: path.clone(),
                            is_dir: true,
                        },
                        backend_opt.as_deref(),
                    );
                    s.apply_ok_msg(format!("Created folder {path}"));
                    s.commit_active_tab();
                });
            }
            CoderExplorerInlineKind::Rename { from } => {
                if path == from {
                    Self::state(cx).update(cx, |s| {
                        s.coder.explorer_inline = None;
                        s.commit_active_tab();
                    });
                    return Ok(());
                }
                let backend = Self::backend(cx)?;
                backend.coder_rename_path(&from, &path)?;
                Self::state(cx).update(cx, |s| {
                    s.coder.explorer_inline = None;
                    coder_remap_open_paths(&mut s.coder, &from, &path);
                    coder_push_undo(
                        &mut s.coder,
                        CoderFsOp::Rename {
                            from: from.clone(),
                            to: path.clone(),
                        },
                        Some(backend.as_ref()),
                    );
                    s.apply_ok_msg(format!("Renamed to {path}"));
                    s.commit_active_tab();
                });
                let _ = Self::coder_reload_dir(&parent, cx);
            }
        }
        Ok(())
    }

    /// Move `path` to trash. Caller must already have confirmed (e.g. UI dialog).
    ///
    /// Does not open another prompt — nesting `window.prompt` inside the
    /// confirm dialog's close path cancelled the delete.
    pub fn coder_delete(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let path = path.to_string();
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        let trash_path = backend.coder_trash_path(&path)?;
        Self::state(cx).update(cx, |s| {
            coder_close_paths_under(&mut s.coder, &path);
            coder_push_undo(
                &mut s.coder,
                CoderFsOp::SoftDelete {
                    path: path.clone(),
                    trash_path,
                },
                Some(backend.as_ref()),
            );
            s.apply_ok_msg(format!(
                "Deleted {name} — Undo available in the explorer menu"
            ));
        });
        Self::coder_reload_parent(&path, cx)?;
        Ok(())
    }

    pub fn coder_duplicate(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let from = path.to_string();
        let to = backend.coder_unique_sibling(&from)?;
        backend.coder_copy_path(&from, &to)?;
        let is_dir = std::path::Path::new(&to).is_dir();
        Self::coder_reload_parent(&from, cx)?;
        Self::state(cx).update(cx, |s| {
            coder_push_undo(
                &mut s.coder,
                CoderFsOp::Create {
                    path: to.clone(),
                    is_dir,
                },
                Some(backend.as_ref()),
            );
            s.apply_ok_msg(format!("Duplicated as {to}"));
        });
        Ok(())
    }

    /// Select an activity-bar panel. Clicking the active panel again toggles it closed.
    /// Opening a sidebar leaves Halo Agent so only one activity tab is active.
    pub fn coder_select_sidebar(kind: CoderSidebar, cx: &mut App) -> BridgeResult<()> {
        let opening = Self::state(cx).update(cx, |s| {
            // Activity-bar tabs are exclusive — leave the agent when showing a sidebar.
            s.coder.agent_open = false;
            if s.coder.sidebar == kind && s.coder.sidebar_open {
                s.coder.sidebar_open = false;
                false
            } else {
                s.coder.sidebar = kind;
                s.coder.sidebar_open = true;
                true
            }
        });
        Self::state(cx).update(cx, |s| s.commit_active_tab());
        let _ = opening;
        Ok(())
    }
}

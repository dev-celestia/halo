//! Editor tab selection, closing, and save plumbing.

use super::*;

impl Bridge {
    pub fn coder_select_tab(
        path: &str,
        editor: &Entity<CodeEditor>,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_select_tab_in_group(path, editor, 0, window, cx)
    }

    pub fn coder_select_tab_in_group(
        path: &str,
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_flush_group_editor(editor, group, cx);
        Self::state(cx).update(cx, |s| {
            if s.coder.layout.contains_leaf(group) {
                s.coder.focused_pane = group;
            }
        });
        let result = Self::coder_load_tab_into_group(path, editor, group, window, cx);
        Self::coder_persist_open_tabs(cx);
        result
    }

    pub fn coder_close_tab(
        path: &str,
        editor: &Entity<CodeEditor>,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_close_tab_in_group(path, editor, 0, window, cx)
    }

    pub fn coder_close_tab_in_group(
        path: &str,
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_flush_group_editor(editor, group, cx);
        let path = path.to_string();

        let (dirty, still_elsewhere, workspace) = {
            let s = Self::state(cx).read(cx);
            let dirty = s
                .coder
                .open_files
                .iter()
                .find(|f| f.path == path)
                .is_some_and(|f| f.dirty && f.kind == CoderFileKind::Text);
            let still_elsewhere = s
                .coder
                .panes
                .iter()
                .any(|p| p.id != group && p.tabs.iter().any(|t| t == &path));
            (dirty, still_elsewhere, s.coder.root_path.clone())
        };

        if dirty && !still_elsewhere {
            let name = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path.as_str())
                .to_string();
            let detail = format!("Save changes to \"{name}\" before closing?");
            let answer = window.prompt(
                PromptLevel::Warning,
                "Unsaved Changes",
                Some(&detail),
                &["Save", "Don't Save", "Cancel"],
                cx,
            );
            let editor = editor.clone();
            let window_handle = window.window_handle();
            let workspace = workspace.clone();
            cx.spawn(async move |cx: &mut AsyncApp| {
                let Ok(idx) = answer.await else {
                    return;
                };
                match idx {
                    0 => {
                        let _ = cx.update(|cx| {
                            let text = editor.read(cx).text(cx);
                            Self::state(cx).update(cx, |s| {
                                if s.coder.layout.contains_leaf(group) {
                                    s.coder.focused_pane = group;
                                    if let Some(pane) = s.coder.pane_mut(group) {
                                        pane.active = Some(path.clone());
                                    }
                                }
                            });
                            let _ = Self::coder_save_path_contents(&path, &text, cx);
                        });
                        let _ = cx.update_window(window_handle, |_, window, cx| {
                            let _ = Self::coder_finish_close_tab_sync(
                                &path,
                                &editor,
                                group,
                                window,
                                workspace.as_deref(),
                                true,
                                cx,
                            );
                        });
                    }
                    1 => {
                        let _ = cx.update_window(window_handle, |_, window, cx| {
                            let _ = Self::coder_finish_close_tab_sync(
                                &path,
                                &editor,
                                group,
                                window,
                                workspace.as_deref(),
                                false,
                                cx,
                            );
                        });
                    }
                    _ => {}
                }
            })
            .detach();
            return Ok(());
        }

        Self::coder_finish_close_tab_sync(
            &path,
            editor,
            group,
            window,
            workspace.as_deref(),
            true,
            cx,
        )
    }

    /// Write `text` to `path` and clear dirty/backup (shared by Save and close-prompt Save).
    pub(super) fn coder_save_path_contents(
        path: &str,
        text: &str,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let (previous_baseline, workspace) = {
            let s = Self::state(cx).read(cx);
            let baseline = s
                .coder
                .open_files
                .iter()
                .find(|f| f.path == path)
                .map(|f| f.baseline.clone())
                .unwrap_or_default();
            (baseline, s.coder.root_path.clone())
        };
        if previous_baseline != text {
            let _ = backend.coder_push_history(path, &previous_baseline);
        }
        backend.coder_write_file(path, text)?;
        if let Some(workspace) = workspace.as_deref() {
            let _ = backend.coder_delete_backup(workspace, path);
        }
        Self::state(cx).update(cx, |s| {
            if let Some(file) = s.coder.open_files.iter_mut().find(|f| f.path == path) {
                file.baseline = text.to_string();
                file.content = text.to_string();
                file.dirty = false;
            }
            s.commit_active_tab();
            // Footer-only — do not call apply_ok_msg (that would toast).
            s.coder.status_message = Some(format!("Saved {path}"));
        });
        Self::coder_lsp_did_save(path, text, cx);
        let _ = Self::git_refresh(cx);
        Ok(())
    }

    fn coder_finish_close_tab_sync(
        path: &str,
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        workspace: Option<&str>,
        keep_backup: bool,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let orphaning = !Self::state(cx)
            .read(cx)
            .coder
            .panes
            .iter()
            .any(|p| p.id != group && p.tabs.iter().any(|t| t == path));
        if orphaning && !keep_backup {
            if let (Some(workspace), Ok(backend)) = (workspace, Self::backend(cx)) {
                let _ = backend.coder_delete_backup(workspace, path);
            }
        }
        let next = Self::state(cx).update(cx, |s| {
            let next = coder_remove_tab_from_group(&mut s.coder, path, group);
            s.commit_active_tab();
            next
        });
        Self::coder_lsp_did_close(path, cx);
        let load_group = {
            let coder = &Self::state(cx).read(cx).coder;
            if group != 0 && !coder.layout.contains_leaf(group) {
                coder.focused_pane
            } else {
                group
            }
        };
        let result = if let Some(next) = next {
            Self::coder_load_tab_into_group(&next, editor, load_group, window, cx)
        } else {
            editor.update(cx, |e, cx| {
                e.set_language("plaintext", cx);
                e.set_text("// Open a folder and select a file to edit.", window, cx);
            });
            Ok(())
        };
        Self::coder_persist_open_tabs(cx);
        result
    }

    /// Close every tab in `group`.
    pub fn coder_close_all_tabs_in_group(
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_flush_group_editor(editor, group, cx);
        let paths = Self::state(cx).read(cx).coder.pane_tabs(group).to_vec();
        if paths.is_empty() {
            return Ok(());
        }
        Self::coder_close_paths_in_group(&paths, editor, group, window, cx)
    }

    /// Close every tab in `group` except `keep_path`.
    pub fn coder_close_other_tabs_in_group(
        keep_path: &str,
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_flush_group_editor(editor, group, cx);
        let paths: Vec<String> = Self::state(cx)
            .read(cx)
            .coder
            .pane_tabs(group)
            .iter()
            .filter(|p| p.as_str() != keep_path)
            .cloned()
            .collect();
        if paths.is_empty() {
            return Ok(());
        }
        Self::coder_close_paths_in_group(&paths, editor, group, window, cx)
    }

    fn coder_close_paths_in_group(
        paths: &[String],
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let (dirty_paths, workspace) = {
            let s = Self::state(cx).read(cx);
            let dirty: Vec<(String, String)> = paths
                .iter()
                .filter_map(|path| {
                    let file = s.coder.open_files.iter().find(|f| &f.path == path)?;
                    if !(file.dirty && file.kind == CoderFileKind::Text) {
                        return None;
                    }
                    let elsewhere = s
                        .coder
                        .panes
                        .iter()
                        .any(|p| p.id != group && p.tabs.iter().any(|t| t == path));
                    if elsewhere {
                        return None;
                    }
                    Some((path.clone(), file.content.clone()))
                })
                .collect();
            (dirty, s.coder.root_path.clone())
        };

        if !dirty_paths.is_empty() {
            let n = dirty_paths.len();
            let detail = if n == 1 {
                let name = std::path::Path::new(&dirty_paths[0].0)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(dirty_paths[0].0.as_str());
                format!("Save changes to \"{name}\" before closing?")
            } else {
                format!("Save changes to {n} files before closing?")
            };
            let answer = window.prompt(
                PromptLevel::Warning,
                "Unsaved Changes",
                Some(&detail),
                &["Save All", "Don't Save", "Cancel"],
                cx,
            );
            let editor = editor.clone();
            let window_handle = window.window_handle();
            let workspace = workspace.clone();
            let paths = paths.to_vec();
            cx.spawn(async move |cx: &mut AsyncApp| {
                let Ok(idx) = answer.await else {
                    return;
                };
                match idx {
                    0 => {
                        let _ = cx.update(|cx| {
                            for (path, content) in &dirty_paths {
                                let _ = Self::coder_save_path_contents(path, content, cx);
                            }
                        });
                        let _ = cx.update_window(window_handle, |_, window, cx| {
                            let _ = Self::coder_finish_close_paths_sync(
                                &paths,
                                &editor,
                                group,
                                window,
                                workspace.as_deref(),
                                true,
                                cx,
                            );
                        });
                    }
                    1 => {
                        let _ = cx.update_window(window_handle, |_, window, cx| {
                            let _ = Self::coder_finish_close_paths_sync(
                                &paths,
                                &editor,
                                group,
                                window,
                                workspace.as_deref(),
                                false,
                                cx,
                            );
                        });
                    }
                    _ => {}
                }
            })
            .detach();
            return Ok(());
        }

        Self::coder_finish_close_paths_sync(
            paths,
            editor,
            group,
            window,
            workspace.as_deref(),
            true,
            cx,
        )
    }

    fn coder_finish_close_paths_sync(
        paths: &[String],
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        workspace: Option<&str>,
        keep_backup: bool,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if !keep_backup {
            let orphans: Vec<String> = {
                let s = Self::state(cx).read(cx);
                paths
                    .iter()
                    .filter(|path| {
                        !s.coder
                            .panes
                            .iter()
                            .any(|p| p.id != group && p.tabs.iter().any(|t| t == *path))
                    })
                    .cloned()
                    .collect()
            };
            if let (Some(workspace), Ok(backend)) = (workspace, Self::backend(cx)) {
                for path in &orphans {
                    let _ = backend.coder_delete_backup(workspace, path);
                }
            }
        }

        for path in paths {
            Self::coder_lsp_did_close(path, cx);
        }

        let next = Self::state(cx).update(cx, |s| {
            if let Some(pane) = s.coder.pane_mut(group) {
                pane.tabs.retain(|p| !paths.iter().any(|close| close == p));
                if pane
                    .active
                    .as_ref()
                    .is_some_and(|a| paths.iter().any(|close| close == a))
                {
                    pane.active = pane.tabs.last().cloned();
                }
                let empty = pane.tabs.is_empty();
                let next = pane.active.clone();
                if empty && s.coder.is_split() {
                    let _ = s.coder.close_pane(group);
                }
                s.coder.prune_orphan_open_files();
                s.commit_active_tab();
                next
            } else {
                None
            }
        });

        let load_group = {
            let coder = &Self::state(cx).read(cx).coder;
            if group != 0 && !coder.layout.contains_leaf(group) {
                coder.focused_pane
            } else {
                group
            }
        };
        let result = if let Some(next) = next {
            Self::coder_load_tab_into_group(&next, editor, load_group, window, cx)
        } else {
            editor.update(cx, |e, cx| {
                e.set_language("plaintext", cx);
                e.set_text("// Open a folder and select a file to edit.", window, cx);
            });
            Ok(())
        };
        Self::coder_persist_open_tabs(cx);
        result
    }
}

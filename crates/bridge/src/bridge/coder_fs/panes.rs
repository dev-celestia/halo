//! Split panes, saving, formatting, image zoom, and tab flushing.

use super::*;

impl Bridge {
    /// Split `pane_id` along `axis`. The new pane clones the active tab.
    pub fn coder_split_pane(
        pane_id: u8,
        axis: CoderSplitAxis,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if let Some(editor) = editors.get(pane_id as usize) {
            Self::coder_flush_group_editor(editor, pane_id, cx);
        }
        let path = Self::state(cx)
            .read(cx)
            .coder
            .pane_active(pane_id)
            .map(str::to_string)
            .ok_or_else(|| BridgeError::message("Open a file before splitting"))?;

        let new_id = Self::state(cx).update(cx, |s| {
            s.coder.seed_primary_tabs_if_empty();
            let id = s
                .coder
                .split_pane(pane_id, axis)
                .map_err(BridgeError::message)?;
            s.commit_active_tab();
            Ok::<u8, BridgeError>(id)
        })?;
        let editor = editors
            .get(new_id as usize)
            .ok_or_else(|| BridgeError::message("Editor pane unavailable"))?;
        let result = Self::coder_load_tab_into_group(&path, editor, new_id, window, cx);
        Self::coder_persist_open_tabs(cx);
        result
    }

    /// Close `pane_id` (clear tabs if it is the last leaf).
    pub fn coder_close_pane(
        pane_id: u8,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if let Some(editor) = editors.get(pane_id as usize) {
            Self::coder_flush_group_editor(editor, pane_id, cx);
        }
        let focus = Self::state(cx).update(cx, |s| {
            let focus = s.coder.close_pane(pane_id).unwrap_or(s.coder.focused_pane);
            s.commit_active_tab();
            focus
        });
        if let Some(editor) = editors.get(focus as usize) {
            if let Some(path) = Self::state(cx)
                .read(cx)
                .coder
                .pane_active(focus)
                .map(str::to_string)
            {
                let _ = Self::coder_load_tab_into_group(&path, editor, focus, window, cx);
            }
        }
        Self::coder_persist_open_tabs(cx);
        Ok(())
    }

    /// Split the focused editor pane; the new pane shows the current file.
    pub fn coder_split_editor(
        axis: CoderSplitAxis,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let focus = Self::state(cx).read(cx).coder.focused_pane;
        Self::coder_split_pane(focus, axis, editors, window, cx)
    }

    pub fn coder_unsplit_editor(
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let focus = Self::state(cx).read(cx).coder.focused_pane;
        Self::coder_close_pane(focus, editors, window, cx)
    }

    pub fn coder_set_split_axis(axis: CoderSplitAxis, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let CoderLayout::Split { axis: current, .. } = &mut s.coder.layout {
                *current = axis;
                s.commit_active_tab();
            }
        });
        Self::coder_persist_open_tabs(cx);
        Ok(())
    }

    pub fn coder_focus_group(
        group: u8,
        editors: &[Entity<CodeEditor>],
        cx: &mut App,
    ) -> BridgeResult<()> {
        let prev = Self::state(cx).read(cx).coder.focused_pane;
        let next = if Self::state(cx).read(cx).coder.layout.contains_leaf(group) {
            group
        } else {
            Self::state(cx).read(cx).coder.layout.first_leaf()
        };
        if prev != next {
            if let Some(editor) = editors.get(prev as usize) {
                Self::coder_flush_group_editor(editor, prev, cx);
            }
        }
        let active = Self::state(cx).update(cx, |s| {
            s.coder.focused_pane = next;
            s.commit_active_tab();
            s.coder.focused_active_path().map(str::to_string)
        });
        if let Some(path) = active {
            let _ = Self::coder_reveal_in_explorer(&path, cx);
        }
        Ok(())
    }

    pub fn coder_save_active(text: &str, window: &mut Window, cx: &mut App) -> BridgeResult<()> {
        let Some(path) = Self::state(cx)
            .read(cx)
            .coder
            .focused_active_path()
            .map(str::to_string)
        else {
            return Err(BridgeError::message("No active file to save"));
        };
        let is_image = Self::state(cx)
            .read(cx)
            .coder
            .open_files
            .iter()
            .find(|f| f.path == path)
            .is_some_and(|f| f.kind == CoderFileKind::Image);
        if is_image {
            return Err(BridgeError::message("Image previews are read-only"));
        }

        let (format_on_save, trim_trailing) = {
            let s = Self::state(cx).read(cx);
            (
                s.coder_settings.format_on_save,
                s.coder_settings.trim_trailing_whitespace,
            )
        };

        let mut out = text.to_string();
        if trim_trailing {
            out = coder_trim_trailing_whitespace(&out);
        }
        if format_on_save {
            if let Some(formatted) = Self::coder_format_for_save(&path, &out, cx) {
                out = formatted;
            }
        }

        if out != text {
            let pane = Self::state(cx).read(cx).coder.focused_pane;
            if let Some(editor) = Self::coder_lsp_editor_for_pane(pane, cx) {
                editor.update(cx, |e, cx| {
                    e.set_text(out.clone(), window, cx);
                });
            }
        }

        Self::coder_save_path_contents(&path, &out, cx)
    }

    /// Format buffer for save: rust-analyzer for Rust, Prettier for JS/TS/JSX/TSX.
    fn coder_format_for_save(path: &str, text: &str, cx: &App) -> Option<String> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "rs" => {
                let edits = Self::coder_lsp_format_document(path, cx)?;
                Some(halo_lsp::apply_text_edits(text, edits))
            }
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" => {
                let root = Self::state(cx).read(cx).coder.root_path.clone()?;
                coder_format_with_prettier(&root, path, text)
            }
            _ => None,
        }
    }

    /// Adjust image-preview zoom by `delta_pct` (e.g. `25` / `-25`). Clamped to 25–400.
    pub fn coder_image_zoom_by(path: &str, delta_pct: i16, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(file) = s
                .coder
                .open_files
                .iter_mut()
                .find(|f| f.path == path && f.kind == CoderFileKind::Image)
            {
                let next = (i32::from(file.zoom_pct) + i32::from(delta_pct)).clamp(25, 400);
                file.zoom_pct = next as u16;
            }
        });
        Ok(())
    }

    /// Set image-preview zoom percent directly (`100` = fit). Clamped to 25–400.
    pub fn coder_image_zoom_set(path: &str, zoom_pct: u16, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(file) = s
                .coder
                .open_files
                .iter_mut()
                .find(|f| f.path == path && f.kind == CoderFileKind::Image)
            {
                file.zoom_pct = zoom_pct.clamp(25, 400);
            }
        });
        Ok(())
    }

    /// True when `path` looks like a Markdown source file.
    pub fn coder_is_markdown_path(path: &str) -> bool {
        std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                let ext = ext.to_ascii_lowercase();
                ext == "md" || ext == "markdown" || ext == "mdx"
            })
    }

    /// Whether this path is currently shown as rendered Markdown preview.
    pub fn coder_markdown_preview_active(path: &str, cx: &App) -> bool {
        Self::state(cx)
            .read(cx)
            .coder
            .markdown_preview_paths
            .iter()
            .any(|p| p == path)
    }

    /// Toggle rendered Markdown preview for `path` (flush editor first so unsaved edits show).
    pub fn coder_toggle_markdown_preview(
        path: &str,
        editors: &[Entity<CodeEditor>],
        group: u8,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if !Self::coder_is_markdown_path(path) {
            return Ok(());
        }
        if let Some(editor) = editors.get(group as usize) {
            Self::coder_flush_group_editor(editor, group, cx);
        }
        Self::state(cx).update(cx, |s| {
            if let Some(ix) = s
                .coder
                .markdown_preview_paths
                .iter()
                .position(|p| p == path)
            {
                s.coder.markdown_preview_paths.remove(ix);
            } else {
                s.coder.markdown_preview_paths.push(path.to_string());
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    #[allow(dead_code)]
    pub(super) fn coder_flush_active_editor(editor: &Entity<CodeEditor>, cx: &mut App) {
        let group = Self::state(cx).read(cx).coder.focused_pane;
        Self::coder_flush_group_editor(editor, group, cx);
    }

    pub(super) fn coder_flush_group_editor(editor: &Entity<CodeEditor>, group: u8, cx: &mut App) {
        let text = editor.read(cx).text(cx);
        Self::state(cx).update(cx, |s| {
            let Some(active) = s.coder.pane_active(group).map(str::to_string) else {
                return;
            };
            if let Some(file) = s.coder.open_files.iter_mut().find(|f| f.path == active) {
                if file.kind == CoderFileKind::Image {
                    return;
                }
                file.content = text.clone();
                file.dirty = text != file.baseline;
            }
            s.commit_active_tab();
        });
    }

    pub(super) fn coder_ensure_tab_in_group(path: &str, group: u8, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            coder_activate_in_group(&mut s.coder, path, group);
            s.commit_active_tab();
        });
    }

    #[allow(dead_code)]
    pub(super) fn coder_load_tab_into_editor(
        path: &str,
        editor: &Entity<CodeEditor>,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::coder_load_tab_into_group(path, editor, 0, window, cx)
    }

    pub(super) fn coder_load_tab_into_group(
        path: &str,
        editor: &Entity<CodeEditor>,
        group: u8,
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let path = path.to_string();

        let existing_kind = Self::state(cx).update(cx, |s| {
            coder_activate_in_group(&mut s.coder, &path, group);
            s.commit_active_tab();
            s.coder
                .open_files
                .iter()
                .find(|f| f.path == path)
                .map(|f| f.kind)
        });

        if existing_kind == Some(CoderFileKind::Image)
            || (existing_kind.is_none() && coder_is_image_path(&path))
        {
            if existing_kind.is_none() {
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
                });
            }
            let _ = Self::coder_reveal_in_explorer(&path, cx);
            return Ok(());
        }

        let highlighter = coder_highlighter_for_path(&path);
        let lsp_language = coder_lsp_language_for_path(&path);
        let contents = Self::state(cx).update(cx, |s| {
            s.coder
                .open_files
                .iter()
                .find(|f| f.path == path)
                .map(|f| f.content.clone())
        });

        let contents = match contents {
            Some(text) => text,
            None => {
                let disk = backend.coder_read_file(&path)?;
                let workspace = Self::state(cx).read(cx).coder.root_path.clone();
                let (contents, dirty, disk_changed) =
                    super::coder_backup::coder_apply_backup_restore(
                        backend.as_ref(),
                        workspace.as_deref(),
                        &path,
                        &disk,
                    );
                Self::state(cx).update(cx, |s| {
                    s.coder.open_files.push(CoderOpenFile {
                        path: path.clone(),
                        kind: CoderFileKind::Text,
                        baseline: disk,
                        content: contents.clone(),
                        dirty,
                        zoom_pct: 100,
                    });
                    s.commit_active_tab();
                    if disk_changed {
                        s.apply_ok_msg(format!(
                            "{path} changed on disk — restored your unsaved edits"
                        ));
                    }
                });
                contents
            }
        };

        editor.update(cx, |e, cx| {
            e.set_language(highlighter, cx);
            e.set_text(contents.clone(), window, cx);
        });
        Self::coder_lsp_did_open(&path, lsp_language, &contents, editor, group, cx);
        let _ = Self::coder_reveal_in_explorer(&path, cx);
        Ok(())
    }
}

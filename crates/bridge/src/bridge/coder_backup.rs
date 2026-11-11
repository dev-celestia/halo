//! Hot-exit backups: mirror dirty buffers, debounce disk writes, restore on open.

use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Debounce sequence for backup / autosave writes (shared across editors).
static BACKUP_SEQ: AtomicU64 = AtomicU64::new(0);

const BACKUP_DEBOUNCE_MS: u64 = 1_000;
/// Minimum interval between autosave-triggered history snapshots for the same file.
const AUTOSAVE_HISTORY_MIN_INTERVAL: Duration = Duration::from_secs(60);

fn should_push_autosave_history(path: &str) -> bool {
    static LAST_PUSH: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let map = LAST_PUSH.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut guard) = map.lock() else {
        return false;
    };
    let now = Instant::now();
    if let Some(prev) = guard.get(path) {
        if now.duration_since(*prev) < AUTOSAVE_HISTORY_MIN_INTERVAL {
            return false;
        }
    }
    if guard.len() > 256 {
        guard.retain(|_, t| now.duration_since(*t) < AUTOSAVE_HISTORY_MIN_INTERVAL * 2);
    }
    guard.insert(path.to_string(), now);
    true
}

impl Bridge {
    /// Mirror the editor buffer into [`CoderOpenFile`] and schedule a hot-exit
    /// backup (or autosave when that setting is on).
    pub fn coder_on_buffer_change(editor: &Entity<CodeEditor>, group: u8, cx: &mut App) {
        let text = editor.read(cx).text(cx);
        let (path, dirty, workspace, autosave) = Self::state(cx).update(cx, |s| {
            let Some(path) = s.coder.pane_active(group).map(str::to_string) else {
                return (None, false, None, false);
            };
            let Some(file) = s.coder.open_files.iter_mut().find(|f| f.path == path) else {
                return (None, false, None, false);
            };
            if file.kind == CoderFileKind::Image {
                return (None, false, None, false);
            }
            let was_dirty = file.dirty;
            file.dirty = text != file.baseline;
            file.content = text;
            let dirty = file.dirty;
            let workspace = s.coder.root_path.clone();
            let autosave = s.coder_settings.autosave;
            // Only clone the entire CoderWork into work_tabs immediately when
            // the tab's dirty indicator flips; otherwise defer until debounce/flush.
            if was_dirty != dirty {
                s.commit_active_tab();
            }
            (Some(path), dirty, workspace, autosave)
        });
        let Some(path) = path else {
            return;
        };
        let Some(workspace) = workspace else {
            return;
        };

        if !dirty {
            let Ok(backend) = Self::backend(cx) else {
                return;
            };
            let _ = backend.coder_delete_backup(&workspace, &path);
            return;
        }

        let seq = BACKUP_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            executor
                .timer(Duration::from_millis(BACKUP_DEBOUNCE_MS))
                .await;
            if BACKUP_SEQ.load(Ordering::SeqCst) != seq {
                return;
            }
            let _ = cx.update(|cx| {
                if autosave {
                    Self::coder_autosave_path(&path, cx)
                } else {
                    Self::coder_write_backup_for_path(&workspace, &path, cx)
                }
            });
        })
        .detach();
    }

    fn coder_write_backup_for_path(workspace: &str, path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| s.commit_active_tab());
        let content = Self::state(cx)
            .read(cx)
            .coder
            .open_files
            .iter()
            .find(|f| f.path == path)
            .map(|f| f.content.clone());
        let Some(content) = content else {
            return Ok(());
        };
        let still_dirty = Self::state(cx)
            .read(cx)
            .coder
            .open_files
            .iter()
            .find(|f| f.path == path)
            .is_some_and(|f| f.dirty);
        if !still_dirty {
            let _ = backend.coder_delete_backup(workspace, path);
            return Ok(());
        }
        backend.coder_write_backup(workspace, path, &content)
    }

    /// When autosave is on: write the real file, push history, clear backup.
    fn coder_autosave_path(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let (content, baseline, workspace) = {
            let s = Self::state(cx).read(cx);
            let file = s.coder.open_files.iter().find(|f| f.path == path);
            let Some(file) = file else {
                return Ok(());
            };
            if !file.dirty || file.kind == CoderFileKind::Image {
                return Ok(());
            }
            (
                file.content.clone(),
                file.baseline.clone(),
                s.coder.root_path.clone(),
            )
        };
        if baseline != content && should_push_autosave_history(path) {
            let _ = backend.coder_push_history(path, &baseline);
        }
        backend.coder_write_file(path, &content)?;
        if let Some(workspace) = workspace.as_deref() {
            let _ = backend.coder_delete_backup(workspace, path);
        }
        Self::state(cx).update(cx, |s| {
            if let Some(file) = s.coder.open_files.iter_mut().find(|f| f.path == path) {
                file.baseline = content.clone();
                file.content = content.clone();
                file.dirty = false;
            }
            s.commit_active_tab();
        });
        Self::coder_lsp_did_save(path, &content, cx);
        let _ = Self::git_refresh(cx);
        Ok(())
    }

    /// Flush every dirty text buffer to the hot-exit store (app quit).
    pub fn coder_flush_all_backups(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let (workspace, dirty) = {
            let s = Self::state(cx).read(cx);
            let Some(workspace) = s.coder.root_path.clone() else {
                return;
            };
            let dirty: Vec<(String, String)> = s
                .coder
                .open_files
                .iter()
                .filter(|f| f.dirty && f.kind == CoderFileKind::Text)
                .map(|f| (f.path.clone(), f.content.clone()))
                .collect();
            (workspace, dirty)
        };
        for (path, content) in dirty {
            let _ = backend.coder_write_backup(&workspace, &path, &content);
        }
    }

    /// Register a quit observer that persists dirty buffers and tears down child processes.
    pub fn coder_install_quit_flush(cx: &mut App) {
        cx.on_app_quit(|cx| {
            Self::coder_flush_all_backups(cx);
            Self::halo_agent_persist_all(cx);
            // Chrome is a separate process tree: quitting Halo without this
            // leaves every chat's browser running until the daemon idles out.
            Self::halo_agent_browser_close_all_blocking(cx);
            Self::coder_lsp_stop(cx);
            Self::coder_pty_kill_all(cx);
            let _ = Self::stop_proxy(cx);
            async {}
        })
        .detach();
    }

    /// Apply a local-history snapshot into the open buffer (leaves it dirty).
    pub fn coder_restore_local_history(
        path: &str,
        history_id: &str,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let text = backend.coder_read_history(path, history_id)?;
        // Ensure the file is open.
        let _ = Self::coder_open_file(path, editors, window, cx);
        Self::state(cx).update(cx, |s| {
            if let Some(file) = s.coder.open_files.iter_mut().find(|f| f.path == path) {
                file.content = text.clone();
                file.dirty = text != file.baseline;
            }
            s.commit_active_tab();
            s.apply_ok_msg(format!("Restored local history for {path}"));
        });
        // Push text into the focused editor.
        let group = Self::state(cx).read(cx).coder.focused_pane;
        if let Some(editor) = editors.get(group as usize) {
            let highlighter = super::util::coder_highlighter_for_path(path);
            editor.update(cx, |e, cx| {
                e.set_language(highlighter, cx);
                e.set_text(text.clone(), window, cx);
            });
        }
        // Persist hot-exit backup for the restored dirty content.
        if let Some(workspace) = Self::state(cx).read(cx).coder.root_path.clone() {
            let _ = backend.coder_write_backup(&workspace, path, &text);
        }
        Ok(())
    }

    /// Newest-first local history entries for the explorer menu.
    pub fn coder_list_local_history(
        path: &str,
        cx: &App,
    ) -> Vec<halo_backend::commands::coder_backup::HistoryEntry> {
        let Ok(backend) = Self::backend(cx) else {
            return Vec::new();
        };
        backend.coder_list_history(path).unwrap_or_default()
    }
}

/// After loading disk text, overlay a hot-exit backup when one exists.
pub(super) fn coder_apply_backup_restore(
    backend: &dyn Backend,
    workspace: Option<&str>,
    path: &str,
    disk: &str,
) -> (String, bool, bool) {
    let Some(workspace) = workspace else {
        return (disk.to_string(), false, false);
    };
    match backend.coder_load_backup(workspace, path, disk) {
        Ok(Some(restore)) => (restore.content, true, restore.disk_changed),
        _ => (disk.to_string(), false, false),
    }
}

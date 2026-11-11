//! Coder IDE: undo / redo stack replay.

use super::util::*;
use super::*;

impl Bridge {
    pub fn coder_undo(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let Some(op) = Self::state(cx).update(cx, |s| s.coder.undo_stack.pop()) else {
            Self::state(cx).update(cx, |s| {
                s.apply_ok_msg("Nothing to undo".into());
            });
            return Ok(());
        };

        let redo_op = match op {
            CoderFsOp::SoftDelete { path, trash_path } => {
                if trash_path.is_empty() {
                    return Err(BridgeError::message("Invalid undo state for delete"));
                }
                backend.coder_rename_path(&trash_path, &path)?;
                let parent = Self::coder_parent_dir(&path);
                let _ = Self::coder_reload_dir(&parent, cx);
                // Redo will soft-delete again (empty trash_path = re-trash).
                CoderFsOp::SoftDelete {
                    path,
                    trash_path: String::new(),
                }
            }
            CoderFsOp::Rename { from, to } => {
                backend.coder_rename_path(&to, &from)?;
                Self::state(cx).update(cx, |s| {
                    coder_remap_open_paths(&mut s.coder, &to, &from);
                });
                let _ = Self::coder_reload_dir(&Self::coder_parent_dir(&from), cx);
                let _ = Self::coder_reload_dir(&Self::coder_parent_dir(&to), cx);
                // Same op: redo renames from -> to again.
                CoderFsOp::Rename { from, to }
            }
            CoderFsOp::Create { path, is_dir } => {
                backend.coder_delete_path(&path)?;
                Self::state(cx).update(cx, |s| {
                    coder_close_paths_under(&mut s.coder, &path);
                });
                let _ = Self::coder_reload_dir(&Self::coder_parent_dir(&path), cx);
                CoderFsOp::Create { path, is_dir }
            }
        };

        Self::state(cx).update(cx, |s| {
            s.coder.redo_stack.push(redo_op);
            s.commit_active_tab();
            s.apply_ok_msg("Undone".into());
        });
        Ok(())
    }

    pub fn coder_redo(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let Some(op) = Self::state(cx).update(cx, |s| s.coder.redo_stack.pop()) else {
            Self::state(cx).update(cx, |s| {
                s.apply_ok_msg("Nothing to redo".into());
            });
            return Ok(());
        };

        let applied = match op {
            CoderFsOp::SoftDelete { path, trash_path } => {
                if !trash_path.is_empty() {
                    let _ = backend.coder_purge_trash(&trash_path);
                }
                let trash = backend.coder_trash_path(&path)?;
                Self::state(cx).update(cx, |s| {
                    coder_close_paths_under(&mut s.coder, &path);
                });
                let _ = Self::coder_reload_dir(&Self::coder_parent_dir(&path), cx);
                CoderFsOp::SoftDelete {
                    path,
                    trash_path: trash,
                }
            }
            CoderFsOp::Rename { from, to } => {
                backend.coder_rename_path(&from, &to)?;
                Self::state(cx).update(cx, |s| {
                    coder_remap_open_paths(&mut s.coder, &from, &to);
                });
                let _ = Self::coder_reload_dir(&Self::coder_parent_dir(&to), cx);
                let _ = Self::coder_reload_dir(&Self::coder_parent_dir(&from), cx);
                CoderFsOp::Rename { from, to }
            }
            CoderFsOp::Create { path, is_dir } => {
                if is_dir {
                    backend.coder_create_dir(&path)?;
                } else {
                    backend.coder_create_file(&path)?;
                }
                let _ = Self::coder_reload_dir(&Self::coder_parent_dir(&path), cx);
                CoderFsOp::Create { path, is_dir }
            }
        };

        Self::state(cx).update(cx, |s| {
            s.coder.undo_stack.push(applied);
            s.commit_active_tab();
            s.apply_ok_msg("Redone".into());
        });
        Ok(())
    }
}

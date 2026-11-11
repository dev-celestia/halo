//! Coder undo-stack helpers.

use super::*;

pub(crate) const CODER_UNDO_LIMIT: usize = 50;

pub(crate) fn coder_push_undo(coder: &mut CoderWork, op: CoderFsOp, backend: Option<&dyn Backend>) {
    coder.undo_stack.push(op);
    while coder.undo_stack.len() > CODER_UNDO_LIMIT {
        let dropped = coder.undo_stack.remove(0);
        if let CoderFsOp::SoftDelete { trash_path, .. } = dropped {
            if !trash_path.is_empty() {
                if let Some(b) = backend {
                    let _ = b.coder_purge_trash(&trash_path);
                }
            }
        }
    }
    for dropped in coder.redo_stack.drain(..) {
        if let CoderFsOp::SoftDelete { trash_path, .. } = dropped {
            if !trash_path.is_empty() {
                if let Some(b) = backend {
                    let _ = b.coder_purge_trash(&trash_path);
                }
            }
        }
    }
}

pub(crate) fn coder_clear_undo_stacks(coder: &mut CoderWork, backend: Option<&dyn Backend>) {
    for op in coder.undo_stack.drain(..).chain(coder.redo_stack.drain(..)) {
        if let CoderFsOp::SoftDelete { trash_path, .. } = op {
            if !trash_path.is_empty() {
                if let Some(b) = backend {
                    let _ = b.coder_purge_trash(&trash_path);
                }
            }
        }
    }
}

//! Workspace-confined path security: canonicalization, containment, mutability.

use super::*;

pub(crate) fn workspace_root() -> Option<PathBuf> {
    load_prefs()
        .root_path
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

/// Canonicalize `path` even when the leaf does not exist yet, by resolving the
/// deepest existing ancestor and re-appending the missing components.
///
/// `fs::canonicalize` alone fails on new files, which is exactly when a
/// confinement check matters most.
pub(crate) fn canonicalize_lenient(path: &Path) -> Result<PathBuf, String> {
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor = path.to_path_buf();

    loop {
        if let Ok(canonical) = cursor.canonicalize() {
            let mut resolved = canonical;
            for part in suffix.iter().rev() {
                resolved.push(part);
            }
            return Ok(resolved);
        }

        let Some(name) = cursor.file_name().map(|name| name.to_owned()) else {
            return Err(format!("Cannot resolve path: {}", path.display()));
        };
        suffix.push(name);

        if !cursor.pop() {
            return Err(format!("Cannot resolve path: {}", path.display()));
        }
    }
}

/// Reject paths outside the open project.
///
/// The Coder API takes absolute paths straight from the UI, so a bad path —
/// from a UI bug, a crafted workspace file, or an agent following instructions
/// in a hostile repo — used to reach anywhere on disk, including
/// `remove_dir_all`. Comparison happens after canonicalization so `..` and
/// symlinks cannot escape the root.
pub(crate) fn ensure_inside_workspace(path: &Path, action: &str) -> Result<(), String> {
    let Some(root) = workspace_root() else {
        // No project open: there is no root to confine to. Destructive calls
        // still require the caller to have picked a folder first, which the UI
        // enforces.
        return Ok(());
    };

    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let target = canonicalize_lenient(path)?;

    if !target.starts_with(&root) {
        return Err(format!(
            "{action}: refusing to touch {} — it is outside the open project ({})",
            path.display(),
            root.display()
        ));
    }

    Ok(())
}

/// Confinement plus the protected-name deny list, for operations that destroy
/// or overwrite data.
pub(crate) fn ensure_mutable(path: &Path, action: &str) -> Result<(), String> {
    ensure_inside_workspace(path, action)?;

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if PROTECTED_NAMES
        .iter()
        .any(|protected| protected.eq_ignore_ascii_case(name))
    {
        return Err(format!(
            "{action}: refusing to touch protected path {}",
            path.display()
        ));
    }

    // The project root itself is never a target for write/delete/rename.
    if let Some(root) = workspace_root()
        && let (Ok(root), Ok(target)) = (root.canonicalize(), canonicalize_lenient(path))
        && root == target
    {
        return Err(format!(
            "{action}: refusing to touch the project root itself ({})",
            root.display()
        ));
    }

    Ok(())
}

/// Delete without any confinement check. Only for paths this module already
/// validated (the app trash).
pub(crate) fn remove_path_raw(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

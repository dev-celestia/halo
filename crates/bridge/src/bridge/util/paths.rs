//! Coder entry lookup and ancestor-directory helpers.

use super::*;

pub(crate) fn coder_find_entry_mut<'a>(
    entries: &'a mut [CoderTreeEntry],
    path: &str,
) -> Option<&'a mut CoderTreeEntry> {
    for entry in entries {
        if entry.path == path {
            return Some(entry);
        }
        if let Some(children) = entry.children.as_mut() {
            if let Some(found) = coder_find_entry_mut(children, path) {
                return Some(found);
            }
        }
    }
    None
}

/// Ancestor directories of `file_path` under `root`, shallow → deep.
///
/// Empty when the file is outside the workspace or sits directly under root
/// (nothing to expand). Paths use the same `display()` form as the tree.
pub(crate) fn coder_ancestor_dirs(root: &str, file_path: &str) -> Vec<String> {
    let root_path = std::path::Path::new(root);
    let file = std::path::Path::new(file_path);
    let Ok(rel) = file.strip_prefix(root_path) else {
        return Vec::new();
    };
    let Some(parent) = rel.parent() else {
        return Vec::new();
    };
    if parent.as_os_str().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut acc = root_path.to_path_buf();
    for component in parent.components() {
        acc.push(component);
        out.push(acc.display().to_string());
    }
    out
}

#[cfg(test)]
mod ancestor_dirs_tests {
    use super::coder_ancestor_dirs;

    #[test]
    fn nested_file_lists_parents_shallow_to_deep() {
        let dirs = coder_ancestor_dirs("/proj", "/proj/apps/web/src/users-manager.tsx");
        assert_eq!(
            dirs,
            vec![
                "/proj/apps".to_string(),
                "/proj/apps/web".to_string(),
                "/proj/apps/web/src".to_string(),
            ]
        );
    }

    #[test]
    fn top_level_file_needs_no_expand() {
        assert!(coder_ancestor_dirs("/proj", "/proj/README.md").is_empty());
    }

    #[test]
    fn outside_workspace_is_empty() {
        assert!(coder_ancestor_dirs("/proj", "/other/file.rs").is_empty());
    }
}

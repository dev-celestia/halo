//! File Explorer — sandboxed local workspace.
//!
//! Mirrors `references/hexbuffer/src/pages/file-explorer`. Everything lives
//! under `Documents/Halo Files`: the app can only read and write inside that
//! sandbox, and every user-typed name is validated as a single path component.
//!
//! The Wordlists catalog lives in [`super::wordlists`] and the R2 client in
//! [`super::r2`]; both are separate apps that share the same sandbox root.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::*;

pub const LOCAL_DIR_NAME: &str = "Halo Files";

// ── safe path helpers (the doc's security posture) ─────────────────────────

/// True when `name` is a strict single path component: non-empty, no `.`, no
/// `..`, no separators. Guards every user-typed file/folder name.
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
}

/// Sanitize a slash-separated relative path into safe segments: strips `.`,
/// `..`, empty and separator-only segments. Used for every remote-derived
/// path (manifest hrefs) before joining under a controlled root.
pub fn safe_path_segments(value: &str) -> Vec<String> {
    value
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .map(|segment| segment.to_string())
        .collect()
}

/// Join `root` with sanitized `segments`, refusing anything that escapes.
pub fn safe_join(root: &Path, segments: &[String]) -> Option<PathBuf> {
    if segments.is_empty() {
        return None;
    }
    let mut path = root.to_path_buf();
    for segment in segments {
        if !is_safe_file_name(segment) {
            return None;
        }
        path.push(segment);
    }
    Some(path)
}

// ── DTOs ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExplorerViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileExplorerView {
    pub view_mode: ExplorerViewMode,
    pub root_dir: Option<String>,
    /// Current directory as a relative segment list (empty = root).
    pub local_segments: Vec<String>,
    pub local_items: Vec<LocalItem>,
    pub local_selected: Option<String>,
    pub local_loading: bool,
    pub local_search: String,
    pub local_creating: bool,
}

impl Default for FileExplorerView {
    fn default() -> Self {
        Self {
            view_mode: ExplorerViewMode::List,
            root_dir: None,
            local_segments: Vec::new(),
            local_items: Vec::new(),
            local_selected: None,
            local_loading: false,
            local_search: String::new(),
            local_creating: false,
        }
    }
}

impl FileExplorerView {
    pub fn local_current_dir(&self) -> Option<PathBuf> {
        let root = self.root_dir.as_deref()?;
        // Empty segments means the sandboxed root itself. `safe_join` refuses
        // empty segment lists (wordlist hrefs need ≥1 component), so handle
        // that case here — otherwise create/import/refresh silently fail at
        // the default landing directory.
        if self.local_segments.is_empty() {
            return Some(PathBuf::from(root));
        }
        safe_join(Path::new(root), &self.local_segments)
    }

    pub fn local_breadcrumbs(&self) -> Vec<(String, Vec<String>)> {
        let mut crumbs = vec![(LOCAL_DIR_NAME.to_string(), Vec::<String>::new())];
        let mut acc = Vec::new();
        for segment in &self.local_segments {
            acc.push(segment.clone());
            crumbs.push((segment.clone(), acc.clone()));
        }
        crumbs
    }

    pub fn local_filtered(&self) -> Vec<LocalItem> {
        let query = self.local_search.to_lowercase();
        self.local_items
            .iter()
            .filter(|item| query.is_empty() || item.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }
}

// ── Bridge commands ────────────────────────────────────────────────────────

/// `Documents/Halo Files`, shared by the File Explorer and Wordlists apps.
pub(crate) fn halo_files_root() -> Option<PathBuf> {
    let docs = dirs::document_dir()?;
    let root = docs.join(LOCAL_DIR_NAME);
    migrate_legacy_sandbox(&docs, &root);
    Some(root)
}

/// Rename the pre-rebrand sandbox (`Documents/Hexbuffer Files`) on first open.
///
/// User-imported files live here, so the rebrand must not strand them in a
/// directory the app no longer shows.
fn migrate_legacy_sandbox(docs: &Path, root: &Path) {
    let legacy = docs.join("Hexbuffer Files");
    if !root.exists() && legacy.is_dir() {
        let _ = fs::rename(&legacy, root);
    }
}

fn list_dir_sorted(path: &Path) -> Result<Vec<LocalItem>, String> {
    let mut items = Vec::new();
    let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let modified = meta.modified().ok().map(|time| {
            let datetime: chrono::DateTime<chrono::Utc> = time.into();
            datetime.format("%Y-%m-%d %H:%M").to_string()
        });
        items.push(LocalItem {
            name: name.clone(),
            path: entry.path().display().to_string(),
            is_dir: meta.is_dir(),
            size: (!meta.is_dir()).then_some(meta.len()),
            modified,
        });
    }
    items.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

impl Bridge {
    pub fn file_explorer_set_view_mode(mode: ExplorerViewMode, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.file_explorer.view_mode = mode);
    }

    /// Create the sandboxed root (+ a seeded readme) on first open, then load
    /// the listing.
    pub fn file_explorer_ensure_root(cx: &mut App) -> BridgeResult<()> {
        let Some(root) = halo_files_root() else {
            return Err(BridgeError::message("Documents directory unavailable"));
        };
        fs::create_dir_all(&root).map_err(|e| BridgeError::message(e.to_string()))?;

        let readme = root.join("readme.txt");
        if !readme.exists() {
            let _ = fs::write(
                &readme,
                "Halo Files\n\n\
                 This sandboxed workspace is managed by the File Explorer.\n\
                 Import files with the Import button or create folders here.\n",
            );
        }
        let sample_dir = root.join("diagrams");
        let _ = fs::create_dir_all(&sample_dir);

        Self::state(cx).update(cx, |s| {
            s.file_explorer.root_dir = Some(root.display().to_string());
        });
        Ok(())
    }

    pub fn file_explorer_refresh(cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).file_explorer.root_dir.is_none() {
            Self::file_explorer_ensure_root(cx)?;
        }
        let (dir, segments) = {
            let state = Self::state(cx).read(cx);
            let dir = state.file_explorer.local_current_dir();
            (dir, state.file_explorer.local_segments.clone())
        };
        let Some(dir) = dir else {
            return Err(BridgeError::message("File Explorer root unavailable"));
        };
        Self::state(cx).update(cx, |s| {
            s.file_explorer.local_loading = true;
        });
        match list_dir_sorted(&dir) {
            Ok(items) => Self::state(cx).update(cx, |s| {
                s.file_explorer.local_items = items;
                s.file_explorer.local_loading = false;
                s.file_explorer.local_segments = segments;
            }),
            Err(err) => {
                Self::state(cx).update(cx, |s| {
                    s.file_explorer.local_loading = false;
                });
                return Err(BridgeError::message(err));
            }
        }
        Ok(())
    }

    pub fn file_explorer_navigate(segments: Vec<String>, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.file_explorer.local_segments = segments;
            s.file_explorer.local_selected = None;
        });
        Self::file_explorer_refresh(cx)
    }

    pub fn file_explorer_navigate_up(cx: &mut App) -> BridgeResult<()> {
        let segments = Self::state(cx)
            .read(cx)
            .file_explorer
            .local_segments
            .clone();
        let mut segments = segments;
        segments.pop();
        Self::file_explorer_navigate(segments, cx)
    }

    pub fn file_explorer_select(path: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.file_explorer.local_selected = path;
        });
    }

    pub fn file_explorer_set_search(query: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.file_explorer.local_search = query.to_string());
    }

    pub fn file_explorer_open(path: &str, cx: &mut App) {
        cx.open_with_system(Path::new(path));
    }

    pub fn file_explorer_reveal(path: &str, cx: &mut App) {
        cx.reveal_path(Path::new(path));
    }

    pub fn file_explorer_create_folder(name: &str, cx: &mut App) -> BridgeResult<()> {
        if !is_safe_file_name(name) {
            return Err(BridgeError::message(
                "Folder name must be a single path component",
            ));
        }
        let dir = Self::state(cx)
            .read(cx)
            .file_explorer
            .local_current_dir()
            .ok_or_else(|| BridgeError::message("File Explorer root unavailable"))?;
        fs::create_dir_all(dir.join(name)).map_err(|e| BridgeError::message(e.to_string()))?;
        Self::state(cx).update(cx, |s| {
            s.file_explorer.local_creating = false;
            s.apply_ok_msg(format!("Created folder {name}"));
        });
        Self::file_explorer_refresh(cx)
    }

    pub fn file_explorer_rename(old_path: &str, new_name: &str, cx: &mut App) -> BridgeResult<()> {
        if !is_safe_file_name(new_name) {
            return Err(BridgeError::message(
                "Name must be a single path component without slashes",
            ));
        }
        let old = Path::new(old_path);
        let Some(parent) = old.parent() else {
            return Err(BridgeError::message("Cannot rename this item"));
        };
        fs::rename(old, parent.join(new_name)).map_err(|e| BridgeError::message(e.to_string()))?;
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg(format!("Renamed to {new_name}"));
        });
        Self::file_explorer_refresh(cx)
    }

    pub fn file_explorer_delete(path: &str, cx: &mut App) -> BridgeResult<()> {
        let target = Path::new(path);
        let is_dir = target.is_dir();
        let result = if is_dir {
            fs::remove_dir_all(target)
        } else {
            fs::remove_file(target)
        };
        result.map_err(|e| BridgeError::message(e.to_string()))?;
        Self::state(cx).update(cx, |s| {
            if s.file_explorer.local_selected.as_deref() == Some(path) {
                s.file_explorer.local_selected = None;
            }
            s.apply_ok_msg("Deleted".to_string());
        });
        Self::file_explorer_refresh(cx)
    }

    /// Copy files picked via the native dialog into the sandboxed root (or
    /// current directory).
    pub fn file_explorer_import(paths: Vec<PathBuf>, cx: &mut App) -> BridgeResult<()> {
        let dir = Self::state(cx)
            .read(cx)
            .file_explorer
            .local_current_dir()
            .ok_or_else(|| BridgeError::message("File Explorer root unavailable"))?;
        let mut imported = 0usize;
        for source in paths {
            let Some(name) = source.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            if !is_safe_file_name(&name) {
                continue;
            }
            let dest = dir.join(&name);
            if source.is_dir() {
                continue;
            }
            if fs::copy(&source, &dest).is_ok() {
                imported += 1;
            }
        }
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg(format!("Imported {imported} file(s)"));
        });
        Self::file_explorer_refresh(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_sandbox_renames_into_place() {
        let docs = std::env::temp_dir().join(format!("halo-sandbox-test-{}", uuid::Uuid::new_v4()));
        let legacy = docs.join("Hexbuffer Files");
        std::fs::create_dir_all(legacy.join("diagrams")).unwrap();
        std::fs::write(legacy.join("notes.txt"), b"keep").unwrap();

        let root = docs.join(LOCAL_DIR_NAME);
        migrate_legacy_sandbox(&docs, &root);

        assert!(!legacy.exists());
        assert_eq!(std::fs::read(root.join("notes.txt")).unwrap(), b"keep");

        let _ = std::fs::remove_dir_all(&docs);
    }

    #[test]
    fn safe_file_name_rejects_traversal() {
        assert!(is_safe_file_name("readme.txt"));
        assert!(is_safe_file_name("my-folder_v2"));
        assert!(!is_safe_file_name(""));
        assert!(!is_safe_file_name("."));
        assert!(!is_safe_file_name(".."));
        assert!(!is_safe_file_name("a/b"));
        assert!(!is_safe_file_name("a\\b"));
        assert!(!is_safe_file_name("C:evil"));
    }

    #[test]
    fn safe_segments_strip_traversal() {
        assert_eq!(
            safe_path_segments("../../etc/passwd"),
            vec!["etc", "passwd"]
        );
        assert_eq!(
            safe_path_segments("dns/subdomains.txt"),
            vec!["dns", "subdomains.txt"]
        );
        assert_eq!(safe_path_segments("a//b\\..\\c"), vec!["a", "b", "c"]);
        assert!(safe_path_segments("..").is_empty());
        assert!(safe_path_segments("").is_empty());
    }

    #[test]
    fn safe_join_refuses_escape() {
        let root = Path::new("/tmp/hb");
        assert_eq!(
            safe_join(root, &["a".into(), "b.txt".into()]),
            Some(PathBuf::from("/tmp/hb/a/b.txt"))
        );
        // Empty segment lists are refused by `safe_join` (caller must handle
        // the sandboxed root itself via `local_current_dir`).
        assert!(safe_join(root, &[]).is_none());
        // `..` never survives sanitize, so a join always stays under root.
        let segments = safe_path_segments("../evil");
        assert!(safe_join(root, &segments).is_none() || !segments.contains(&"..".to_string()));
    }

    #[test]
    fn local_current_dir_includes_sandbox_root() {
        let mut view = FileExplorerView::default();
        assert!(view.local_current_dir().is_none());

        view.root_dir = Some("/tmp/halo-files".into());
        assert_eq!(
            view.local_current_dir(),
            Some(PathBuf::from("/tmp/halo-files"))
        );

        view.local_segments = vec!["diagrams".into()];
        assert_eq!(
            view.local_current_dir(),
            Some(PathBuf::from("/tmp/halo-files/diagrams"))
        );
    }
}

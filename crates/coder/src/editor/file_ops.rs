//! Filesystem operations: list, read, write, create, rename, delete, trash.

use super::*;

use super::paths::*;

pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, String> {
    let root = PathBuf::from(path);
    ensure_inside_workspace(&root, "list")?;
    if !root.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" {
            continue;
        }
        entries.push(DirEntry {
            name,
            path: entry.path().display().to_string(),
            is_dir: meta.is_dir(),
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

pub(crate) fn ensure_readable_file(path: &Path, max_bytes: u64) -> Result<(), String> {
    ensure_inside_workspace(path, "read")?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > max_bytes {
        return Err(format!(
            "File too large ({} bytes; max {max_bytes})",
            meta.len()
        ));
    }
    Ok(())
}

pub fn read_file(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    ensure_readable_file(path, MAX_FILE_BYTES)?;
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.iter().any(|&b| b == 0) {
        return Err("Binary files are not supported in Coder v1".into());
    }
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8".into())
}

/// Confirm an image file is readable under the open project (no bytes loaded).
pub fn ensure_image_file(path: &str) -> Result<(), String> {
    ensure_readable_file(Path::new(path), MAX_IMAGE_BYTES)
}

pub fn write_file(path: &str, contents: &str) -> Result<(), String> {
    let path = Path::new(path);
    ensure_mutable(path, "write")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, contents).map_err(|e| e.to_string())
}

/// Create an empty file. Fails if the path already exists.
pub fn create_file(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    ensure_mutable(path, "create")?;
    if path.exists() {
        return Err(format!("Already exists: {}", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::File::create(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Create a directory (and parents). Fails if a file already exists at `path`.
pub fn create_dir(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    ensure_mutable(path, "create")?;
    if path.is_file() {
        return Err(format!("A file already exists: {}", path.display()));
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

pub fn rename_path(from: &str, to: &str) -> Result<(), String> {
    let from = Path::new(from);
    let to = Path::new(to);
    ensure_mutable(from, "rename")?;
    ensure_mutable(to, "rename")?;
    if !from.exists() {
        return Err(format!("Not found: {}", from.display()));
    }
    if to.exists() {
        return Err(format!("Already exists: {}", to.display()));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(from, to).map_err(|e| e.to_string())
}

pub fn delete_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    ensure_mutable(path, "delete")?;
    if !path.exists() {
        return Err(format!("Not found: {}", path.display()));
    }
    remove_path_raw(path)
}

/// Soft-delete: move `path` into the app trash and return the trash location.
pub fn move_to_trash(path: &str) -> Result<String, String> {
    let src = Path::new(path);
    ensure_mutable(src, "trash")?;
    if !src.exists() {
        return Err(format!("Not found: {}", src.display()));
    }
    let name = src
        .file_name()
        .ok_or_else(|| format!("No file name for {}", src.display()))?;
    let trash_dir = halo_core::paths::get_shared_app_dir()
        .join(TRASH_DIR)
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&trash_dir).map_err(|e| e.to_string())?;
    let dest = trash_dir.join(name);
    match fs::rename(src, &dest) {
        Ok(()) => Ok(dest.display().to_string()),
        Err(_) => {
            // Cross-device rename fails — copy then remove.
            if src.is_dir() {
                copy_dir_recursive(src, &dest)?;
                fs::remove_dir_all(src).map_err(|e| e.to_string())?;
            } else {
                fs::copy(src, &dest).map_err(|e| e.to_string())?;
                fs::remove_file(src).map_err(|e| e.to_string())?;
            }
            Ok(dest.display().to_string())
        }
    }
}

/// Permanently remove a trash entry (and its parent uuid folder when empty).
///
/// The path must resolve inside the app trash: this used to accept any path
/// and `remove_dir_all` it, which made it an arbitrary recursive delete.
pub fn purge_trash_entry(trash_path: &str) -> Result<(), String> {
    let path = Path::new(trash_path);
    let trash_root = halo_core::paths::get_shared_app_dir().join(TRASH_DIR);

    let Ok(trash_root) = trash_root.canonicalize() else {
        // Nothing has ever been trashed.
        return Ok(());
    };
    let target = canonicalize_lenient(path)?;

    if target == trash_root || !target.starts_with(&trash_root) {
        return Err(format!(
            "Refusing to purge {}: not inside the app trash",
            path.display()
        ));
    }

    if target.exists() {
        remove_path_raw(&target)?;
    }

    // Drop the now-empty `<uuid>` folder that held it.
    if let Some(parent) = target.parent() {
        if parent != trash_root
            && parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| uuid::Uuid::parse_str(name).is_ok())
        {
            let _ = fs::remove_dir(parent);
        }
    }

    Ok(())
}

/// Remove all entries from the app soft-delete trash (e.g. on startup when undo stack is empty).
pub fn purge_all_trash() {
    let trash_root = halo_core::paths::get_shared_app_dir().join(TRASH_DIR);
    if trash_root.exists() {
        let _ = fs::remove_dir_all(&trash_root);
    }
}

/// Copy a file or directory to `to`. Directories are copied recursively.
pub fn copy_path(from: &str, to: &str) -> Result<(), String> {
    let from = Path::new(from);
    let to = Path::new(to);
    ensure_mutable(to, "copy")?;
    if !from.exists() {
        return Err(format!("Not found: {}", from.display()));
    }
    if to.exists() {
        return Err(format!("Already exists: {}", to.display()));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if from.is_dir() {
        copy_dir_recursive(from, to)
    } else {
        fs::copy(from, to).map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub(crate) fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        if meta.is_dir() {
            copy_dir_recursive(&src, &dest)?;
        } else {
            fs::copy(&src, &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Pick a free sibling path like `name copy`, `name copy 2`, …
pub fn unique_sibling_path(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    let parent = path
        .parent()
        .ok_or_else(|| format!("No parent for {}", path.display()))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    for i in 1..10_000 {
        let name = if i == 1 {
            format!("{stem} copy{ext}")
        } else {
            format!("{stem} copy {i}{ext}")
        };
        let candidate = parent.join(&name);
        if !candidate.exists() {
            return Ok(candidate.display().to_string());
        }
    }
    Err("Could not find a free name for duplicate".into())
}

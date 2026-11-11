//! Local filesystem helpers for the Coder IDE.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Soft cap for text files opened in the editor (2 MiB).
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Soft cap for image previews (32 MiB).
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// Soft cap for remembered project folders in the switcher.
const MAX_RECENT_PROJECTS: usize = 12;

/// Directory (under the app data dir) holding soft-deleted files.
const TRASH_DIR: &str = "coder-trash";

/// Names that are never deleted or overwritten, even inside the project root.
///
/// These are the paths whose loss is unrecoverable and whose presence in a
/// repository is routine — a hostile repo plus a plausible agent instruction
/// ("clean up the stale files") should not be able to reach them.
const PROTECTED_NAMES: &[&str] = &[
    ".git",
    ".ssh",
    ".gnupg",
    ".aws",
    ".config",
    ".halo",
    ".hexbuffer",
    "Library",
    ".Trash",
];

mod file_ops;
mod layout;
mod paths;
mod prefs;
#[cfg(test)]
mod tests;

pub use file_ops::*;
pub use layout::*;
pub use prefs::*;

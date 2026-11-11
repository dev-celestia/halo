//! Git helpers for the Source Control app (shells out to `git`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Default)]
pub struct GitFileEntry {
    pub path: String,
    /// Short porcelain status, e.g. `M`, `A`, `D`, `?`, `R`.
    pub status: String,
    /// Set for renames (`R  old -> new`): the path before the rename.
    pub previous_path: Option<String>,
    /// Lines added on this side of the index (`+N` in the row badge).
    pub additions: u32,
    /// Lines removed on this side of the index (`-N` in the row badge).
    pub deletions: u32,
}

#[derive(Clone, Debug, Default)]
pub struct GitStatus {
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Configured remotes (`origin`, `upstream`, …) for Push to… menus.
    pub remotes: Vec<String>,
    /// Local branch names for Push to… branch pickers.
    pub local_branches: Vec<String>,
    pub staged: Vec<GitFileEntry>,
    pub unstaged: Vec<GitFileEntry>,
    pub untracked: Vec<GitFileEntry>,
    /// Paths matching `.gitignore` / exclude rules (`!!` in porcelain).
    pub ignored: Vec<GitFileEntry>,
    /// Set when the folder is not inside a git work tree.
    pub not_a_repo: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitCommitDto {
    pub sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub refs: Vec<String>,
    pub subject: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitBranchInfoDto {
    pub name: String,
    pub is_current: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Fully merged into HEAD, so `git branch -d` would accept it. Drives the
    /// safe/forced choice in the branch menu instead of parsing stderr.
    #[serde(default)]
    pub merged: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitRemoteInfoDto {
    pub name: String,
    pub branches: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitStashDto {
    pub index: usize,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitWorktreeDto {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitCommitFileDto {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
}

mod mutations;
mod process;
mod queries;
mod status;
#[cfg(test)]
mod tests;

pub use mutations::*;
pub use queries::*;
pub use status::*;

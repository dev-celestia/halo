//! Source Control — top-level tool for changes, commit, sync, pull requests,
//! and diff review.
//!
//! Separate from [`crate::pages::coder`]. Explorer still asks this module for
//! decoration helpers; the terminal dock hosts the Git Log tab body; the coder
//! status bar hosts the project / branch / changes chips.

mod decor;
pub mod diff;
pub(crate) use diff as diff_view;
pub mod changes;
mod git_alert;
pub mod graph;
mod log;
mod page;
mod projects;
pub mod pulls;
mod status;

pub use graph::render_graph_workspace;
pub use page::render_source_control;

pub(crate) use git_alert::render_git_alert;

pub use changes::{GitChangesList, render_review};
pub(crate) use decor::{
    GitDecor, GitDecorMap, author_avatar, decor_map, rel_to_repo, repo_root, status_color,
    status_dot,
};
pub(crate) use log::{
    git_log_open, git_log_refresh_button, git_log_tab_button, render_git_log_pane,
};
pub(crate) use projects::render_project_chip;
pub(crate) use status::{render_branch_chip, render_changes_chip};

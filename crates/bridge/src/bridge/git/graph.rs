//! Git Graph and Git Output commands: graph refresh, commit selection, branches.

use super::*;

// ── Git Graph & Branch Explorer ────────────────────────────────────────────

impl Bridge {
    pub fn git_graph_set_open_sections(
        open_ixs: &[usize],
        keys: &[String],
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.git_graph.set_open_sections(open_ixs, keys);
        });
        Ok(())
    }

    /// Expand or collapse the Git Output dock under the graph.
    pub fn git_output_toggle(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.git_output_open = !s.git_output_open;
            s.commit_active_tab();
        });
    }

    /// Explicit close from the dock header.
    pub fn git_output_close(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.git_output_open = false;
            s.commit_active_tab();
        });
    }

    /// Empty the Git Output dock but keep it open so the clear is visible.
    pub fn git_output_clear(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.git_output.clear();
            s.commit_active_tab();
        });
    }

    pub fn git_graph_refresh(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = {
            let state = Self::state(cx).read(cx);
            if let Some(tab_repo) = state.git_graph.active_repo() {
                Some(tab_repo.to_string())
            } else if let Some(coder_root) = &state.coder.root_path {
                Some(coder_root.clone())
            } else {
                None
            }
        };

        let Some(root) = root else {
            Self::state(cx).update(cx, |s| {
                s.git_graph.error = Some("No folder open".into());
                s.git_graph.is_loading = false;
            });
            return Ok(());
        };

        Self::state(cx).update(cx, |s| {
            if s.git_graph.repo_tabs.is_empty() {
                s.git_graph.repo_tabs.push(root.clone());
                s.git_graph.active_repo_tab = 0;
            }
            s.git_graph.is_loading = true;
            s.git_graph.error = None;
        });

        // Fetch graph log, branches, remotes, tags, stashes, worktrees. Each
        // read fails into the Git Output dock instead of vanishing — a repo
        // mid-rebase or a corrupt index otherwise showed an empty graph with
        // no explanation.
        let raw_commits =
            Self::git_graph_fetch("Load commit graph", cx, backend.git_graph_log(&root, 300));
        let raw_branches =
            Self::git_graph_fetch("List branches", cx, backend.git_list_branches(&root));
        let raw_remotes =
            Self::git_graph_fetch("List remotes", cx, backend.git_list_remotes(&root));
        let raw_tags = Self::git_graph_fetch("List tags", cx, backend.git_list_tags(&root));
        let raw_stashes =
            Self::git_graph_fetch("List stashes", cx, backend.git_list_stashes(&root));
        let raw_worktrees =
            Self::git_graph_fetch("List worktrees", cx, backend.git_list_worktrees(&root));

        let commits = compute_graph_lanes(raw_commits);

        let branches = raw_branches
            .into_iter()
            .map(|b| crate::work::GitGraphBranch {
                name: b.name,
                is_current: b.is_current,
                upstream: b.upstream,
                ahead: b.ahead,
                behind: b.behind,
                merged: b.merged,
            })
            .collect();

        let remotes = raw_remotes
            .into_iter()
            .map(|r| crate::work::GitGraphRemote {
                name: r.name,
                branches: r.branches,
            })
            .collect();

        let stashes = raw_stashes
            .into_iter()
            .map(|s| crate::work::GitGraphStash {
                index: s.index,
                message: s.message,
                timestamp: s.timestamp,
            })
            .collect();

        let worktrees = raw_worktrees
            .into_iter()
            .map(|w| crate::work::GitGraphWorktree {
                path: w.path,
                head: w.head,
                branch: w.branch,
            })
            .collect();

        let first_sha = commits.first().map(|c| c.sha.clone());

        Self::state(cx).update(cx, |s| {
            s.git_graph.commits = commits;
            s.git_graph.branches = branches;
            s.git_graph.remotes = remotes;
            s.git_graph.tags = raw_tags;
            s.git_graph.stashes = stashes;
            s.git_graph.worktrees = worktrees;
            s.git_graph.is_loading = false;
        });

        let selected = Self::state(cx).read(cx).git_graph.selected_commit.clone();
        if let Some(sha) = selected {
            let _ = Self::git_graph_select_commit(&sha, cx);
        } else if let Some(sha) = first_sha {
            let _ = Self::git_graph_select_commit(&sha, cx);
        }

        Ok(())
    }

    pub fn git_graph_select_commit(sha: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = {
            let state = Self::state(cx).read(cx);
            state
                .git_graph
                .active_repo()
                .map(str::to_string)
                .or_else(|| state.coder.root_path.clone())
        };
        let Some(root) = root else {
            return Ok(());
        };

        Self::state(cx).update(cx, |s| {
            s.git_graph.selected_commit = Some(sha.to_string());
        });

        let raw_files = Self::git_graph_fetch(
            "Load commit files",
            cx,
            backend.git_commit_files(&root, sha),
        );
        let files: Vec<crate::work::GitGraphCommitFile> = raw_files
            .into_iter()
            .map(|f| crate::work::GitGraphCommitFile {
                path: f.path,
                status: f.status,
                additions: f.additions,
                deletions: f.deletions,
            })
            .collect();

        let first_file = files.first().map(|f| f.path.clone());

        Self::state(cx).update(cx, |s| {
            s.git_graph.commit_files = files;
        });

        if let Some(file_path) = first_file {
            let _ = Self::git_graph_select_commit_file(sha, &file_path, cx);
        } else {
            Self::state(cx).update(cx, |s| {
                s.git_graph.selected_commit_file = None;
                s.git_graph.commit_diff = None;
            });
        }

        Ok(())
    }

    pub fn git_graph_select_commit_file(sha: &str, path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = {
            let state = Self::state(cx).read(cx);
            state
                .git_graph
                .active_repo()
                .map(str::to_string)
                .or_else(|| state.coder.root_path.clone())
        };
        let Some(root) = root else {
            return Ok(());
        };

        let diff_text = Self::git_graph_fetch(
            "Load commit diff",
            cx,
            backend.git_commit_file_diff(&root, sha, path),
        );

        Self::state(cx).update(cx, |s| {
            s.git_graph.selected_commit_file = Some(path.to_string());
            s.git_graph.commit_diff = Some(crate::work::GitDiff {
                path: path.to_string(),
                staged: false,
                text: diff_text,
            });
        });

        Ok(())
    }

    pub fn git_graph_open_repo_tab(path: &str, cx: &mut App) -> BridgeResult<()> {
        let clean = path.trim().to_string();
        if clean.is_empty() {
            return Ok(());
        }
        Self::state(cx).update(cx, |s| {
            if let Some(idx) = s.git_graph.repo_tabs.iter().position(|p| p == &clean) {
                s.git_graph.active_repo_tab = idx;
            } else {
                s.git_graph.repo_tabs.push(clean);
                s.git_graph.active_repo_tab = s.git_graph.repo_tabs.len() - 1;
            }
            s.git_graph.selected_commit = None;
            s.git_graph.commit_files.clear();
            s.git_graph.commit_diff = None;
        });
        Self::git_graph_refresh(cx)
    }

    pub fn git_graph_close_repo_tab(index: usize, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if index < s.git_graph.repo_tabs.len() {
                s.git_graph.repo_tabs.remove(index);
                if s.git_graph.active_repo_tab >= s.git_graph.repo_tabs.len()
                    && !s.git_graph.repo_tabs.is_empty()
                {
                    s.git_graph.active_repo_tab = s.git_graph.repo_tabs.len() - 1;
                }
            }
        });
        let _ = Self::git_graph_refresh(cx);
    }

    pub fn git_graph_select_repo_tab(index: usize, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if index < s.git_graph.repo_tabs.len() {
                s.git_graph.active_repo_tab = index;
            }
        });
        let _ = Self::git_graph_refresh(cx);
    }

    pub fn git_graph_set_filter(query: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.git_graph.filter_query = query.to_string();
        });
    }

    pub fn git_graph_select_inspector_tab(tab: crate::work::GitInspectorTab, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.git_graph.selected_inspector_tab = tab;
        });
    }

    pub fn git_graph_checkout(branch: &str, cx: &mut App) -> BridgeResult<()> {
        let branch = branch.trim();
        if branch.is_empty() {
            return Ok(());
        }
        // The open diff and any ticks describe the branch we are leaving.
        Self::state(cx).update(cx, |s| {
            s.git_diff = None;
            s.git_selection.clear();
        });
        let label = format!("Switching to {branch}");
        let target = branch.to_string();
        let checkout = target.clone();
        let root = Self::git_graph_root(cx);
        Self::git_async_inner(
            label,
            cx,
            move |backend, root| backend.git_checkout(&root, &target),
            root,
            true,
            Some(checkout),
        )
    }

    /// Resolve a refused branch switch by stashing the colliding edits and
    /// retrying. The stash stays in the Stashes list, so nothing is lost.
    ///
    /// The block records the repository and branch of the failed attempt, so
    /// the retry lands on the same worktree the user clicked in — repo tabs can
    /// point away from the open project.
    pub fn git_recover_checkout(cx: &mut App) -> BridgeResult<()> {
        let Some(block) = Self::state(cx).read(cx).git.checkout_block.clone() else {
            return Ok(());
        };
        let branch = block.branch.clone();
        let from_graph = block.from_graph;
        let label = format!("Stashing changes and switching to {branch}");
        Self::git_async_inner(
            label,
            cx,
            move |backend, root| backend.git_stash_and_checkout(&root, &branch),
            Some(block.root),
            from_graph,
            None,
        )
    }

    /// Clear the Source Control error strip.
    pub fn git_dismiss_error(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let had_error = s.git.error.take().is_some();
            let had_block = s.git.checkout_block.take().is_some();
            if had_error || had_block {
                s.commit_active_tab();
            }
        });
    }

    /// Create a branch from `base` and switch to it (branch menu).
    pub fn git_branch_create_from(name: &str, base: &str, cx: &mut App) -> BridgeResult<()> {
        let name = name.trim().to_string();
        let base = base.trim().to_string();
        if name.is_empty() || base.is_empty() {
            return Ok(());
        }
        // Creating and switching rewrites the worktree, so the open diff and
        // the staged ticks belong to the branch we are leaving.
        Self::state(cx).update(cx, |s| {
            s.git_diff = None;
            s.git_selection.clear();
        });
        let label = format!("Creating {name} from {base}");
        let root = Self::git_graph_root(cx);
        Self::git_async_inner(
            label,
            cx,
            move |backend, root| backend.git_create_branch_from(&root, &name, &base),
            root,
            true,
            None,
        )
    }

    /// Rename a branch (branch menu). Renaming the current branch moves HEAD.
    pub fn git_branch_rename(from: &str, to: &str, cx: &mut App) -> BridgeResult<()> {
        let from = from.trim().to_string();
        let to = to.trim().to_string();
        if from.is_empty() || to.is_empty() || from == to {
            return Ok(());
        }
        let label = format!("Renaming {from} to {to}");
        let root = Self::git_graph_root(cx);
        Self::git_async_inner(
            label,
            cx,
            move |backend, root| backend.git_rename_branch(&root, &from, &to),
            root,
            true,
            None,
        )
    }

    /// Delete a branch (branch menu). `force` picks `-D` for unmerged branches;
    /// the menu decides that from [`crate::work::GitGraphBranch::merged`].
    pub fn git_branch_delete(branch: &str, force: bool, cx: &mut App) -> BridgeResult<()> {
        let branch = branch.trim().to_string();
        if branch.is_empty() {
            return Ok(());
        }
        let label = format!("Deleting {branch}");
        let root = Self::git_graph_root(cx);
        Self::git_async_inner(
            label,
            cx,
            move |backend, root| backend.git_delete_branch(&root, &branch, force),
            root,
            true,
            None,
        )
    }

    /// Copy a branch name to the clipboard (branch menu).
    pub fn git_copy_branch_name(branch: &str, cx: &mut App) -> BridgeResult<()> {
        cx.write_to_clipboard(ClipboardItem::new_string(branch.to_string()));
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg("Copied branch name".into());
        });
        Ok(())
    }

    /// Hand a branch brief to the Agent composer without sending it.
    ///
    /// The shell drains `pending_agent_prompt`, opens the Agent tab and fills
    /// the composer (see `HaloApp::apply_pending_agent_prompt`), so the user
    /// reviews the prompt and decides when to run it.
    pub fn git_queue_agent_prompt(prompt: String, cx: &mut App) -> BridgeResult<()> {
        if prompt.trim().is_empty() {
            return Ok(());
        }
        Self::state(cx).update(cx, |s| {
            s.pending_agent_prompt = Some(prompt);
        });
        Ok(())
    }

    pub fn take_pending_agent_prompt(cx: &mut App) -> Option<String> {
        Self::state(cx).update(cx, |s| s.pending_agent_prompt.take())
    }
}

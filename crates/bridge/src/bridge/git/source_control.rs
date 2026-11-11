//! Source Control commands: status, staging, commit, sync, pulls, and diffs.

use super::*;

impl Bridge {
    pub fn git_refresh(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let Some(root) = Self::state(cx).read(cx).coder.root_path.clone() else {
            Self::state(cx).update(cx, |s| {
                s.git = crate::work::GitState::default();
                s.git.error = Some("No folder open".into());
                s.git_selection.clear();
                s.git_diff = None;
                s.commit_active_tab();
            });
            return Ok(());
        };
        match backend.git_status(&root) {
            Ok(git) => {
                Self::state(cx).update(cx, |s| {
                    s.git = git;
                    prune_selection(s);
                    s.commit_active_tab();
                });
                ensure_git_diff_open(cx);
            }
            Err(err) => {
                let detail = err.to_string();
                Self::state(cx).update(cx, |s| {
                    s.git.error = Some(detail.clone());
                    s.git.checkout_block = None;
                    s.push_git_output("Refresh status", detail);
                    s.commit_active_tab();
                });
            }
        }
        Ok(())
    }

    /// Expand or collapse the Ignored section in Source Control.
    pub fn git_toggle_ignored(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.git_ignored_open = !s.git_ignored_open;
            s.commit_active_tab();
        });
    }

    pub fn git_stage(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        Self::git_logged("Stage file", cx, || {
            backend.git_stage(&root, &[path.to_string()])
        })?;
        Self::git_refresh(cx)
    }

    pub fn git_unstage(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        Self::git_logged("Unstage file", cx, || {
            backend.git_unstage(&root, &[path.to_string()])
        })?;
        Self::git_refresh(cx)
    }

    pub fn git_stage_all(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        Self::git_logged("Stage all", cx, || backend.git_stage_all(&root))?;
        Self::state(cx).update(cx, |s| s.git_selection.clear());
        Self::git_refresh(cx)
    }

    pub fn git_unstage_all(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        Self::git_logged("Unstage all", cx, || backend.git_unstage_all(&root))?;
        Self::state(cx).update(cx, |s| s.git_selection.clear());
        Self::git_refresh(cx)
    }

    /// Tick or untick one row for bulk staging.
    pub fn git_toggle_select(path: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            match s.git_selection.iter().position(|p| p == path) {
                Some(index) => {
                    s.git_selection.remove(index);
                }
                None => s.git_selection.push(path.to_string()),
            }
            s.commit_active_tab();
        });
    }

    pub fn git_clear_selection(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.git_selection.clear();
            s.commit_active_tab();
        });
    }

    /// Stage every ticked path that is not already in the index.
    pub fn git_stage_selected(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        let paths = {
            let state = Self::state(cx);
            let state = state.read(cx);
            let git = &state.git;
            git.unstaged
                .iter()
                .chain(git.untracked.iter())
                .filter(|f| state.git_selection.iter().any(|p| p == &f.path))
                .map(|f| f.path.clone())
                .collect::<Vec<_>>()
        };
        if paths.is_empty() {
            return Ok(());
        }
        Self::git_logged("Stage selected", cx, || backend.git_stage(&root, &paths))?;
        Self::state(cx).update(cx, |s| s.git_selection.clear());
        Self::git_refresh(cx)
    }

    /// Unstage every ticked path that is currently in the index.
    pub fn git_unstage_selected(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        let paths = {
            let state = Self::state(cx);
            let state = state.read(cx);
            let git = &state.git;
            git.staged
                .iter()
                .filter(|f| state.git_selection.iter().any(|p| p == &f.path))
                .map(|f| f.path.clone())
                .collect::<Vec<_>>()
        };
        if paths.is_empty() {
            return Ok(());
        }
        Self::git_logged("Unstage selected", cx, || {
            backend.git_unstage(&root, &paths)
        })?;
        Self::state(cx).update(cx, |s| s.git_selection.clear());
        Self::git_refresh(cx)
    }

    /// Discard a path: untracked files are deleted from disk, tracked files are
    /// restored from the index. The UI owns the confirmation step.
    pub fn git_discard(path: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        Self::git_logged("Discard changes", cx, || backend.git_discard(&root, path))?;
        Self::git_refresh(cx)
    }

    /// Load the unified diff for one path into the panel.
    ///
    /// `staged` selects index-vs-HEAD; otherwise the worktree is diffed
    /// against the index (untracked files diff against `/dev/null`).
    pub fn git_diff(path: &str, staged: bool, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        match backend.git_diff(&root, path, staged) {
            Ok(text) => {
                Self::state(cx).update(cx, |s| {
                    s.git_diff = Some(crate::work::GitDiff {
                        path: path.to_string(),
                        staged,
                        text,
                    });
                });
                Ok(())
            }
            Err(err) => Self::git_fail("Load diff", err, cx),
        }
    }

    pub fn git_close_diff(cx: &mut App) {
        Self::state(cx).update(cx, |s| s.git_diff = None);
    }

    /// Stage (`reverse = false`) or unstage one hunk of the open diff.
    ///
    /// The patch is re-read afterwards so the pane drops the hunk it just
    /// moved, rather than leaving a button that would apply twice.
    pub fn git_apply_hunk(patch: &str, reverse: bool, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        let Some(diff) = Self::state(cx).read(cx).git_diff.clone() else {
            return Ok(());
        };
        match backend.git_apply_hunk(&root, patch, reverse) {
            Ok(msg) => {
                Self::state(cx).update(cx, |s| s.apply_ok_msg(msg));
                Self::git_refresh(cx)?;
                Self::git_diff(&diff.path, diff.staged, cx)
            }
            Err(err) => Self::git_fail("Apply hunk", err, cx),
        }
    }

    /// Flip the pane between unified and split layout.
    pub fn git_toggle_split(cx: &mut App) {
        Self::state(cx).update(cx, |s| s.git_diff_split = !s.git_diff_split);
    }

    /// Soft-wrap long lines, or scroll each pane horizontally.
    pub fn git_toggle_wrap(cx: &mut App) {
        Self::state(cx).update(cx, |s| s.git_diff_wrap = !s.git_diff_wrap);
    }

    pub fn git_commit(message: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        match backend.git_commit(&root, message) {
            Ok(msg) => {
                Self::state(cx).update(cx, |s| s.apply_ok_msg(msg));
                Self::git_refresh(cx)
            }
            Err(err) => Self::git_fail("Commit", err, cx),
        }
    }

    pub fn git_amend(message: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let root = Self::coder_require_root(cx)?;
        match backend.git_amend(&root, message) {
            Ok(msg) => {
                Self::state(cx).update(cx, |s| s.apply_ok_msg(msg));
                Self::git_refresh(cx)
            }
            Err(err) => Self::git_fail("Amend commit", err, cx),
        }
    }

    /// Switch to an existing branch. Runs off the UI thread — checkout rewrites
    /// the worktree.
    ///
    /// A refusal caused by local edits is remembered as a
    /// [`GitCheckoutBlock`](crate::work::GitCheckoutBlock) so the panes can
    /// offer "Stash & Switch" instead of showing git's stderr verbatim.
    pub fn git_checkout(branch: &str, cx: &mut App) -> BridgeResult<()> {
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
        Self::git_async_inner(
            label,
            cx,
            move |backend, root| backend.git_checkout(&root, &target),
            None,
            false,
            Some(checkout),
        )
    }

    /// Create a branch at HEAD and switch to it.
    ///
    /// Closes the inline name field as soon as the work is dispatched; if git
    /// rejects the name the error lands in the panel's error strip, which is
    /// more visible than an error inside a one-line input anyway.
    pub fn git_create_branch(name: &str, cx: &mut App) -> BridgeResult<()> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Ok(());
        }
        Self::state(cx).update(cx, |s| {
            s.git_new_branch = false;
            s.git_diff = None;
        });
        let label = format!("Creating {name}");
        Self::git_async(label, cx, move |backend, root| {
            backend.git_create_branch(&root, &name)
        })
    }

    /// Open the inline "new branch" field.
    pub fn git_begin_new_branch(cx: &mut App) {
        Self::state(cx).update(cx, |s| s.git_new_branch = true);
    }

    pub fn git_cancel_new_branch(cx: &mut App) {
        Self::state(cx).update(cx, |s| s.git_new_branch = false);
    }

    pub fn git_push(cx: &mut App) -> BridgeResult<()> {
        Self::git_async("Pushing", cx, |backend, root| backend.git_push(&root))
    }

    pub fn git_push_to(remote: &str, cx: &mut App) -> BridgeResult<()> {
        let label = format!("Pushing to {remote}");
        let remote = remote.to_string();
        Self::git_async(label, cx, move |backend, root| {
            backend.git_push_to(&root, &remote)
        })
    }

    pub fn git_push_to_branch(remote: &str, branch: &str, cx: &mut App) -> BridgeResult<()> {
        let label = format!("Pushing {branch} to {remote}");
        let remote = remote.to_string();
        let branch = branch.to_string();
        Self::git_async(label, cx, move |backend, root| {
            backend.git_push_to_branch(&root, &remote, &branch)
        })
    }

    pub fn git_pull(cx: &mut App) -> BridgeResult<()> {
        Self::git_async("Pulling", cx, |backend, root| backend.git_pull(&root))
    }

    pub fn git_fetch(cx: &mut App) -> BridgeResult<()> {
        Self::git_async("Fetching", cx, |backend, root| backend.git_fetch(&root))
    }

    pub fn git_sync(cx: &mut App) -> BridgeResult<()> {
        Self::git_async("Syncing", cx, |backend, root| backend.git_sync(&root))
    }

    /// Run a slow git operation off the UI thread.
    ///
    /// Push/Pull/Fetch/Sync block on the remote for seconds, and checkout has to
    /// rewrite the worktree; both used to freeze the window. Here they run on a
    /// worker, `git_busy` carries the label for the panel's progress row, and
    /// the refreshed status is fetched on the same worker so a large repo's
    /// `git status` never lands on the UI thread.
    ///
    /// A second call while one is in flight is a no-op — that is the
    /// double-click guard, not just a UI nicety.
    fn git_async<F>(label: impl Into<String>, cx: &mut App, work: F) -> BridgeResult<()>
    where
        F: FnOnce(Arc<dyn Backend>, String) -> BridgeResult<String> + Send + 'static,
    {
        Self::git_async_inner(label, cx, work, None, false, None)
    }

    /// Root of the repository the Graph & Branches pane is showing.
    ///
    /// Repo tabs let the graph point at a different work tree than the open
    /// project, and every branch menu action must land on the tree its row came
    /// from — not on the Coder root.
    pub(super) fn git_graph_root(cx: &App) -> Option<String> {
        let state = Self::state(cx).read(cx);
        state
            .git_graph
            .active_repo()
            .map(str::to_string)
            .or_else(|| state.coder.root_path.clone())
    }

    /// [`git_async`] with an explicit repository and an optional graph refresh.
    ///
    /// Branch operations mutate refs the graph pane lists, so they refresh
    /// `git_graph` after the worker reports success. Failures leave the graph
    /// alone and surface through `git.error` instead.
    ///
    /// `checkout` names the branch a switch was heading to. When git refuses
    /// because local edits would be overwritten, its file list is parsed into a
    /// [`GitCheckoutBlock`](crate::work::GitCheckoutBlock) for the recovery UI.
    pub(super) fn git_async_inner<F>(
        label: impl Into<String>,
        cx: &mut App,
        work: F,
        root: Option<String>,
        refresh_graph: bool,
        checkout: Option<String>,
    ) -> BridgeResult<()>
    where
        F: FnOnce(Arc<dyn Backend>, String) -> BridgeResult<String> + Send + 'static,
    {
        let backend = Self::backend(cx)?;
        let root = match root {
            Some(root) => root,
            None => Self::coder_require_root(cx)?,
        };
        let op_root = root.clone();
        let state = Self::state(cx);

        if state.read(cx).git_busy.is_some() {
            return Ok(());
        }
        let label = label.into();
        state.update(cx, |s| {
            s.git_busy = Some(label.clone());
            s.git.error = None;
            s.git.checkout_block = None;
            s.commit_active_tab();
        });

        Self::run_blocking(
            cx,
            move || {
                let msg = work(backend.clone(), root.clone())?;
                // Best-effort: the op itself succeeded, so a status hiccup
                // should not be reported as a failure.
                let status = backend.git_status(&root).ok();
                Ok((msg, status))
            },
            move |result, cx| {
                let (refreshed, succeeded) = Self::state(cx).update(cx, |s| {
                    s.git_busy = None;
                    let mut did_refresh = false;
                    let mut succeeded = false;
                    match result {
                        Ok((msg, status)) => {
                            succeeded = true;
                            s.apply_ok_msg(msg);
                            if let Some(status) = status {
                                s.git = status;
                                prune_selection(s);
                                did_refresh = true;
                            }
                        }
                        Err(err) => {
                            let detail = err.to_string();
                            s.git.checkout_block = checkout.as_deref().and_then(|branch| {
                                checkout_conflict_files(&detail).map(|files| {
                                    crate::work::GitCheckoutBlock {
                                        branch: branch.to_string(),
                                        root: op_root.clone(),
                                        files,
                                        from_graph: refresh_graph,
                                    }
                                })
                            });
                            // The dock keeps the full stderr. The toast is only
                            // a pointer: git's file list is already shown in
                            // the dock header (and in Changes if open).
                            let message = match &s.git.checkout_block {
                                Some(block) => format!(
                                    "Can't switch to '{}': {} uncommitted {} would be overwritten — see Git Output",
                                    block.branch,
                                    block.files.len(),
                                    if block.files.len() == 1 {
                                        "file"
                                    } else {
                                        "files"
                                    }
                                ),
                                None => format!("{label} failed — see Git Output"),
                            };
                            s.apply_git_err(&label, detail.clone(), message);
                            s.git.error = Some(detail);
                        }
                    }
                    s.commit_active_tab();
                    (did_refresh, succeeded)
                });
                if refreshed {
                    ensure_git_diff_open(cx);
                }
                if succeeded && refresh_graph {
                    let _ = Self::git_graph_refresh(cx);
                }
            },
        );
        Ok(())
    }

    /// Drop the inbox after a project switch and reload when that pane is open.
    pub(in crate::bridge) fn pulls_on_project_changed(cx: &mut App) {
        let pane = Self::state(cx).read(cx).source_control_pane;
        Self::state(cx).update(cx, |s| {
            s.pulls.items.clear();
            s.pulls.selected = None;
            s.pulls.error = None;
            s.pulls.loading = false;
            s.pulls_epoch = s.pulls_epoch.wrapping_add(1);
        });
        if pane == crate::work::SourceControlPane::PullRequests {
            let _ = Self::git_pulls_refresh(cx);
        }
    }

    pub fn select_source_control_pane(pane: crate::work::SourceControlPane, cx: &mut App) {
        let changed = Self::state(cx).update(cx, |s| {
            if s.source_control_pane == pane {
                return false;
            }
            s.source_control_pane = pane;
            true
        });
        if changed {
            match pane {
                crate::work::SourceControlPane::PullRequests => {
                    let _ = Self::git_pulls_refresh(cx);
                }
                crate::work::SourceControlPane::Graph => {
                    let _ = Self::git_graph_refresh(cx);
                }
                crate::work::SourceControlPane::Changes => {
                    let _ = Self::git_refresh(cx);
                }
            }
        }
    }

    pub fn git_pulls_set_filter(filter: crate::work::PullRequestFilter, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.pulls.filter = filter;
        });
    }

    pub fn git_pulls_set_query(query: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if s.pulls.query != query {
                s.pulls.query = query.to_string();
            }
        });
    }

    pub fn git_pulls_select(number: u64, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.pulls.selected = Some(number);
        });
    }

    /// Move the selection within the filtered list. `delta` is -1 or 1.
    pub fn git_pulls_move(delta: i32, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let visible: Vec<u64> = s.pulls.visible().iter().map(|pr| pr.number).collect();
            if visible.is_empty() {
                return;
            }
            let current = s
                .pulls
                .selected
                .and_then(|n| visible.iter().position(|x| *x == n));
            let next = match current {
                Some(i) => (i as i32 + delta).clamp(0, visible.len() as i32 - 1) as usize,
                None => {
                    if delta < 0 {
                        visible.len() - 1
                    } else {
                        0
                    }
                }
            };
            s.pulls.selected = Some(visible[next]);
        });
    }

    /// Load pull requests with `gh` off the UI thread. Files on disk are untouched.
    pub fn git_pulls_refresh(cx: &mut App) -> BridgeResult<()> {
        let Some(root) = Self::state(cx).read(cx).coder.root_path.clone() else {
            Self::state(cx).update(cx, |s| {
                s.pulls.items.clear();
                s.pulls.selected = None;
                s.pulls.error = None;
                s.pulls.loading = false;
            });
            return Ok(());
        };
        if Self::state(cx).read(cx).pulls.loading {
            return Ok(());
        }
        let epoch = Self::state(cx).update(cx, |s| {
            s.pulls.loading = true;
            s.pulls.error = None;
            s.pulls_epoch = s.pulls_epoch.wrapping_add(1);
            s.pulls_epoch
        });

        Self::run_blocking(
            cx,
            move || load_pull_requests(&root).map(|items| (root, items)),
            move |result, cx| {
                Self::state(cx).update(cx, |s| {
                    if s.pulls_epoch != epoch {
                        return;
                    }
                    s.pulls.loading = false;
                    match result {
                        Ok((root, items)) => {
                            if s.coder.root_path.as_deref() != Some(root.as_str()) {
                                return;
                            }
                            let still_there = s
                                .pulls
                                .selected
                                .is_some_and(|n| items.iter().any(|pr| pr.number == n));
                            if !still_there {
                                s.pulls.selected = None;
                            }
                            s.pulls.items = items;
                            s.pulls.error = None;
                        }
                        Err(err) => {
                            s.pulls.error = Some(err.to_string());
                        }
                    }
                });
            },
        );
        Ok(())
    }

    fn git_fail(op: &str, err: crate::BridgeError, cx: &mut App) -> BridgeResult<()> {
        let detail = err.to_string();
        let message = format!("{op} failed — see Git Output");
        Self::state(cx).update(cx, |s| {
            s.git.error = Some(detail.clone());
            s.git.checkout_block = None;
            s.apply_git_err(op, detail, message);
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Run a synchronous git operation and, on failure, copy its stderr into
    /// the Git Output dock before handing the error back.
    ///
    /// Stage/unstage/discard return the error to the caller as well (the pane
    /// keeps its own inline handling), but the dock is the only place the raw
    /// command output is kept for later.
    fn git_logged<F>(op: &str, cx: &mut App, work: F) -> BridgeResult<()>
    where
        F: FnOnce() -> BridgeResult<()>,
    {
        match work() {
            Ok(()) => Ok(()),
            Err(err) => {
                let detail = err.to_string();
                Self::state(cx).update(cx, |s| s.push_git_output(op, detail));
                Err(err)
            }
        }
    }

    /// Best-effort graph read: a failure is logged to the Git Output dock and
    /// substituted with the empty value so the rest of the graph still paints.
    pub(super) fn git_graph_fetch<T: Default>(op: &str, cx: &mut App, result: BridgeResult<T>) -> T {
        match result {
            Ok(value) => value,
            Err(err) => {
                let detail = err.to_string();
                Self::state(cx).update(cx, |s| s.push_git_output(op, detail));
                T::default()
            }
        }
    }
}

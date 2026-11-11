//! Staging, committing, branching, and remote sync mutations.

use super::*;

use super::process::*;

pub fn stage(workspace: &str, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let repo = require_repo(workspace)?;
    let mut args: Vec<&str> = vec!["add", "--"];
    let owned: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    args.extend(owned);
    let (_stdout, stderr, code) = run_git(&repo, &args)?;
    if code != 0 {
        return Err(stderr.trim().to_string());
    }
    Ok(())
}

pub fn unstage(workspace: &str, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let repo = require_repo(workspace)?;
    let mut args: Vec<&str> = vec!["restore", "--staged", "--"];
    let owned: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    args.extend(owned);
    let (_stdout, stderr, code) = run_git(&repo, &args)?;
    if code != 0 {
        return Err(stderr.trim().to_string());
    }
    Ok(())
}

pub fn discard(workspace: &str, path: &str) -> Result<(), String> {
    let repo = require_repo(workspace)?;
    // Untracked: remove from disk. Tracked: restore worktree.
    let status = status(workspace)?;
    if status.untracked.iter().any(|f| f.path == path) {
        let full = repo.join(path);
        if full.is_dir() {
            std::fs::remove_dir_all(&full).map_err(|e| e.to_string())?;
        } else if full.exists() {
            std::fs::remove_file(&full).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let (_stdout, stderr, code) = run_git(&repo, &["restore", "--", path])?;
    if code != 0 {
        return Err(stderr.trim().to_string());
    }
    Ok(())
}

/// Unified diff for one path. `staged` compares the index against `HEAD`;
/// otherwise the worktree against the index. Untracked files are diffed
/// against `/dev/null` so a new file still shows its full contents.
///
/// Returns an empty string when the file has no textual changes (binary, mode
/// change only, or nothing selected).
pub fn diff(workspace: &str, path: &str, staged: bool) -> Result<String, String> {
    let repo = require_repo(workspace)?;
    let untracked = !staged && status(workspace)?.untracked.iter().any(|f| f.path == path);

    let (stdout, stderr, code) = if staged {
        run_git(&repo, &["diff", "--no-color", "--cached", "--", path])?
    } else if untracked {
        // `--no-index` exits 1 when the files differ — the normal case here.
        run_git(
            &repo,
            &["diff", "--no-color", "--no-index", "--", "/dev/null", path],
        )?
    } else {
        run_git(&repo, &["diff", "--no-color", "--", path])?
    };

    let ok = code == 0 || (untracked && code == 1);
    if !ok {
        return Err(git_err("diff", code, &stdout, &stderr));
    }
    Ok(stdout)
}

/// Apply one hunk to the index, leaving the worktree alone.
///
/// `reverse` is the unstage direction: the patch is the index-vs-HEAD diff of a
/// staged hunk, so applying it backwards drops exactly that hunk from the index.
pub fn apply_patch(workspace: &str, patch: &str, reverse: bool) -> Result<String, String> {
    if !patch.contains("@@") || !patch.contains("--- ") || !patch.contains("+++ ") {
        return Err("Patch is missing its file or hunk header".into());
    }
    let repo = require_repo(workspace)?;
    let mut args = vec!["apply", "--cached", "--whitespace=nowarn"];
    if reverse {
        args.push("--reverse");
    }
    args.push("-");
    let (stdout, stderr, code) = run_git_stdin(&repo, &args, patch)?;
    if code != 0 {
        return Err(git_err("apply", code, &stdout, &stderr));
    }
    Ok(if reverse {
        "Unstaged hunk".into()
    } else {
        "Staged hunk".into()
    })
}

pub fn commit(workspace: &str, message: &str) -> Result<String, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Commit message is empty".into());
    }
    let repo = require_repo(workspace)?;
    let (_stdout, stderr, code) = run_git(&repo, &["commit", "-m", message])?;
    if code != 0 {
        return Err(if stderr.trim().is_empty() {
            format!("git commit failed (exit {code})")
        } else {
            stderr.trim().to_string()
        });
    }
    Ok("Committed".into())
}

/// Amend the previous commit. Empty `message` keeps the prior message (`--no-edit`).
pub fn amend(workspace: &str, message: &str) -> Result<String, String> {
    let repo = require_repo(workspace)?;
    let message = message.trim();
    let (stdout, stderr, code) = if message.is_empty() {
        run_git(&repo, &["commit", "--amend", "--no-edit"])?
    } else {
        run_git(&repo, &["commit", "--amend", "-m", message])?
    };
    if code != 0 {
        return Err(git_err("amend", code, &stdout, &stderr));
    }
    Ok("Amended".into())
}

/// Switch to an existing branch.
///
/// Uses `git checkout` rather than `git switch` so a name that only exists on a
/// remote still works: git's DWIM creates the local tracking branch, which is
/// what picking `origin/foo` from a list implies.
pub fn checkout(workspace: &str, branch: &str) -> Result<String, String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("Branch name is empty".into());
    }
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["checkout", branch])?;
    if code != 0 {
        return Err(git_err("checkout", code, &stdout, &stderr));
    }
    Ok(format!("Switched to {branch}"))
}

/// Tip of the stash ref, if any stash exists.
///
/// [`stash_and_checkout`] compares this before and after `git stash push` so a
/// failed retry only pops the stash it created — never one the user parked
/// earlier.
pub(crate) fn stash_tip(repo: &Path) -> Option<String> {
    run_git(repo, &["rev-parse", "--verify", "--quiet", "refs/stash"])
        .ok()
        .filter(|(_, _, code)| *code == 0)
        .map(|(stdout, _, _)| stdout.trim().to_string())
        .filter(|tip| !tip.is_empty())
}

/// Park every local change in a stash, then switch to `branch`.
///
/// This is the recovery path for [`checkout`]'s "local changes would be
/// overwritten" refusal: nothing is discarded, the edits land in an ordinary
/// stash entry the user can pop or drop. If the switch still fails, the stash
/// is popped again so a failed recovery does not leave the work hidden.
pub fn stash_and_checkout(workspace: &str, branch: &str) -> Result<String, String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("Branch name is empty".into());
    }
    let repo = require_repo(workspace)?;

    let before = stash_tip(&repo);
    let message = format!("hexbuffer: changes before switching to {branch}");
    let (stdout, stderr, code) = run_git(
        &repo,
        &["stash", "push", "--include-untracked", "-m", &message],
    )?;
    if code != 0 {
        return Err(git_err("stash", code, &stdout, &stderr));
    }
    // "No local changes to save" exits 0 without creating an entry; popping in
    // that case would resurrect an unrelated stash.
    let created = stash_tip(&repo) != before;

    let (stdout, stderr, code) = run_git(&repo, &["checkout", branch])?;
    if code != 0 {
        let err = git_err("checkout", code, &stdout, &stderr);
        if !created {
            return Err(err);
        }
        let (_out, pop_err, pop_code) = run_git(&repo, &["stash", "pop"])?;
        return Err(if pop_code == 0 {
            err
        } else {
            format!(
                "{err}\nThe stashed changes could not be restored automatically: {}",
                pop_err.trim()
            )
        });
    }
    Ok(if created {
        format!("Stashed local changes and switched to {branch}")
    } else {
        format!("Switched to {branch}")
    })
}

/// Create a branch at HEAD and switch to it.
pub fn create_branch(workspace: &str, name: &str) -> Result<String, String> {
    let name = name.trim();
    validate_branch_name(name)?;
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["checkout", "-b", name])?;
    if code != 0 {
        return Err(git_err("create branch", code, &stdout, &stderr));
    }
    Ok(format!("Created and switched to {name}"))
}

/// Create a branch from `base` and switch to it.
///
/// The base is a local branch name (or any ref git accepts); the new branch is
/// checked out so the row the user right-clicked becomes the branch they work on.
pub fn create_branch_from(workspace: &str, name: &str, base: &str) -> Result<String, String> {
    let name = name.trim();
    let base = base.trim();
    validate_branch_name(name)?;
    validate_branch_name(base)?;
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["checkout", "-b", name, base])?;
    if code != 0 {
        return Err(git_err("create branch", code, &stdout, &stderr));
    }
    Ok(format!("Created and switched to {name} from {base}"))
}

/// Rename a branch (the current branch included).
pub fn rename_branch(workspace: &str, from: &str, to: &str) -> Result<String, String> {
    let from = from.trim();
    let to = to.trim();
    if from.is_empty() {
        return Err("Branch name is empty".into());
    }
    validate_branch_name(to)?;
    if from == to {
        return Ok(format!("Branch is already named {to}"));
    }
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["branch", "-m", from, to])?;
    if code != 0 {
        return Err(git_err("rename branch", code, &stdout, &stderr));
    }
    Ok(format!("Renamed {from} to {to}"))
}

/// Reject names git would reject, but say why — `git checkout -b` only answers
/// "not a valid branch name", which does not tell the user what to change.
pub(crate) fn validate_branch_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Branch name is empty".into());
    }
    if name.starts_with('-') {
        return Err("Branch names cannot start with `-`".into());
    }
    if name.chars().any(char::is_whitespace) {
        return Err("Branch names cannot contain spaces".into());
    }
    if let Some(bad) = name.chars().find(|c| "~^:?*[\\".contains(*c)) {
        return Err(format!("Branch names cannot contain `{bad}`"));
    }
    if name.contains("..") {
        return Err("Branch names cannot contain `..`".into());
    }
    if name.ends_with('/') || name.ends_with(".lock") || name == "@" {
        return Err("Invalid branch name".into());
    }
    Ok(())
}

/// Push the current branch to its upstream (or set upstream to `origin` if none).
pub fn push(workspace: &str) -> Result<String, String> {
    let repo = require_repo(workspace)?;
    let status = status(workspace)?;
    let (stdout, stderr, code) = if status.upstream.is_some() {
        run_git(&repo, &["push"])?
    } else {
        // First push: publish current HEAD to origin and set upstream.
        run_git(&repo, &["push", "-u", "origin", "HEAD"])?
    };
    if code != 0 {
        return Err(git_err("push", code, &stdout, &stderr));
    }
    Ok(summarize_remote("Pushed", &stdout, &stderr))
}

/// Push the current branch to `remote` (sets upstream with `-u`).
pub fn push_to(workspace: &str, remote: &str) -> Result<String, String> {
    let remote = remote.trim();
    if remote.is_empty() {
        return Err("Remote is empty".into());
    }
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["push", "-u", remote, "HEAD"])?;
    if code != 0 {
        return Err(git_err("push", code, &stdout, &stderr));
    }
    Ok(summarize_remote(
        &format!("Pushed to {remote}"),
        &stdout,
        &stderr,
    ))
}

/// Push the current local branch to `remote`/`branch` (refspec `HEAD:branch`).
pub fn push_to_branch(workspace: &str, remote: &str, branch: &str) -> Result<String, String> {
    let remote = remote.trim();
    let branch = branch.trim();
    if remote.is_empty() || branch.is_empty() {
        return Err("Remote and branch are required".into());
    }
    let repo = require_repo(workspace)?;
    let refspec = format!("HEAD:{branch}");
    let (stdout, stderr, code) = run_git(&repo, &["push", remote, &refspec])?;
    if code != 0 {
        return Err(git_err("push", code, &stdout, &stderr));
    }
    Ok(summarize_remote(
        &format!("Pushed to {remote}/{branch}"),
        &stdout,
        &stderr,
    ))
}

pub fn pull(workspace: &str) -> Result<String, String> {
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["pull", "--ff-only"])?;
    if code != 0 {
        return Err(git_err("pull", code, &stdout, &stderr));
    }
    Ok(summarize_remote("Pulled", &stdout, &stderr))
}

pub fn fetch(workspace: &str) -> Result<String, String> {
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["fetch", "--all", "--prune"])?;
    if code != 0 {
        return Err(git_err("fetch", code, &stdout, &stderr));
    }
    Ok(summarize_remote("Fetched", &stdout, &stderr))
}

/// Pull then push — convenience “Sync” for the source-control menu.
pub fn sync(workspace: &str) -> Result<String, String> {
    let pull_msg = pull(workspace)?;
    let push_msg = push(workspace)?;
    Ok(format!("{pull_msg}; {push_msg}"))
}

pub fn stage_all(workspace: &str) -> Result<(), String> {
    let repo = require_repo(workspace)?;
    let (_stdout, stderr, code) = run_git(&repo, &["add", "-A"])?;
    if code != 0 {
        return Err(stderr.trim().to_string());
    }
    Ok(())
}

pub fn unstage_all(workspace: &str) -> Result<(), String> {
    let repo = require_repo(workspace)?;
    let (_stdout, stderr, code) = run_git(&repo, &["restore", "--staged", "."])?;
    if code != 0 {
        return Err(stderr.trim().to_string());
    }
    Ok(())
}

pub(crate) fn list_remote_names(repo: &Path) -> Result<Vec<String>, String> {
    let (stdout, stderr, code) = run_git(repo, &["remote"])?;
    if code != 0 {
        return Err(stderr.trim().to_string());
    }
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

pub(crate) fn list_local_branches(repo: &Path) -> Result<Vec<String>, String> {
    let (stdout, stderr, code) = run_git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    if code != 0 {
        return Err(stderr.trim().to_string());
    }
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

pub(crate) fn git_err(op: &str, code: i32, stdout: &str, stderr: &str) -> String {
    let detail = [stderr.trim(), stdout.trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or("");
    if detail.is_empty() {
        format!("git {op} failed (exit {code})")
    } else {
        detail.to_string()
    }
}

pub(crate) fn summarize_remote(fallback: &str, stdout: &str, stderr: &str) -> String {
    let detail = [stdout.trim(), stderr.trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or("");
    if detail.is_empty() {
        fallback.to_string()
    } else {
        // Keep the toast/status line short.
        let first = detail.lines().next().unwrap_or(detail);
        if first.len() > 120 {
            format!("{fallback}: {}…", &first[..117])
        } else {
            format!("{fallback}: {first}")
        }
    }
}

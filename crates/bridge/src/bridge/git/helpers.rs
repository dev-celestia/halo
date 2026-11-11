//! Free helpers shared by the Source Control and Git Graph surfaces.

use super::*;


/// Colliding paths from git's "would be overwritten by checkout" refusal.
///
/// git prints the sentence, then one tab-indented path per line, then advice
/// ("Please commit your changes…", "Aborting"). Only the indented block holds
/// paths; treating the surrounding sentences as files would put nonsense in the
/// error card.
pub(super) fn checkout_conflict_files(detail: &str) -> Option<Vec<String>> {
    let mut lines = detail.lines();
    lines.find(|line| line.contains("would be overwritten by checkout"))?;
    let mut files = Vec::new();
    for line in lines {
        if !line.starts_with('\t') && !line.starts_with("    ") {
            break;
        }
        let path = line.trim();
        if path.is_empty() {
            break;
        }
        files.push(path.to_string());
    }
    (!files.is_empty()).then_some(files)
}

/// Drop ticks whose path is no longer in any list — a staged path moves from
/// `unstaged` to `staged`, a commit empties `staged`, a discard removes the row.
/// Without this the footer would keep offering "Stage Selected" for rows that
/// are already gone.
pub(super) fn load_pull_requests(root: &str) -> BridgeResult<Vec<crate::work::PullRequest>> {
    let output = std::process::Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "100",
            "--json",
            "number,title,author,state,body,updatedAt",
        ])
        .current_dir(root)
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                BridgeError::message("gh is not installed")
            } else {
                BridgeError::message(format!("run gh: {err}"))
            }
        })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let err = err.trim();
        let msg = if err.is_empty() {
            format!("gh exited {}", output.status)
        } else {
            err.to_string()
        };
        return Err(BridgeError::message(msg));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|err| BridgeError::message(format!("gh returned invalid JSON: {err}")))?;
    let rows = value
        .as_array()
        .ok_or_else(|| BridgeError::message("gh returned unexpected JSON"))?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(number) = row.get("number").and_then(|v| v.as_u64()) else {
            continue;
        };
        let title = row
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let author = match row.get("author") {
            Some(author) if author.is_string() => author.as_str().unwrap_or("").to_string(),
            Some(author) => author
                .get("login")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            None => String::new(),
        };
        let state = row
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let body = row
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let updated_at = row
            .get("updatedAt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        items.push(crate::work::PullRequest {
            number,
            title,
            author,
            state,
            body,
            updated_at,
        });
    }
    Ok(items)
}

pub(super) fn prune_selection(state: &mut crate::BridgeState) {
    if state.git_selection.is_empty() {
        return;
    }
    let git = &state.git;
    let live: std::collections::HashSet<&str> = git
        .staged
        .iter()
        .chain(git.unstaged.iter())
        .chain(git.untracked.iter())
        .map(|f| f.path.as_str())
        .collect();
    state
        .git_selection
        .retain(|path| live.contains(path.as_str()));
}

/// Keep a useful diff open while Source Control is the active work tab.
///
/// If the current path is still in the snapshot, reload it (preserving staged
/// vs worktree when possible). Otherwise open the first staged, then unstaged,
/// then untracked file. Outside Source Control this is a no-op.
pub(super) fn ensure_git_diff_open(cx: &mut App) {
    let target = Bridge::state(cx).update(cx, |s| {
        let in_git = s.active_work_kind() == Some(crate::work::WorkKind::SourceControl);
        if !in_git || s.git.not_a_repo {
            return None;
        }
        let git = &s.git;
        if let Some(diff) = &s.git_diff {
            let in_staged = git.staged.iter().any(|f| f.path == diff.path);
            let in_worktree = git.unstaged.iter().any(|f| f.path == diff.path)
                || git.untracked.iter().any(|f| f.path == diff.path);
            // Prefer the side the user was already reviewing when a path is
            // both staged and dirty (partial stage).
            if diff.staged && in_staged {
                return Some((diff.path.clone(), true));
            }
            if !diff.staged && in_worktree {
                return Some((diff.path.clone(), false));
            }
            if in_staged {
                return Some((diff.path.clone(), true));
            }
            if in_worktree {
                return Some((diff.path.clone(), false));
            }
            s.git_diff = None;
        }
        if let Some(f) = git.staged.first() {
            return Some((f.path.clone(), true));
        }
        if let Some(f) = git.unstaged.first() {
            return Some((f.path.clone(), false));
        }
        git.untracked.first().map(|f| (f.path.clone(), false))
    });
    if let Some((path, staged)) = target {
        let _ = Bridge::git_diff(&path, staged, cx);
    }
}

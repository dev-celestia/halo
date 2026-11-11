//! Read commands: log text and working-tree status parsing.

use super::*;

use super::process::*;

/// History for the terminal's read-only Git Log tab.
///
/// `--graph --decorate --oneline` is what a user would type at the prompt.
/// Colour is forced off because the pane paints plain rows rather than feeding
/// a VT parser; the cap keeps a huge repository from stalling the UI thread.
pub fn log(workspace: &str) -> Result<String, String> {
    let cwd = Path::new(workspace);
    if !cwd.is_dir() {
        return Err(format!("Not a directory: {workspace}"));
    }
    let Some(repo_root) = find_repo_root(cwd)? else {
        return Ok("Not a git repository — nothing to show.".to_string());
    };
    let (stdout, stderr, code) = run_git(
        &repo_root,
        &[
            "log",
            "--graph",
            "--decorate=short",
            "--oneline",
            "--no-color",
            "-n",
            "500",
        ],
    )?;
    if code != 0 {
        return Err(if stderr.trim().is_empty() {
            format!("git log failed (exit {code})")
        } else {
            stderr.trim().to_string()
        });
    }
    if stdout.trim().is_empty() {
        Ok("No commits yet.".to_string())
    } else {
        Ok(stdout.trim_end().to_string())
    }
}

/// Snapshot working-tree status for the Coder workspace folder.
pub fn status(workspace: &str) -> Result<GitStatus, String> {
    let cwd = Path::new(workspace);
    if !cwd.is_dir() {
        return Err(format!("Not a directory: {workspace}"));
    }

    let Some(repo_root) = find_repo_root(cwd)? else {
        return Ok(GitStatus {
            not_a_repo: true,
            ..GitStatus::default()
        });
    };

    let (stdout, stderr, code) = run_git(
        &repo_root,
        &[
            "status",
            "--porcelain=v1",
            "-b",
            "--untracked-files=all",
            "--ignored=matching",
        ],
    )?;
    if code != 0 {
        return Err(if stderr.trim().is_empty() {
            format!("git status failed (exit {code})")
        } else {
            stderr.trim().to_string()
        });
    }

    let mut result = GitStatus {
        repo_root: Some(repo_root.display().to_string()),
        not_a_repo: false,
        remotes: list_remote_names(&repo_root).unwrap_or_default(),
        local_branches: list_local_branches(&repo_root).unwrap_or_default(),
        ..GitStatus::default()
    };

    // Line counts for the `+N −M` row badges. Each side is measured against
    // its own base, so a path that is both staged and dirty shows the count
    // for the row it appears on rather than a combined total.
    let staged_counts = numstat(&repo_root, true);
    let unstaged_counts = numstat(&repo_root, false);

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            parse_branch_header(rest, &mut result);
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let index = line.as_bytes()[0] as char;
        let work = line.as_bytes()[1] as char;
        let path_part = &line[3..];
        // Rename: `R  old -> new` — keep the new path and remember the old one.
        let (previous_path, path) = match path_part.rsplit_once(" -> ") {
            Some((old, neu)) => (
                Some(old.trim_end_matches('/').to_string()),
                neu.trim_end_matches('/').to_string(),
            ),
            None => (None, path_part.trim_end_matches('/').to_string()),
        };

        if index == '!' && work == '!' {
            result.ignored.push(GitFileEntry {
                path,
                status: "!".into(),
                previous_path: None,
                ..GitFileEntry::default()
            });
            continue;
        }

        if index == '?' && work == '?' {
            let (additions, deletions) = untracked_counts(&repo_root, &path);
            result.untracked.push(GitFileEntry {
                path,
                status: "?".into(),
                previous_path: None,
                additions,
                deletions,
            });
            continue;
        }

        if index != ' ' && index != '?' {
            let (additions, deletions) = staged_counts.get(&path).copied().unwrap_or_default();
            result.staged.push(GitFileEntry {
                path: path.clone(),
                status: index.to_string(),
                previous_path: previous_path.clone(),
                additions,
                deletions,
            });
        }
        if work != ' ' && work != '?' {
            let (additions, deletions) = unstaged_counts.get(&path).copied().unwrap_or_default();
            result.unstaged.push(GitFileEntry {
                path,
                status: work.to_string(),
                previous_path,
                additions,
                deletions,
            });
        }
    }

    Ok(result)
}

pub(crate) fn parse_branch_header(rest: &str, out: &mut GitStatus) {
    // Formats:
    //   main
    //   main...origin/main
    //   main...origin/main [ahead 1]
    //   main...origin/main [behind 2]
    //   main...origin/main [ahead 1, behind 2]
    //   HEAD (no branch)
    //   No commits yet on main        (unborn branch, `git init` and no commit)
    //   Initial commit on main        (older git wording)

    // An unborn branch has a name but no ref yet. Without this the naive
    // space-split below reports a branch called "No".
    if let Some(name) = rest
        .strip_prefix("No commits yet on ")
        .or_else(|| rest.strip_prefix("Initial commit on "))
    {
        out.branch = Some(name.trim().to_string());
        return;
    }

    // Detached HEAD: there is no branch, so nothing may push or amend.
    if rest.starts_with("HEAD (no branch)") {
        out.branch = None;
        return;
    }

    let (head, tracking) = match rest.split_once(' ') {
        Some((h, t)) => (h, Some(t)),
        None => (rest, None),
    };

    if let Some((local, remote)) = head.split_once("...") {
        out.branch = Some(local.to_string());
        out.upstream = Some(remote.to_string());
    } else {
        out.branch = Some(head.to_string());
    }

    if let Some(tracking) = tracking {
        if let Some(start) = tracking.find('[') {
            if let Some(end) = tracking.find(']') {
                let inner = &tracking[start + 1..end];
                for part in inner.split(',') {
                    let part = part.trim();
                    if let Some(n) = part.strip_prefix("ahead ") {
                        out.ahead = n.trim().parse().unwrap_or(0);
                    } else if let Some(n) = part.strip_prefix("behind ") {
                        out.behind = n.trim().parse().unwrap_or(0);
                    }
                }
            }
        }
    }
}

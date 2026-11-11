//! DTO queries: commit graph, branches, remotes, tags, stashes, worktrees.

use super::*;

use super::mutations::*;

use super::process::*;

/// Retrieve the commit log formatted for topological DAG visualization.
pub fn graph_log(workspace: &str, max_count: usize) -> Result<Vec<GitCommitDto>, String> {
    let repo = require_repo(workspace)?;
    let count_arg = format!("-n{}", max_count.max(1));
    let args = [
        "log",
        "--all",
        "--topo-order",
        "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%D%x00%s",
        &count_arg,
    ];
    let (stdout, stderr, code) = run_git(&repo, &args)?;
    if code != 0 {
        return Err(git_err("graph_log", code, &stdout, &stderr));
    }

    let mut commits = Vec::new();
    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\0').collect();
        if parts.len() < 7 {
            continue;
        }
        let sha = parts[0].trim().to_string();
        let parents = parts[1].split_whitespace().map(|s| s.to_string()).collect();
        let author_name = parts[2].trim().to_string();
        let author_email = parts[3].trim().to_string();
        let timestamp = parts[4].trim().parse::<i64>().unwrap_or(0);
        let refs = if parts[5].trim().is_empty() {
            Vec::new()
        } else {
            parts[5]
                .split(',')
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect()
        };
        let subject = parts[6].to_string();

        commits.push(GitCommitDto {
            sha,
            parents,
            author_name,
            author_email,
            timestamp,
            refs,
            subject,
        });
    }

    Ok(commits)
}

/// List all local branches with upstream tracking info and ahead/behind counts.
pub fn list_branches(workspace: &str) -> Result<Vec<GitBranchInfoDto>, String> {
    let repo = require_repo(workspace)?;
    let args = [
        "for-each-ref",
        "--format=%(refname:short)\t%(HEAD)\t%(upstream:short)\t%(upstream:track)",
        "refs/heads",
    ];
    let (stdout, stderr, code) = run_git(&repo, &args)?;
    if code != 0 {
        return Err(git_err("list_branches", code, &stdout, &stderr));
    }

    let mut branches = Vec::new();

    // `git branch -d` refuses unmerged branches, so the branch menu uses this
    // to pick between the safe and the forced delete instead of parsing stderr.
    // When the query fails, report every branch as merged: the delete then goes
    // through `-d` and git keeps its own safety check.
    let merged: Option<HashSet<String>> = run_git(
        &repo,
        &["branch", "--merged", "HEAD", "--format=%(refname:short)"],
    )
    .ok()
    .filter(|(_, _, code)| *code == 0)
    .map(|(out, _, _)| {
        out.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    });

    for line in stdout.lines() {
        let parts: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else if line.contains('\0') {
            line.split('\0').collect()
        } else if line.contains("%x00") {
            line.split("%x00").collect()
        } else {
            line.split_whitespace().collect()
        };
        if parts.is_empty() || parts[0].trim().is_empty() {
            continue;
        }
        let name = parts[0].trim().to_string();
        let is_current = parts.get(1).map_or(false, |h| h.trim() == "*");
        let upstream = parts.get(2).and_then(|u| {
            let u = u.trim();
            if u.is_empty() {
                None
            } else {
                Some(u.to_string())
            }
        });
        let mut ahead = 0;
        let mut behind = 0;
        if let Some(track) = parts.get(3) {
            if let Some(pos) = track.find("ahead ") {
                let rest = &track[pos + 6..];
                let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                ahead = num_str.parse().unwrap_or(0);
            }
            if let Some(pos) = track.find("behind ") {
                let rest = &track[pos + 7..];
                let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                behind = num_str.parse().unwrap_or(0);
            }
        }
        branches.push(GitBranchInfoDto {
            merged: merged.as_ref().is_none_or(|set| set.contains(&name)),
            name,
            is_current,
            upstream,
            ahead,
            behind,
        });
    }

    Ok(branches)
}

/// List all remotes and their remote branches.
pub fn list_remotes(workspace: &str) -> Result<Vec<GitRemoteInfoDto>, String> {
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["remote"])?;
    if code != 0 {
        return Err(git_err("list_remotes", code, &stdout, &stderr));
    }

    let mut remotes = Vec::new();
    for r_name in stdout.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let ref_path = format!("refs/remotes/{r_name}");
        let (branch_out, _, _) = run_git(
            &repo,
            &["for-each-ref", "--format=%(refname:short)", &ref_path],
        )?;
        let branches: Vec<String> = branch_out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|b| !b.is_empty() && !b.ends_with("/HEAD"))
            .collect();
        remotes.push(GitRemoteInfoDto {
            name: r_name.to_string(),
            branches,
        });
    }
    Ok(remotes)
}

/// List all git tags sorted by creation date descending.
pub fn list_tags(workspace: &str) -> Result<Vec<String>, String> {
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["tag", "-l", "--sort=-creatordate"])?;
    if code != 0 {
        return Err(git_err("list_tags", code, &stdout, &stderr));
    }
    Ok(stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// List all stashes.
pub fn list_stashes(workspace: &str) -> Result<Vec<GitStashDto>, String> {
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["stash", "list", "--format=%gd\t%gs\t%at"])?;
    if code != 0 {
        return Err(git_err("list_stashes", code, &stdout, &stderr));
    }
    let mut stashes = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = if line.contains('\t') {
            line.split('\t').collect()
        } else if line.contains('\0') {
            line.split('\0').collect()
        } else if line.contains("%x00") {
            line.split("%x00").collect()
        } else {
            line.split_whitespace().collect()
        };
        if parts.len() < 3 {
            continue;
        }
        let gd = parts[0].trim();
        let idx = if let Some(start) = gd.find('{') {
            if let Some(end) = gd.find('}') {
                gd[start + 1..end].parse().unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };
        let message = parts[1].trim().to_string();
        let timestamp = parts[2].trim().parse().unwrap_or(0);
        stashes.push(GitStashDto {
            index: idx,
            message,
            timestamp,
        });
    }
    Ok(stashes)
}

/// List all worktrees.
pub fn list_worktrees(workspace: &str) -> Result<Vec<GitWorktreeDto>, String> {
    let repo = require_repo(workspace)?;
    let (stdout, stderr, code) = run_git(&repo, &["worktree", "list", "--porcelain"])?;
    if code != 0 {
        return Err(git_err("list_worktrees", code, &stdout, &stderr));
    }
    let mut worktrees = Vec::new();
    let mut curr_path = String::new();
    let mut curr_head = String::new();
    let mut curr_branch = None;

    for line in stdout.lines() {
        if line.is_empty() {
            if !curr_path.is_empty() {
                worktrees.push(GitWorktreeDto {
                    path: curr_path,
                    head: curr_head,
                    branch: curr_branch,
                });
                curr_path = String::new();
                curr_head = String::new();
                curr_branch = None;
            }
            continue;
        }
        if let Some(p) = line.strip_prefix("worktree ") {
            curr_path = p.trim().to_string();
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            curr_head = h.trim().to_string();
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            curr_branch = Some(b.trim().to_string());
        }
    }
    if !curr_path.is_empty() {
        worktrees.push(GitWorktreeDto {
            path: curr_path,
            head: curr_head,
            branch: curr_branch,
        });
    }
    Ok(worktrees)
}

/// List files changed in a specific commit with porcelain status and line counts.
pub fn commit_files(workspace: &str, sha: &str) -> Result<Vec<GitCommitFileDto>, String> {
    let repo = require_repo(workspace)?;
    let sha = sha.trim();
    if sha.is_empty() {
        return Ok(Vec::new());
    }

    let (numstat_out, _, _) = run_git(
        &repo,
        &["diff-tree", "--no-commit-id", "--numstat", "-r", "-m", sha],
    )?;
    let mut stats: HashMap<String, (u32, u32)> = HashMap::new();
    for line in numstat_out.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let add = parts[0].parse::<u32>().unwrap_or(0);
            let del = parts[1].parse::<u32>().unwrap_or(0);
            let path = parts[2].trim().to_string();
            stats.insert(path, (add, del));
        }
    }

    let (status_out, stderr, code) = run_git(
        &repo,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "-r",
            "-m",
            sha,
        ],
    )?;
    if code != 0 {
        return Err(git_err("commit_files", code, &status_out, &stderr));
    }

    let mut files = Vec::new();
    for line in status_out.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }
        let status = parts[0].chars().next().unwrap_or('M').to_string();
        let path = if parts.len() > 1 {
            parts[parts.len() - 1].trim().to_string()
        } else {
            String::new()
        };
        if path.is_empty() {
            continue;
        }
        let (additions, deletions) = stats.get(&path).copied().unwrap_or((0, 0));
        files.push(GitCommitFileDto {
            path,
            status,
            additions,
            deletions,
        });
    }

    Ok(files)
}

/// Get the unified patch for one file in a commit.
pub fn commit_file_diff(workspace: &str, sha: &str, path: &str) -> Result<String, String> {
    let repo = require_repo(workspace)?;
    let sha = sha.trim();
    let path = path.trim();
    if sha.is_empty() || path.is_empty() {
        return Ok(String::new());
    }

    let (stdout, stderr, code) =
        run_git(&repo, &["show", "--no-color", "--format=", sha, "--", path])?;
    if code != 0 {
        return Err(git_err("commit_file_diff", code, &stdout, &stderr));
    }
    Ok(stdout)
}

/// Delete a branch (safe or forced).
pub fn delete_branch(workspace: &str, branch: &str, force: bool) -> Result<String, String> {
    let repo = require_repo(workspace)?;
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("Branch name is empty".into());
    }
    let flag = if force { "-D" } else { "-d" };
    let (stdout, stderr, code) = run_git(&repo, &["branch", flag, branch])?;
    if code != 0 {
        return Err(git_err("delete_branch", code, &stdout, &stderr));
    }
    Ok(format!("Deleted branch {branch}"))
}

pub(crate) fn require_repo(workspace: &str) -> Result<PathBuf, String> {
    let cwd = Path::new(workspace);
    find_repo_root(cwd)?.ok_or_else(|| "Not a git repository".to_string())
}

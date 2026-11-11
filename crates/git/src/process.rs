//! git subprocess plumbing: command runner, repo-root discovery, numstat parsing.

use super::*;

pub(crate) fn run_git(cwd: &Path, args: &[&str]) -> Result<(String, String, i32), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        // Force English diagnostics: callers match on git's wording (a refused
        // checkout names its colliding files), and a localized stderr would
        // turn a handled failure back into an opaque one.
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let code = output.status.code().unwrap_or(-1);
    Ok((stdout, stderr, code))
}

/// Same as [`run_git`] but feeds `input` to the command's stdin.
///
/// `git apply` reads the patch from stdin; writing it to a temp file instead
/// would leave a copy of the user's source on disk and need cleanup on error.
pub(crate) fn run_git_stdin(
    cwd: &Path,
    args: &[&str],
    input: &str,
) -> Result<(String, String, i32), String> {
    use std::io::Write as _;
    use std::process::Stdio;

    let mut child = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run git: {e}"))?;

    child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open git stdin".to_string())?
        .write_all(input.as_bytes())
        .map_err(|e| format!("Failed to write patch: {e}"))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let code = output.status.code().unwrap_or(-1);
    Ok((stdout, stderr, code))
}

pub(crate) fn find_repo_root(path: &Path) -> Result<Option<PathBuf>, String> {
    let (stdout, _stderr, code) = run_git(path, &["rev-parse", "--show-toplevel"])?;
    if code != 0 {
        return Ok(None);
    }
    let root = stdout.trim();
    if root.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(root)))
}

/// `(additions, deletions)` for one path.
pub(crate) type LineCounts = (u32, u32);

/// `git diff --numstat -z`, keyed by the post-image path.
///
/// `staged` selects index-vs-HEAD; otherwise it is worktree-vs-index, which is
/// the side the row it decorates actually belongs to. Failure is silent: line
/// counts are decoration, and a status snapshot without them still works.
pub(crate) fn numstat(repo: &Path, staged: bool) -> HashMap<String, LineCounts> {
    let mut args = vec!["diff", "--numstat", "-z"];
    if staged {
        args.push("--cached");
    }
    let Ok((stdout, _stderr, code)) = run_git(repo, &args) else {
        return HashMap::new();
    };
    if code != 0 {
        return HashMap::new();
    }
    parse_numstat(&stdout)
}

/// Split `--numstat -z` output.
///
/// A rename entry carries an empty path field followed by both paths:
/// `a\td\t\0old\0new\0`. Every other entry is `a\td\tpath\0`. Binary files
/// report `-` for both counts, which [`parse`](str::parse) rejects into `0`.
pub(crate) fn parse_numstat(out: &str) -> HashMap<String, LineCounts> {
    let mut map = HashMap::new();
    let mut rest = out;
    while !rest.is_empty() {
        let Some(tab) = rest.find('\t') else { break };
        let add = &rest[..tab];
        rest = &rest[tab + 1..];
        let Some(tab) = rest.find('\t') else { break };
        let del = &rest[..tab];
        rest = &rest[tab + 1..];

        let count = (add.parse().unwrap_or(0), del.parse().unwrap_or(0));
        if let Some(renamed) = rest.strip_prefix('\0') {
            // Rename: old path, then new path. The new path is the row's key.
            let Some(nul) = renamed.find('\0') else { break };
            let after_old = &renamed[nul + 1..];
            let Some(nul) = after_old.find('\0') else {
                break;
            };
            map.insert(after_old[..nul].to_string(), count);
            rest = &after_old[nul + 1..];
        } else {
            let Some(nul) = rest.find('\0') else { break };
            map.insert(rest[..nul].to_string(), count);
            rest = &rest[nul + 1..];
        }
    }
    map
}

/// Untracked files never appear in `git diff`, so their additions are counted
/// from disk. Anything large or binary reports zero rather than making the
/// background refresh read a multi-hundred-megabyte artifact.
pub(crate) const UNTRACKED_COUNT_CAP: u64 = 512 * 1024;

pub(crate) fn untracked_counts(repo: &Path, path: &str) -> LineCounts {
    let abs = repo.join(path);
    let Ok(meta) = std::fs::metadata(&abs) else {
        return (0, 0);
    };
    if !meta.is_file() || meta.len() > UNTRACKED_COUNT_CAP {
        return (0, 0);
    }
    let Ok(bytes) = std::fs::read(&abs) else {
        return (0, 0);
    };
    if bytes.contains(&0) {
        return (0, 0);
    }
    let mut lines = bytes.iter().filter(|b| **b == b'\n').count() as u32;
    if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
        lines += 1;
    }
    (lines, 0)
}

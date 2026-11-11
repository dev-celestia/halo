//! Coder git command tests.

use super::*;

use super::mutations::*;
use super::process::*;
use super::queries::*;
use super::status::*;

fn header(line: &str) -> GitStatus {
    let mut status = GitStatus::default();
    parse_branch_header(line, &mut status);
    status
}

#[test]
fn parses_plain_branch() {
    let status = header("main");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.upstream, None);
}

/// `--numstat -z` emits `a\td\tpath\0` per file and `a\td\t\0old\0new\0`
/// for a rename; binary files use `-` for both counts.
#[test]
fn parses_numstat_entries_and_renames() {
    let out = "3\t1\tsrc/a.rs\0-\t-\timg.png\00\t0\t\0old/name.rs\0new/name.rs\0";
    let map = parse_numstat(out);
    assert_eq!(map.get("src/a.rs"), Some(&(3, 1)));
    assert_eq!(map.get("img.png"), Some(&(0, 0)));
    assert_eq!(map.get("new/name.rs"), Some(&(0, 0)));
    assert_eq!(map.len(), 3);
}

#[test]
fn parses_tracking_branch() {
    let status = header("main...origin/main [ahead 1, behind 2]");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!(status.ahead, 1);
    assert_eq!(status.behind, 2);
}

/// `git status` reports an unborn branch as `No commits yet on <name>`;
/// a naive space-split would call the branch "No".
#[test]
fn parses_unborn_branch() {
    assert_eq!(
        header("No commits yet on main").branch.as_deref(),
        Some("main")
    );
    assert_eq!(
        header("Initial commit on trunk").branch.as_deref(),
        Some("trunk")
    );
}

#[test]
fn detached_head_has_no_branch() {
    assert_eq!(header("HEAD (no branch)").branch, None);
}

#[test]
fn rejects_names_git_would_reject() {
    assert!(validate_branch_name("").is_err());
    assert!(validate_branch_name("bad name").is_err());
    assert!(validate_branch_name("-oops").is_err());
    assert!(validate_branch_name("a..b").is_err());
    assert!(validate_branch_name("feat/x").is_ok());
    assert!(validate_branch_name("fix-123").is_ok());
}

/// End-to-end: two worktree edits, stage only the first hunk through
/// `apply_patch`, then take it back out again.
///
/// This is the path the diff pane's per-hunk buttons use, and it exercises
/// the stdin plumbing in [`run_git_stdin`] as well as git's acceptance of a
/// patch rebuilt from one hunk of a larger diff.
#[test]
fn stages_and_unstages_a_single_hunk() {
    let dir = std::env::temp_dir().join(format!(
        "halo-hunk-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let workspace = dir.to_string_lossy().to_string();

    let body: String = (1..=30).map(|n| format!("line {n}\n")).collect();
    std::fs::write(dir.join("f.txt"), &body).expect("seed file");

    run_git(&dir, &["init", "-q", "-b", "main"]).expect("init");
    run_git(&dir, &["add", "f.txt"]).expect("add");
    let (_, stderr, code) = run_git(
        &dir,
        &[
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
    )
    .expect("commit");
    assert_eq!(code, 0, "seed commit failed: {stderr}");

    let edited = body
        .replace("line 3\n", "line 3 CHANGED\n")
        .replace("line 27\n", "line 27 CHANGED\n");
    std::fs::write(dir.join("f.txt"), edited).expect("edit file");

    // Rebuild the first hunk exactly the way the diff pane does.
    let full = diff(&workspace, "f.txt", false).expect("diff");
    let mut header = Vec::new();
    let mut hunk = Vec::new();
    for line in full.lines() {
        if line.starts_with("@@") {
            hunk.push(line);
            continue;
        }
        if hunk.is_empty() {
            header.push(line);
        } else if !line.starts_with("@@") {
            hunk.push(line);
        }
    }
    // Drop the second hunk: everything from its `@@` onward.
    let second = hunk
        .iter()
        .position(|l| l.starts_with("@@") && *l != hunk[0])
        .unwrap_or(hunk.len());
    let patch = format!(
        "{}\n",
        header.join("\n") + "\n" + &hunk[..second].join("\n")
    );

    apply_patch(&workspace, &patch, false).expect("stage hunk");

    let after_stage = status(&workspace).expect("status");
    assert_eq!(after_stage.staged.len(), 1, "hunk 1 staged");
    assert_eq!(after_stage.unstaged.len(), 1, "hunk 2 still unstaged");

    // Unstage: the staged diff is now a one-hunk patch, applied backwards.
    let staged = diff(&workspace, "f.txt", true).expect("staged diff");
    apply_patch(&workspace, &staged, true).expect("unstage hunk");

    let after_unstage = status(&workspace).expect("status");
    assert!(after_unstage.staged.is_empty(), "index back to HEAD");
    assert_eq!(after_unstage.unstaged.len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

/// The recovery a refused branch switch offers in the UI: the colliding
/// edits move to a stash instead of being discarded, and the checkout lands.
#[test]
fn stash_and_checkout_parks_overlapping_edits() {
    let dir = std::env::temp_dir().join(format!(
        "halo-stash-switch-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let workspace = dir.to_string_lossy().to_string();
    let config = ["-c", "user.email=t@example.com", "-c", "user.name=t"];

    let mut init = config.to_vec();
    init.extend(["init", "-q", "-b", "main"]);
    run_git(&dir, &init).expect("init");

    std::fs::write(dir.join("f.txt"), "base\n").expect("seed");
    run_git(&dir, &["add", "f.txt"]).expect("add");
    let mut commit = config.to_vec();
    commit.extend(["commit", "-qm", "base"]);
    run_git(&dir, &commit).expect("commit");

    run_git(&dir, &["checkout", "-q", "-b", "feature"]).expect("branch");
    std::fs::write(dir.join("f.txt"), "feature\n").expect("feature edit");
    run_git(&dir, &["add", "f.txt"]).expect("add feature");
    let mut commit = config.to_vec();
    commit.extend(["commit", "-qm", "feature"]);
    run_git(&dir, &commit).expect("commit feature");
    run_git(&dir, &["checkout", "-q", "main"]).expect("back to main");

    std::fs::write(dir.join("f.txt"), "work in progress\n").expect("dirty edit");
    assert!(
        checkout(&workspace, "feature").is_err(),
        "checkout must refuse to overwrite the edit"
    );

    let msg = stash_and_checkout(&workspace, "feature").expect("stash and switch");
    assert!(msg.contains("Stashed"), "{msg}");
    assert_eq!(
        std::fs::read_to_string(dir.join("f.txt")).expect("read"),
        "feature\n"
    );
    assert_eq!(list_stashes(&workspace).expect("stashes").len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

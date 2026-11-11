//! Source Control and Git Graph tests.

use super::*;
use halo_backend::commands::coder_git::GitCommitDto;

/// The Git Output dock auto-opens on the first failure and keeps only the
/// newest entries; an older error must not evict the newest one.
#[test]
fn git_output_ring_keeps_newest_and_auto_opens() {
    let mut state = BridgeState::default();
    assert!(!state.git_output_open);

    state.push_git_output("Switching to main", "fatal: refused");
    assert!(state.git_output_open);
    assert_eq!(state.git_output.len(), 1);
    assert_eq!(state.git_output[0].label, "Switching to main");

    for i in 0..BridgeState::GIT_OUTPUT_LIMIT + 10 {
        state.push_git_output(format!("op {i}"), format!("err {i}"));
    }
    assert_eq!(state.git_output.len(), BridgeState::GIT_OUTPUT_LIMIT);
    assert_eq!(
        state.git_output.last().unwrap().label,
        format!("op {}", BridgeState::GIT_OUTPUT_LIMIT + 9)
    );
    assert_eq!(state.git_output[0].label, "op 10");

    // Retrying the same failing command (auto-refresh) must not add a row.
    let before = state.git_output.len();
    state.push_git_output("op 209", "err 209");
    assert_eq!(state.git_output.len(), before);
    assert_eq!(state.git_output.last().unwrap().label, "op 209");

    // Git failures toast as a pointer to the dock, not as a red error.
    state.apply_git_err(
        "Push",
        "fatal: rejected".into(),
        "Push failed — see Git Output".into(),
    );
    assert!(state.last_error_info);
    assert_eq!(
        state.last_error.as_deref(),
        Some("Push failed — see Git Output")
    );
    assert_eq!(state.git_output.last().unwrap().detail, "fatal: rejected");

    // A non-git error takes the toast lane back.
    state.apply_err(crate::BridgeError::message("boom"));
    assert!(!state.last_error_info);
}

/// Real `git checkout` stderr for overlapping edits: the header, tab-indented
/// paths, then advice that must not be mistaken for a file.
#[test]
fn checkout_refusal_lists_only_the_colliding_paths() {
    let detail =
        "error: Your local changes to the following files would be overwritten by checkout:\n\
                  \tsrc/a.rs\n\
                  \tapps/web/package.json\n\
                  Please commit your changes or stash them before you switch branches.\n\
                  Aborting\n";
    let files = checkout_conflict_files(detail).expect("blocked checkout is recognised");
    assert_eq!(files, vec!["src/a.rs", "apps/web/package.json"]);
}

#[test]
fn unrelated_git_errors_are_not_checkout_blocks() {
    let detail = "error: pathspec 'nope' did not match any file(s) known to git\n";
    assert_eq!(checkout_conflict_files(detail), None);
}

#[test]
fn test_dag_lane_layout_linear() {
    let commits = vec![
        GitCommitDto {
            sha: "c3".into(),
            parents: vec!["c2".into()],
            author_name: "dev".into(),
            author_email: "dev@example.com".into(),
            timestamp: 1700000003,
            refs: vec!["HEAD -> master".into()],
            subject: "commit 3".into(),
        },
        GitCommitDto {
            sha: "c2".into(),
            parents: vec!["c1".into()],
            author_name: "dev".into(),
            author_email: "dev@example.com".into(),
            timestamp: 1700000002,
            refs: vec![],
            subject: "commit 2".into(),
        },
        GitCommitDto {
            sha: "c1".into(),
            parents: vec![],
            author_name: "dev".into(),
            author_email: "dev@example.com".into(),
            timestamp: 1700000001,
            refs: vec!["tag: v1.0".into()],
            subject: "commit 1".into(),
        },
    ];

    let graph = compute_graph_lanes(commits);
    assert_eq!(graph.len(), 3);
    assert_eq!(graph[0].lane, 0);
    assert_eq!(graph[1].lane, 0);
    assert_eq!(graph[2].lane, 0);
    assert!(!graph[0].has_incoming);
    assert!(graph[1].has_incoming);
    assert!(graph[2].has_incoming);

    assert_eq!(graph[0].refs.len(), 1);
    assert_eq!(graph[0].refs[0].name, "master");
    assert!(graph[0].refs[0].is_head);

    assert_eq!(graph[2].refs.len(), 1);
    assert_eq!(graph[2].refs[0].name, "v1.0");
    assert_eq!(graph[2].refs[0].kind, crate::work::GitGraphRefKind::Tag);
}

#[test]
fn test_dag_lane_layout_merge_commit() {
    let commits = vec![
        GitCommitDto {
            sha: "m1".into(),
            parents: vec!["c2".into(), "f1".into()],
            author_name: "dev".into(),
            author_email: "dev@example.com".into(),
            timestamp: 1700000005,
            refs: vec!["HEAD -> master".into()],
            subject: "Merge feature".into(),
        },
        GitCommitDto {
            sha: "c2".into(),
            parents: vec!["c1".into()],
            author_name: "dev".into(),
            author_email: "dev@example.com".into(),
            timestamp: 1700000004,
            refs: vec![],
            subject: "commit c2".into(),
        },
        GitCommitDto {
            sha: "f1".into(),
            parents: vec!["c1".into()],
            author_name: "dev".into(),
            author_email: "dev@example.com".into(),
            timestamp: 1700000003,
            refs: vec!["feature".into()],
            subject: "feature 1".into(),
        },
        GitCommitDto {
            sha: "c1".into(),
            parents: vec![],
            author_name: "dev".into(),
            author_email: "dev@example.com".into(),
            timestamp: 1700000001,
            refs: vec![],
            subject: "root".into(),
        },
    ];

    let graph = compute_graph_lanes(commits);
    assert_eq!(graph.len(), 4);
    assert_eq!(graph[0].lane, 0);
    assert_eq!(graph[0].parent_lanes.len(), 2);
    assert_eq!(graph[0].parent_lanes[0], 0);
    assert_eq!(graph[0].parent_lanes[1], 1);

    assert_eq!(graph[2].lane, 1);
}

#[test]
fn test_repo_tabs_filtering() {
    let mut state = crate::work::GitGraphState::default();
    state.repo_tabs = vec!["/path/to/repoA".into(), "/path/to/repoB".into()];
    state.active_repo_tab = 0;
    assert_eq!(state.active_repo(), Some("/path/to/repoA"));

    state.commits.push(crate::work::GitGraphCommit {
        sha: "abcdef123456".into(),
        short_sha: "abcdef1".into(),
        author_name: "Alice Developer".into(),
        subject: "Add brilliant feature".into(),
        ..Default::default()
    });
    state.commits.push(crate::work::GitGraphCommit {
        sha: "9876543210ab".into(),
        short_sha: "9876543".into(),
        author_name: "Bob Fixer".into(),
        subject: "Fix critical bug".into(),
        ..Default::default()
    });

    state.filter_query = "feature".into();
    let filtered = state.filtered_commits();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].short_sha, "abcdef1");

    state.filter_query = "bob".into();
    let filtered = state.filtered_commits();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].short_sha, "9876543");
}

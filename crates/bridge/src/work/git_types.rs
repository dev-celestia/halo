//! Git snapshot types for the Source Control page and the Git Graph.

/// One changed path in the Coder git panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitFile {
    pub path: String,
    pub status: String,
    /// Set for renames: the path before the rename (`old` in `old -> new`).
    pub previous_path: Option<String>,
    /// Lines added on this row's side of the index (`+N` badge).
    pub additions: u32,
    /// Lines removed on this row's side of the index (`-N` badge).
    pub deletions: u32,
}

/// Unified diff currently shown in the Source Control panel.
///
/// UI-only view state: it lives on [`BridgeState`](crate::BridgeState) rather
/// than on [`GitState`] so a status refresh does not silently swap the
/// text under the reader's cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitDiff {
    pub path: String,
    /// `true` when the diff is index-vs-HEAD rather than worktree-vs-index.
    pub staged: bool,
    /// Raw `git diff` output; may be empty for binary or mode-only changes.
    pub text: String,
}

/// Which surface the Source Control activity rail is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SourceControlPane {
    #[default]
    Changes,
    Graph,
    PullRequests,
}

/// Inbox filter for the pull-request list. Applied on the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PullRequestFilter {
    #[default]
    Open,
    Merged,
    Closed,
    All,
}

impl PullRequestFilter {
    pub fn matches(self, state: &str) -> bool {
        let state = state.to_ascii_uppercase();
        match self {
            Self::Open => state == "OPEN",
            Self::Merged => state == "MERGED",
            Self::Closed => state == "CLOSED",
            Self::All => true,
        }
    }

    pub fn empty_label(self) -> &'static str {
        match self {
            Self::Open => "No open pull requests.",
            Self::Merged => "No merged pull requests.",
            Self::Closed => "No closed pull requests.",
            Self::All => "No pull requests.",
        }
    }
}

/// One row from `gh pr list`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub body: String,
    pub updated_at: String,
}

/// Pull-request inbox for the open project.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PullRequestView {
    pub items: Vec<PullRequest>,
    pub filter: PullRequestFilter,
    pub query: String,
    pub selected: Option<u64>,
    pub error: Option<String>,
    pub loading: bool,
    /// `brew install gh` is running from the pull-request pane.
    pub installing_gh: bool,
}

impl PullRequestView {
    pub fn visible(&self) -> Vec<&PullRequest> {
        let q = self.query.trim().to_ascii_lowercase();
        self.items
            .iter()
            .filter(|pr| self.filter.matches(&pr.state))
            .filter(|pr| {
                if q.is_empty() {
                    return true;
                }
                pr.title.to_ascii_lowercase().contains(&q)
                    || pr.number.to_string().contains(q.trim_start_matches('#'))
            })
            .collect()
    }

    pub fn open_count(&self) -> usize {
        self.items
            .iter()
            .filter(|pr| pr.state.eq_ignore_ascii_case("open"))
            .count()
    }
}

/// A branch switch git refused because local edits would be overwritten.
///
/// Carried on [`GitState`] so the Source Control panes can explain the refusal
/// in their own words and offer the stash-then-switch recovery, instead of
/// echoing git's multi-line stderr.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCheckoutBlock {
    /// Branch the switch was heading to.
    pub branch: String,
    /// Repository the switch ran in. Repo tabs can point away from the open
    /// project, so the retry must land on the same tree.
    pub root: String,
    /// Paths git listed as colliding.
    pub files: Vec<String>,
    /// The switch came from the Graph pane; a successful retry must refresh
    /// that pane as well as the changes snapshot.
    pub from_graph: bool,
}

/// Cached `git status` snapshot for the open workspace.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitState {
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub remotes: Vec<String>,
    pub local_branches: Vec<String>,
    pub staged: Vec<GitFile>,
    pub unstaged: Vec<GitFile>,
    pub untracked: Vec<GitFile>,
    /// Paths matching exclude rules (dimmed in the explorer).
    pub ignored: Vec<GitFile>,
    pub not_a_repo: bool,
    pub error: Option<String>,
    /// Set when the last failure was a checkout blocked by local edits; cleared
    /// by the next refresh or operation.
    pub checkout_block: Option<GitCheckoutBlock>,
}

impl GitState {
    pub fn from_backend(status: halo_backend::commands::coder_git::GitStatus) -> Self {
        let map = |entries: Vec<halo_backend::commands::coder_git::GitFileEntry>| {
            entries
                .into_iter()
                .map(|e| GitFile {
                    path: e.path,
                    status: e.status,
                    previous_path: e.previous_path,
                    additions: e.additions,
                    deletions: e.deletions,
                })
                .collect()
        };
        Self {
            repo_root: status.repo_root,
            branch: status.branch,
            upstream: status.upstream,
            ahead: status.ahead,
            behind: status.behind,
            remotes: status.remotes,
            local_branches: status.local_branches,
            staged: map(status.staged),
            unstaged: map(status.unstaged),
            untracked: map(status.untracked),
            ignored: map(status.ignored),
            not_a_repo: status.not_a_repo,
            error: None,
            checkout_block: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GitInspectorTab {
    #[default]
    Changes,
    Details,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitGraphRefKind {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitGraphRef {
    pub name: String,
    pub kind: GitGraphRefKind,
    pub is_head: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitGraphCommit {
    pub sha: String,
    pub short_sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub refs: Vec<GitGraphRef>,
    pub subject: String,
    pub lane: usize,
    pub active_lanes: Vec<Option<usize>>,
    pub parent_lanes: Vec<usize>,
    pub has_incoming: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitGraphBranch {
    pub name: String,
    pub is_current: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Fully merged into HEAD — the branch menu deletes with `-d` when true and
    /// `-D` (after a warning) when false.
    pub merged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitGraphRemote {
    pub name: String,
    pub branches: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitGraphStash {
    pub index: usize,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitGraphWorktree {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitGraphCommitFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GitGraphState {
    pub repo_tabs: Vec<String>,
    pub active_repo_tab: usize,
    pub commits: Vec<GitGraphCommit>,
    pub selected_commit: Option<String>,
    pub commit_files: Vec<GitGraphCommitFile>,
    pub selected_commit_file: Option<String>,
    pub commit_diff: Option<GitDiff>,
    pub branches: Vec<GitGraphBranch>,
    pub remotes: Vec<GitGraphRemote>,
    pub tags: Vec<String>,
    pub stashes: Vec<GitGraphStash>,
    pub worktrees: Vec<GitGraphWorktree>,
    pub filter_query: String,
    pub selected_inspector_tab: GitInspectorTab,
    pub error: Option<String>,
    pub is_loading: bool,
    /// Accordion section keys currently collapsed in the Branch Explorer
    /// (`worktrees`, `branches`, `remotes`, `tags`, `stashes`). Empty ⇒ all open.
    pub collapsed_sections: Vec<String>,
}

impl GitGraphState {
    pub fn active_repo(&self) -> Option<&str> {
        self.repo_tabs.get(self.active_repo_tab).map(String::as_str)
    }

    pub fn section_open(&self, key: &str) -> bool {
        !self.collapsed_sections.iter().any(|k| k == key)
    }

    /// Replace collapsed keys from an Accordion `on_toggle_click` open-index set.
    pub fn set_open_sections(&mut self, open_ixs: &[usize], keys: &[String]) {
        self.collapsed_sections = keys
            .iter()
            .enumerate()
            .filter(|(i, _)| !open_ixs.contains(i))
            .map(|(_, k)| k.clone())
            .collect();
    }

    pub fn filtered_commits(&self) -> Vec<&GitGraphCommit> {
        let q = self.filter_query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return self.commits.iter().collect();
        }
        self.commits
            .iter()
            .filter(|c| {
                c.subject.to_ascii_lowercase().contains(&q)
                    || c.author_name.to_ascii_lowercase().contains(&q)
                    || c.short_sha.to_ascii_lowercase().starts_with(&q)
                    || c.refs
                        .iter()
                        .any(|r| r.name.to_ascii_lowercase().contains(&q))
            })
            .collect()
    }
}

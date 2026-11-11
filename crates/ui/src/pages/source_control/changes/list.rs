//! Virtualized changes list, rows, section kinds, and capabilities.

use std::rc::Rc;

use celestia_ui::components::composite::virtual_list::{
    VirtualListBuilder, VirtualListScrollHandle,
};
use gpui::*;
use halo_bridge::{GitFile, GitState};

use super::file_row::{changes_file_row, ignored_toggle, section_header};

/// Uniform heights for virtualized Source Control rows.
pub(crate) const GIT_SECTION_ROW_H: Pixels = px(28.);
/// File rows are single-line (name only); the full path is a hover tooltip.
pub(crate) const GIT_FILE_ROW_H: Pixels = px(32.);
pub(crate) const GIT_SECTION_GAP_H: Pixels = px(8.);

/// What the current snapshot can actually do.
///
/// Every button and menu item gates on one of these, so a click can never run
/// an operation git would reject (no remote, detached HEAD, empty index).
#[derive(Clone, Copy, Debug)]
pub(crate) struct GitCaps {
    pub(crate) in_repo: bool,
    pub(crate) has_branch: bool,
    pub(crate) has_remote: bool,
    pub(crate) has_staged: bool,
    pub(crate) has_unstaged: bool,
    /// A remote operation is in flight; every network action is locked out.
    pub(crate) busy: bool,
}

impl GitCaps {
    pub(crate) fn new(git: &GitState, busy: bool) -> Self {
        Self {
            in_repo: !git.not_a_repo,
            has_branch: git.branch.as_deref().is_some_and(|b| !b.is_empty()),
            has_remote: !git.remotes.is_empty(),
            has_staged: !git.staged.is_empty(),
            has_unstaged: !git.unstaged.is_empty() || !git.untracked.is_empty(),
            busy,
        }
    }

    /// Push / Pull / Sync need a checked-out branch plus somewhere to send it.
    pub(crate) fn can_sync_with_remote(self) -> bool {
        self.in_repo && self.has_branch && self.has_remote && !self.busy
    }

    /// Fetch only needs a remote; it works on a detached HEAD.
    pub(crate) fn can_fetch(self) -> bool {
        self.in_repo && self.has_remote && !self.busy
    }

    /// Amend rewrites HEAD, so a commit must already exist on a branch.
    pub(crate) fn can_amend(self) -> bool {
        self.in_repo && self.has_branch
    }
}

/// Which list a row belongs to. Drives available actions and styling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SectionKind {
    Staged,
    Unstaged,
    Untracked,
    Ignored,
}

impl SectionKind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Staged => "s",
            Self::Unstaged => "u",
            Self::Untracked => "n",
            Self::Ignored => "i",
        }
    }

    pub(crate) fn is_staged(self) -> bool {
        matches!(self, Self::Staged)
    }
}

/// One virtual-list entry: section chrome, a file row, or vertical gap.
#[derive(Clone)]
pub(crate) enum GitListItem {
    /// `gap_2` between sections in the old non-virtual layout.
    Gap,
    Section {
        title: SharedString,
        kind: SectionKind,
        count: usize,
    },
    File {
        file: GitFile,
        kind: SectionKind,
    },
    IgnoredToggle {
        count: usize,
        open: bool,
    },
}

impl GitListItem {
    pub(crate) fn height(&self) -> Pixels {
        match self {
            Self::Gap => GIT_SECTION_GAP_H,
            Self::Section { .. } | Self::IgnoredToggle { .. } => GIT_SECTION_ROW_H,
            Self::File { .. } => GIT_FILE_ROW_H,
        }
    }
}

/// Virtualized changes list for the Source Control column.
///
/// Owned by [`ToolEditors`] so [`VirtualListBuilder`] has a stable
/// [`Render`] entity across frames; [`Self::sync`] refreshes the snapshot
/// whenever the panel re-renders from bridge state.
pub struct GitChangesList {
    scroll_handle: VirtualListScrollHandle,
    items: Vec<GitListItem>,
    repo_root: String,
    editors: Vec<Entity<celestia_ui::components::CodeEditor>>,
    selection: Vec<String>,
    active_diff: Option<(String, bool)>,
}

impl GitChangesList {
    pub fn new(_cx: &mut App) -> Self {
        Self {
            scroll_handle: VirtualListScrollHandle::new(),
            items: Vec::new(),
            repo_root: String::new(),
            editors: Vec::new(),
            selection: Vec::new(),
            active_diff: None,
        }
    }

    /// Rebuild the flat item list from the current git snapshot.
    pub fn sync(
        &mut self,
        git: &GitState,
        repo_root: &str,
        editors: &[Entity<celestia_ui::components::CodeEditor>],
        selection: &[String],
        active_diff: Option<&(String, bool)>,
        ignored_open: bool,
    ) {
        self.repo_root = repo_root.to_string();
        self.editors = editors.to_vec();
        self.selection = selection.to_vec();
        self.active_diff = active_diff.cloned();

        let mut items = Vec::new();
        let mut first_section = true;
        for (title, files, kind) in [
            ("Staged Changes", &git.staged, SectionKind::Staged),
            ("Unstaged Changes", &git.unstaged, SectionKind::Unstaged),
            ("Untracked Files", &git.untracked, SectionKind::Untracked),
        ] {
            if files.is_empty() {
                continue;
            }
            if !first_section {
                items.push(GitListItem::Gap);
            }
            first_section = false;
            items.push(GitListItem::Section {
                title: SharedString::from(title),
                kind,
                count: files.len(),
            });
            for file in files {
                items.push(GitListItem::File {
                    file: file.clone(),
                    kind,
                });
            }
        }

        if !git.ignored.is_empty() {
            if !first_section {
                items.push(GitListItem::Gap);
            }
            items.push(GitListItem::IgnoredToggle {
                count: git.ignored.len(),
                open: ignored_open,
            });
            if ignored_open {
                for file in &git.ignored {
                    items.push(GitListItem::File {
                        file: file.clone(),
                        kind: SectionKind::Ignored,
                    });
                }
            }
        }

        self.items = items;
    }

    fn render_item(&self, ix: usize, cx: &App) -> AnyElement {
        let Some(item) = self.items.get(ix) else {
            return div().into_any_element();
        };
        match item {
            GitListItem::Gap => div().w_full().h(GIT_SECTION_GAP_H).into_any_element(),
            GitListItem::Section { title, kind, count } => {
                section_header(title, *kind, *count, cx).into_any_element()
            }
            GitListItem::IgnoredToggle { count, open } => {
                ignored_toggle(*count, *open, cx).into_any_element()
            }
            GitListItem::File { file, kind } => {
                let viewing = self.active_diff.as_ref().is_some_and(|(path, staged)| {
                    path == &file.path && *staged == kind.is_staged()
                });
                let selected = self.selection.iter().any(|p| p == &file.path);
                changes_file_row(
                    file,
                    *kind,
                    &self.repo_root,
                    &self.editors,
                    selected,
                    viewing,
                    cx,
                )
                .into_any_element()
            }
        }
    }
}

impl Render for GitChangesList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sizes = Rc::new(
            self.items
                .iter()
                .map(|item| size(px(0.), item.height()))
                .collect::<Vec<_>>(),
        );

        VirtualListBuilder::vertical(cx.entity(), "sc-changes")
            .item_sizes(sizes)
            .track_scroll(&self.scroll_handle)
            .render_item(|this, ix, _window, cx| this.render_item(ix, cx))
            .size_full()
    }
}

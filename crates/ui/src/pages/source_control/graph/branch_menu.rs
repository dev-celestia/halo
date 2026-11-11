//! Right-click menu for a local branch in the Branch Explorer.
//!
//! The menu is built from a [`BranchMenuCtx`] snapshot: the popup outlives the
//! frame that spawned it, so it must not borrow `BridgeState`. Every mutating
//! entry goes through a `Bridge::git_branch_*` command, which runs off the UI
//! thread behind the `git_busy` guard and refreshes the graph on success.

use gpui::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use halo_bridge::Bridge;

use crate::components::{confirm_destructive, prompt_text};

/// What an AI entry should ask the agent for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BranchAiIntent {
    Review,
    Explain,
    Summarise,
}

/// Snapshot of one branch row, taken when the row is rendered.
#[derive(Clone, Debug, Default)]
pub(crate) struct BranchMenuCtx {
    pub(crate) branch: String,
    pub(crate) is_current: bool,
    /// Fully merged into HEAD — delete uses `-d` when true and `-D` (with a
    /// warning) when false.
    pub(crate) merged: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: u32,
    pub(crate) behind: u32,
    /// Branch HEAD points at, for "Merge into…"-style context and delete copy.
    pub(crate) current_branch: Option<String>,
    /// A git operation is already running; mutating items are disabled so a
    /// second one cannot be queued behind the `git_busy` guard.
    pub(crate) busy: bool,
}

pub(crate) fn branch_menu(
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
    ctx: &BranchMenuCtx,
) -> PopupMenu {
    let branch = ctx.branch.clone();

    let mut menu = menu.item(
        PopupMenuItem::new(format!("Checkout '{branch}'"))
            .checked(ctx.is_current)
            .disabled(ctx.is_current || ctx.busy)
            .on_click({
                let branch = branch.clone();
                move |_, _, cx| {
                    let _ = Bridge::git_graph_checkout(&branch, cx);
                }
            }),
    );

    menu = menu.separator().item(
        PopupMenuItem::new("New Branch…")
            .disabled(ctx.busy)
            .on_click({
                let base = branch.clone();
                move |_, window, cx| {
                    let base = base.clone();
                    prompt_text(
                        window,
                        cx,
                        SharedString::from(format!("New branch from '{base}'")),
                        "",
                        "Create",
                        move |name, cx| {
                            let _ = Bridge::git_branch_create_from(name, &base, cx);
                        },
                    );
                }
            }),
    );

    menu = menu
        .separator()
        .item(PopupMenuItem::new("Rename…").disabled(ctx.busy).on_click({
            let from = branch.clone();
            move |_, window, cx| {
                let target = from.clone();
                prompt_text(
                    window,
                    cx,
                    SharedString::from(format!("Rename branch '{from}'")),
                    &from,
                    "Rename",
                    move |to, cx| {
                        let _ = Bridge::git_branch_rename(&target, to, cx);
                    },
                );
            }
        }));

    let merged = ctx.merged;
    let current_branch = ctx.current_branch.clone();
    menu = menu.item(
        PopupMenuItem::new("Delete…")
            .disabled(ctx.is_current || ctx.busy)
            .on_click({
                let branch = branch.clone();
                let current_branch = current_branch.clone();
                move |_, window, cx| {
                    let branch = branch.clone();
                    let (title, body) = if merged {
                        (
                            format!("Delete branch '{branch}'?"),
                            format!("'{branch}' will be deleted. This cannot be undone."),
                        )
                    } else {
                        let against = current_branch.clone().unwrap_or_else(|| "HEAD".to_string());
                        (
                            format!("Delete branch '{branch}'?"),
                            format!(
                                "'{branch}' has commits that are not merged into '{against}'. \
                                 Deleting it will lose that work."
                            ),
                        )
                    };
                    // Unmerged branches get the forced flag; the copy above is
                    // the warning that replaces parsing git's refusal text.
                    let force = !merged;
                    confirm_destructive(window, cx, title, body, "Delete", move |_, cx| {
                        let _ = Bridge::git_branch_delete(&branch, force, cx);
                    });
                }
            }),
    );

    menu = menu.separator().submenu("AI", window, cx, {
        let ctx = ctx.clone();
        move |sub, _, _| {
            let mut sub = sub;
            for (intent, label) in [
                (BranchAiIntent::Review, "Review Branch Changes"),
                (BranchAiIntent::Explain, "Explain Branch Changes"),
                (BranchAiIntent::Summarise, "Summarise for a Pull Request"),
            ] {
                let ctx = ctx.clone();
                sub = sub.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = Bridge::git_queue_agent_prompt(branch_agent_prompt(intent, &ctx), cx);
                }));
            }
            sub
        }
    });

    menu.separator()
        .item(PopupMenuItem::new("Copy Branch Name").on_click({
            let branch = branch.clone();
            move |_, _, cx| {
                let _ = Bridge::git_copy_branch_name(&branch, cx);
            }
        }))
}

/// Prompt handed to the Agent composer for one AI entry.
///
/// Kept pure so the wording is testable and so every entry names the branch,
/// the base it is compared against, and the tracking state the agent would
/// otherwise have to guess.
pub(crate) fn branch_agent_prompt(intent: BranchAiIntent, ctx: &BranchMenuCtx) -> String {
    let branch = &ctx.branch;
    let base = branch_base(ctx);
    let tracking = match &ctx.upstream {
        Some(upstream) if ctx.ahead > 0 || ctx.behind > 0 => format!(
            "Tracking `{upstream}` ({}, {}).",
            ahead_behind(ctx.ahead, "ahead"),
            ahead_behind(ctx.behind, "behind"),
        ),
        Some(upstream) => format!("Tracking `{upstream}`, in sync."),
        None => "The branch has no upstream.".to_string(),
    };

    let task = match intent {
        BranchAiIntent::Review => format!(
            "Review the changes on branch `{branch}` against `{base}`. Use git to find the \
             merge base, read the diff, and open the touched files. Report correctness bugs, \
             security issues, missing tests, and anything that would block a merge. Do not \
             modify files."
        ),
        BranchAiIntent::Explain => format!(
            "Explain what branch `{branch}` changes relative to `{base}`: the intent, the areas \
             touched, and the risk of each. Use git for the commit log and the diff, and keep \
             the answer short enough to paste into a review comment."
        ),
        BranchAiIntent::Summarise => format!(
            "Write a pull request title and description for branch `{branch}` targeting `{base}`. \
             Cover what changed, why, how it was verified, and any follow-ups. Use git for the \
             commit log and the diff stat; do not modify files."
        ),
    };

    format!("{task}\n\n{tracking}")
}

/// Branch the agent should diff against: the checked-out branch for other
/// rows, and the upstream ref (or the default branch) when the row is HEAD.
fn branch_base(ctx: &BranchMenuCtx) -> String {
    match &ctx.current_branch {
        Some(current) if !ctx.is_current && current != &ctx.branch => current.clone(),
        _ => ctx
            .upstream
            .clone()
            .unwrap_or_else(|| "the default branch".to_string()),
    }
}

fn ahead_behind(count: u32, direction: &str) -> String {
    if count == 1 {
        format!("1 commit {direction}")
    } else {
        format!("{count} commits {direction}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(branch: &str, is_current: bool) -> BranchMenuCtx {
        BranchMenuCtx {
            branch: branch.into(),
            is_current,
            merged: true,
            upstream: Some("origin/main".into()),
            ahead: 2,
            behind: 0,
            current_branch: Some("main".into()),
            busy: false,
        }
    }

    #[test]
    fn review_prompt_names_branch_and_base() {
        let prompt = branch_agent_prompt(BranchAiIntent::Review, &ctx("feature/x", false));
        assert!(prompt.contains("`feature/x`"));
        assert!(prompt.contains("`main`"));
        assert!(prompt.contains("Do not modify files"));
    }

    #[test]
    fn current_branch_uses_upstream_as_base() {
        let prompt = branch_agent_prompt(BranchAiIntent::Explain, &ctx("main", true));
        assert!(prompt.contains("against `origin/main`"), "{prompt}");
        assert!(prompt.contains("Tracking `origin/main` (2 commits ahead)"));
    }

    #[test]
    fn tracking_line_counts_commits() {
        let mut ctx = ctx("feature/x", false);
        ctx.upstream = None;
        assert!(
            branch_agent_prompt(BranchAiIntent::Summarise, &ctx)
                .contains("The branch has no upstream.")
        );
    }

    #[test]
    fn base_falls_back_to_default_branch() {
        let mut ctx = ctx("main", true);
        ctx.upstream = None;
        assert!(
            branch_agent_prompt(BranchAiIntent::Review, &ctx)
                .contains("against `the default branch`")
        );
    }
}

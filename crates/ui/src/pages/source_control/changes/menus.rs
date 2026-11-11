//! Context menus for Source Control: top "More Actions" and per-file row menus.

use std::path::Path;

use celestia_ui::components::primitive::input::InputState;
use gpui::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use halo_bridge::{Bridge, CoderSidebar};

use super::list::{GitCaps, SectionKind};
use crate::components::confirm_destructive;

/// Everything the "More Actions" menu needs, bundled so the signature stays
/// readable and the flags travel together.
#[derive(Clone)]
pub(crate) struct GitMenuCtx {
    pub(crate) commit_input: Entity<InputState>,
    pub(crate) remotes: Vec<String>,
    pub(crate) local_branches: Vec<String>,
    pub(crate) caps: GitCaps,
}

pub(crate) fn changes_more_menu(
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
    ctx: &GitMenuCtx,
) -> PopupMenu {
    let caps = ctx.caps;
    let commit_input = ctx.commit_input.clone();
    let mut menu = menu
        .item(
            PopupMenuItem::new("Amend Last Commit")
                .disabled(!caps.can_amend())
                .on_click({
                    let commit_input = commit_input.clone();
                    move |_, _, cx| {
                        let msg = commit_input.read(cx).value().to_string();
                        let _ = Bridge::git_amend(&msg, cx);
                    }
                }),
        )
        .separator()
        .item(
            PopupMenuItem::new("Push")
                .disabled(!caps.can_sync_with_remote())
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_push(cx);
                }),
        );

    if ctx.remotes.is_empty() {
        menu = menu.item(PopupMenuItem::new("Push to…").disabled(true));
    } else {
        let remotes = ctx.remotes.clone();
        let local_branches = ctx.local_branches.clone();
        menu = menu.submenu("Push to…", window, cx, move |sub, window, cx| {
            let mut sub = sub;
            for remote in &remotes {
                let remote_name = remote.clone();
                let branches = local_branches.clone();
                if branches.is_empty() {
                    sub = sub.item(
                        PopupMenuItem::new(remote.clone()).on_click(move |_, _, cx| {
                            let _ = Bridge::git_push_to(&remote_name, cx);
                        }),
                    );
                } else {
                    sub = sub.submenu(remote.clone(), window, cx, move |branch_menu, _, _| {
                        let mut branch_menu = branch_menu.item(
                            PopupMenuItem::new("Current Branch (HEAD)").on_click({
                                let remote_name = remote_name.clone();
                                move |_, _, cx| {
                                    let _ = Bridge::git_push_to(&remote_name, cx);
                                }
                            }),
                        );
                        branch_menu = branch_menu.separator();
                        for branch in &branches {
                            let remote_name = remote_name.clone();
                            let branch = branch.clone();
                            branch_menu = branch_menu.item(
                                PopupMenuItem::new(branch.clone()).on_click(move |_, _, cx| {
                                    let _ = Bridge::git_push_to_branch(&remote_name, &branch, cx);
                                }),
                            );
                        }
                        branch_menu
                    });
                }
            }
            sub
        });
    }

    menu = menu
        .item(
            PopupMenuItem::new("Pull")
                .disabled(!caps.can_sync_with_remote())
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_pull(cx);
                }),
        )
        .item(
            PopupMenuItem::new("Fetch")
                .disabled(!caps.can_fetch())
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_fetch(cx);
                }),
        )
        .item(
            PopupMenuItem::new("Sync")
                .disabled(!caps.can_sync_with_remote())
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_sync(cx);
                }),
        )
        .separator()
        .item(
            PopupMenuItem::new("Stage All")
                .disabled(!caps.has_unstaged)
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_stage_all(cx);
                }),
        )
        .item(
            PopupMenuItem::new("Unstage All")
                .disabled(!caps.has_staged)
                .on_click(|_, _, cx| {
                    let _ = Bridge::git_unstage_all(cx);
                }),
        );

    menu
}

pub(crate) fn changes_row_menu(
    menu: PopupMenu,
    path: &str,
    abs: &str,
    kind: SectionKind,
    discardable: bool,
    editors: &[Entity<celestia_ui::components::CodeEditor>],
) -> PopupMenu {
    let path = path.to_string();
    let abs = abs.to_string();
    let editors = editors.to_vec();

    let mut menu = menu.item({
        let abs = abs.clone();
        let editors = editors.clone();
        PopupMenuItem::new("Open in Editor").on_click(move |_, window, cx| {
            if Path::new(&abs).is_file() {
                let _ = Bridge::focus_or_open_work_tab(halo_bridge::WorkKind::Coder, cx);
                let _ = Bridge::coder_select_sidebar(CoderSidebar::Explorer, cx);
                let _ = Bridge::coder_open_file(&abs, &editors, window, cx);
            }
        })
    });

    if kind != SectionKind::Ignored {
        let staged = kind.is_staged();
        let diff_path = path.clone();
        menu = menu
            .separator()
            .item(PopupMenuItem::new("View Diff").on_click(move |_, _, cx| {
                let _ = Bridge::git_diff(&diff_path, staged, cx);
            }))
            .item(if kind.is_staged() {
                let path = path.clone();
                PopupMenuItem::new("Unstage").on_click(move |_, _, cx| {
                    let _ = Bridge::git_unstage(&path, cx);
                })
            } else {
                let path = path.clone();
                PopupMenuItem::new("Stage").on_click(move |_, _, cx| {
                    let _ = Bridge::git_stage(&path, cx);
                })
            });
    }

    if discardable {
        let path = path.clone();
        let untracked = kind == SectionKind::Untracked;
        let label = if untracked {
            "Delete File…"
        } else {
            "Discard Changes…"
        };
        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
            let path = path.clone();
            let (title, body, action) = if untracked {
                (
                    "Delete file?",
                    format!("\"{path}\" will be deleted from disk. This cannot be undone."),
                    "Delete",
                )
            } else {
                (
                    "Discard changes?",
                    format!(
                        "Uncommitted changes to \"{path}\" will be permanently discarded. This cannot be undone."
                    ),
                    "Discard",
                )
            };
            confirm_destructive(window, cx, title, body, action, move |_, cx| {
                let _ = Bridge::git_discard(&path, cx);
            });
        }));
    }

    menu.separator()
        .item({
            let abs = abs.clone();
            PopupMenuItem::new("Copy Path").on_click(move |_, _, cx| {
                let _ = Bridge::coder_copy_path(&abs, cx);
            })
        })
        .item({
            let path = path.clone();
            PopupMenuItem::new("Copy Relative Path").on_click(move |_, _, cx| {
                let _ = Bridge::coder_copy_relative_path(&path, cx);
            })
        })
}

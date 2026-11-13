//! Branch Explorer sidebar — foldable Worktrees, Branches, Remotes, Tags, Stashes.

use std::path::Path;

use celestia_ui::components::primitive::accordion::Accordion;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::ContextMenuExt as _;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::{Sizable as _, Size};
use halo_bridge::{Bridge, BridgeState, SourceControlPane};

use super::branch_menu::{BranchMenuCtx, branch_menu};
use crate::pages::editors::ToolEditors;
use crate::pages::source_control::changes::branch_bar::busy_row;

const SECTION_WORKTREES: &str = "worktrees";
const SECTION_BRANCHES: &str = "branches";
const SECTION_REMOTES: &str = "remotes";
const SECTION_TAGS: &str = "tags";
const SECTION_STASHES: &str = "stashes";

pub fn render_branch_explorer(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    let graph = &state.git_graph;
    let git = &state.git;

    let dirty_count = git.staged.len() + git.unstaged.len() + git.untracked.len();
    let total_commits = graph.commits.len();

    div()
        .id("sc-branch-explorer")
        .size_full()
        .flex()
        .flex_col()
        // The resizable split already draws the divider to the right; a border
        // here stacked a second 1px line against it (a 2px seam at 2x).
        .child(
            div()
                .h(px(42.))
                .px_3()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .truncate()
                        .child("Branches"),
                )
                .child(
                    Button::new("sc-exp-refresh")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::ArrowClockwise)
                        .tooltip("Refresh repository")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::git_graph_refresh(cx);
                        }),
                ),
        )
        // Pinned actions: tight intra-group gap; border separates from filter.
        .child(
            div()
                .flex()
                .flex_col()
                .px_2()
                .pt_2()
                .pb_1p5()
                .gap_0p5()
                .border_b_1()
                .border_color(theme.border)
                .child(pinned_row(
                    "sc-exp-local-changes",
                    PhosphorIcon::GitDiff,
                    "Local Changes",
                    format!("{dirty_count}"),
                    dirty_count > 0,
                    theme,
                    |_, _, cx| {
                        Bridge::select_source_control_pane(SourceControlPane::Changes, cx);
                    },
                ))
                .child(pinned_row(
                    "sc-exp-all-commits",
                    PhosphorIcon::GitCommit,
                    "All Commits",
                    format!("{total_commits}"),
                    false,
                    theme,
                    |_, _, cx| {
                        Bridge::git_graph_set_filter("", cx);
                    },
                )),
        )
        .child(
            div()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    Input::new(&editors.sc_graph_filter)
                        .prefix(Phosphor::new(PhosphorIcon::MagnifyingGlass).size(px(13.))),
                ),
        )
        // Branch ops run from this pane's rows, so keep the in-flight label
        // here. Failures go to the Git Output dock at the bottom of the
        // workspace (with the raw stderr) instead of a strip that pushed the
        // tree down.
        .when_some(state.git_busy.clone(), |d, label| {
            d.child(div().px_3().py_1().child(busy_row(&label, cx)))
        })
        .child(
            div()
                .id("sc-exp-tree-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(sections_accordion(state, cx)),
        )
}

fn sections_accordion(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let graph = &state.git_graph;
    let busy = state.git_busy.is_some();
    let current_branch = graph
        .branches
        .iter()
        .find(|b| b.is_current)
        .map(|b| b.name.clone());
    // Remote rows check out the local branch when one exists and the remote ref
    // otherwise; deciding here keeps checkout a single fire-and-forget call now
    // that it no longer reports errors synchronously.
    let local_names: Vec<String> = graph.branches.iter().map(|b| b.name.clone()).collect();

    let mut keys: Vec<String> = Vec::new();
    if !graph.worktrees.is_empty() {
        keys.push(SECTION_WORKTREES.into());
    }
    keys.push(SECTION_BRANCHES.into());
    if !graph.remotes.is_empty() {
        keys.push(SECTION_REMOTES.into());
    }
    if !graph.tags.is_empty() {
        keys.push(SECTION_TAGS.into());
    }
    if !graph.stashes.is_empty() {
        keys.push(SECTION_STASHES.into());
    }

    let mut accordion = Accordion::new("sc-branch-sections")
        .multiple(true)
        .bordered(false)
        .with_size(Size::XSmall)
        .on_toggle_click({
            let keys = keys.clone();
            move |open_ixs, _, cx| {
                let _ = Bridge::git_graph_set_open_sections(open_ixs, &keys, cx);
            }
        });

    if !graph.worktrees.is_empty() {
        let open = graph.section_open(SECTION_WORKTREES);
        let title = section_title(
            "Worktrees",
            PhosphorIcon::FolderSimple,
            Some(graph.worktrees.len()),
            theme,
        );
        let body = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .children(graph.worktrees.iter().map(|w| {
                let name = Path::new(&w.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| w.path.clone());
                let branch_label = w.branch.as_deref().unwrap_or("detached");
                tree_row(
                    PhosphorIcon::Folder,
                    name,
                    Some(branch_label.to_string()),
                    false,
                    theme,
                    |_, _, _| {},
                )
            }));
        accordion = accordion.item(move |item| {
            item.title(title)
                .show_chevron(true)
                .open(open)
                .bg(theme.transparent)
                .title_style(StyleRefinement::default().px(px(8.)).py(px(4.)))
                .content_style(StyleRefinement::default().px(px(8.)).pb(px(6.)))
                .child(body)
        });
    }

    {
        let open = graph.section_open(SECTION_BRANCHES);
        let title = section_title(
            "Branches",
            PhosphorIcon::GitBranch,
            Some(graph.branches.len()),
            theme,
        );
        let body = if graph.branches.is_empty() {
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("No local branches.")
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .children(graph.branches.iter().map(|b| {
                    let is_current = b.is_current;
                    let branch_name = b.name.clone();
                    let ahead_behind = if b.ahead > 0 || b.behind > 0 {
                        let mut parts = Vec::new();
                        if b.ahead > 0 {
                            parts.push(format!("↑{}", b.ahead));
                        }
                        if b.behind > 0 {
                            parts.push(format!("↓{}", b.behind));
                        }
                        Some(parts.join(" "))
                    } else {
                        None
                    };
                    let icon = if is_current {
                        PhosphorIcon::Check
                    } else {
                        PhosphorIcon::GitBranch
                    };
                    let menu_ctx = BranchMenuCtx {
                        branch: b.name.clone(),
                        is_current,
                        merged: b.merged,
                        upstream: b.upstream.clone(),
                        ahead: b.ahead,
                        behind: b.behind,
                        current_branch: current_branch.clone(),
                        busy,
                    };
                    // The id is the branch name, not the row index: checkout and
                    // rename reorder the list, and a menu left open across that
                    // reorder must not migrate to another row's state.
                    div()
                        .id(SharedString::from(format!("branch-item-{}", b.name)))
                        .context_menu(move |menu, window, cx| {
                            branch_menu(menu, window, cx, &menu_ctx)
                        })
                        .child(tree_row(
                            icon,
                            b.name.clone(),
                            ahead_behind,
                            is_current,
                            theme,
                            move |_, _, cx| {
                                if is_current {
                                    return;
                                }
                                let _ = Bridge::git_graph_checkout(&branch_name, cx);
                            },
                        ))
                }))
                .into_any_element()
        };
        accordion = accordion.item(move |item| {
            item.title(title)
                .show_chevron(true)
                .open(open)
                .bg(theme.transparent)
                .title_style(StyleRefinement::default().px(px(8.)).py(px(4.)))
                .content_style(StyleRefinement::default().px(px(8.)).pb(px(6.)))
                .child(body)
        });
    }

    if !graph.remotes.is_empty() {
        let open = graph.section_open(SECTION_REMOTES);
        let title = section_title(
            "Remotes",
            PhosphorIcon::Cloud,
            Some(graph.remotes.len()),
            theme,
        );
        // Inter-remote gap is ~2× intra-branch gap so remotes read as groups.
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .children(graph.remotes.iter().map(|r| {
                let remote_name = r.name.clone();
                let local_names = local_names.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.muted_foreground)
                            .child(remote_name.clone()),
                    )
                    .children(r.branches.iter().map(move |rb| {
                        let short = short_remote_branch(&remote_name, rb);
                        let can_switch = !short.eq_ignore_ascii_case("HEAD");
                        // Prefer the local branch; a remote-only branch is checked
                        // out by its full ref so git's DWIM creates the tracking
                        // branch. Resolved here because checkout no longer returns a
                        // synchronous error to fall back on.
                        let switch_to = if !can_switch {
                            String::new()
                        } else if local_names.iter().any(|local| local == &short) {
                            short.clone()
                        } else {
                            rb.clone()
                        };
                        div().pl_3().child(tree_row(
                            PhosphorIcon::GitBranch,
                            short,
                            None,
                            false,
                            theme,
                            move |_, _, cx| {
                                if switch_to.is_empty() {
                                    return;
                                }
                                let _ = Bridge::git_graph_checkout(&switch_to, cx);
                            },
                        ))
                    }))
            }));
        accordion = accordion.item(move |item| {
            item.title(title)
                .show_chevron(true)
                .open(open)
                .bg(theme.transparent)
                .title_style(StyleRefinement::default().px(px(8.)).py(px(4.)))
                .content_style(StyleRefinement::default().px(px(8.)).pb(px(6.)))
                .child(body)
        });
    }

    if !graph.tags.is_empty() {
        let open = graph.section_open(SECTION_TAGS);
        let title = section_title("Tags", PhosphorIcon::Tag, Some(graph.tags.len()), theme);
        let body = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .children(graph.tags.iter().take(20).map(|t| {
                tree_row(
                    PhosphorIcon::Tag,
                    t.clone(),
                    None,
                    false,
                    theme,
                    |_, _, _| {},
                )
            }));
        accordion = accordion.item(move |item| {
            item.title(title)
                .show_chevron(true)
                .open(open)
                .bg(theme.transparent)
                .title_style(StyleRefinement::default().px(px(8.)).py(px(4.)))
                .content_style(StyleRefinement::default().px(px(8.)).pb(px(6.)))
                .child(body)
        });
    }

    if !graph.stashes.is_empty() {
        let open = graph.section_open(SECTION_STASHES);
        let title = section_title(
            "Stashes",
            PhosphorIcon::Archive,
            Some(graph.stashes.len()),
            theme,
        );
        let body = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .children(graph.stashes.iter().map(|s| {
                tree_row(
                    PhosphorIcon::Archive,
                    format!("stash@{}: {}", s.index, s.message),
                    None,
                    false,
                    theme,
                    |_, _, _| {},
                )
            }));
        accordion = accordion.item(move |item| {
            item.title(title)
                .show_chevron(true)
                .open(open)
                .bg(theme.transparent)
                .title_style(StyleRefinement::default().px(px(8.)).py(px(4.)))
                .content_style(StyleRefinement::default().px(px(8.)).pb(px(6.)))
                .child(body)
        });
    }

    accordion
}

fn section_title(
    label: &str,
    icon: PhosphorIcon,
    count: Option<usize>,
    theme: &celestia_ui::theme::Theme,
) -> Div {
    let mut row = div()
        .flex()
        .items_center()
        .gap_1p5()
        .min_w_0()
        .child(
            Phosphor::new(icon)
                .size(px(12.))
                .color(theme.muted_foreground),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        );
    if let Some(n) = count {
        row = row.child(
            div()
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground.opacity(0.8))
                .child(format!("{n}")),
        );
    }
    row
}

fn pinned_row(
    id: &'static str,
    icon: PhosphorIcon,
    label: &'static str,
    count: String,
    emphasize: bool,
    theme: &celestia_ui::theme::Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_between()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .hover(|h| h.bg(theme.muted.opacity(0.3)))
        .on_mouse_down(MouseButton::Left, on_click)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(Phosphor::new(icon).size(px(14.)).color(if emphasize {
                    theme.warning
                } else {
                    theme.muted_foreground
                }))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(label),
                ),
        )
        .child(
            div()
                .px_1p5()
                .py(px(1.))
                .rounded(px(8.))
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .font_family(theme.mono_font_family.clone())
                .whitespace_nowrap()
                .when(emphasize, |d| {
                    d.bg(theme.warning.opacity(0.18)).text_color(theme.warning)
                })
                .when(!emphasize, |d| {
                    d.bg(theme.muted.opacity(0.2))
                        .text_color(theme.muted_foreground)
                })
                .child(count),
        )
}

fn tree_row(
    icon: PhosphorIcon,
    label: impl Into<SharedString>,
    trailing: Option<String>,
    selected: bool,
    theme: &celestia_ui::theme::Theme,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .when(selected, |d| {
            d.bg(theme.primary.opacity(0.16))
                .text_color(theme.primary)
                .font_weight(FontWeight::NORMAL)
        })
        .when(!selected, |d| d.hover(|h| h.bg(theme.muted.opacity(0.25))))
        .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
            cx.stop_propagation();
            on_click(ev, window, cx);
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .min_w_0()
                .flex_1()
                .child(Phosphor::new(icon).size(px(13.)).color(if selected {
                    theme.primary
                } else {
                    theme.muted_foreground
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .truncate()
                        .child(label.into()),
                ),
        )
        .when_some(trailing, |d, meta| {
            d.child(
                div()
                    .flex_none()
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child(meta),
            )
        })
}

fn short_remote_branch(remote: &str, full: &str) -> String {
    let prefix = format!("{remote}/");
    full.strip_prefix(&prefix).unwrap_or(full).to_string()
}

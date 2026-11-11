//! Git Graph & Branch Explorer module.
//!
//! Integrates:
//! 1. Left Branch Explorer sidebar (Worktrees, Branches, Remotes, Tags, Stashes, Filter)
//!    with a project create/switch footer.
//! 2. Interactive DAG commit graph with topological tracks.
//! 3. Commit details and diff inspector.

pub mod branch_explorer;
pub mod branch_menu;
pub mod commit_graph;
pub mod commit_inspector;
pub mod git_output;
pub mod repo_tabs;

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::Theme;
use gpui::*;
use halo_bridge::BridgeState;

use crate::pages::chrome::radius_xs;
use crate::pages::editors::ToolEditors;

/// Soft tinted chip — same shape as HTTP method/status tags in history.
pub(super) fn soft_chip(
    label: impl Into<SharedString>,
    color: Hsla,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .px_1p5()
        .py(px(1.))
        .rounded(radius_xs(theme))
        .bg(color.opacity(0.18))
        .font_weight(FontWeight::NORMAL)
        .text_xs()
        .text_color(color)
        .whitespace_nowrap()
        .child(label.into())
}

const SIDEBAR_WIDTH: Pixels = px(280.);
const SIDEBAR_MIN: Pixels = px(220.);
const SIDEBAR_MAX: Pixels = px(440.);

const GRAPH_HEIGHT: Pixels = px(360.);
const GRAPH_MIN: Pixels = px(180.);

pub fn render_graph_workspace(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &mut App,
) -> impl IntoElement {
    div()
        .id("source-control-graph-workspace")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        // Body: Horizontal split (Left sidebar + Right graph & inspector)
        .child(
            div().flex_1().w_full().min_h_0().child(
                ResizablePanelGroup::horizontal("sc-graph-h-split")
                    .with_handle(true)
                    .child(
                        resizable_panel()
                            .size(SIDEBAR_WIDTH)
                            .size_range(SIDEBAR_MIN..SIDEBAR_MAX)
                            .child(div().size_full().overflow_hidden().child(
                                branch_explorer::render_branch_explorer(state, editors, cx),
                            )),
                    )
                    .child(
                        resizable_panel().child(
                            div().size_full().min_w_0().overflow_hidden().child(
                                ResizablePanelGroup::vertical("sc-graph-v-split")
                                    .with_handle(true)
                                    .child(
                                        resizable_panel()
                                            .size(GRAPH_HEIGHT)
                                            .min_size(GRAPH_MIN)
                                            .child(div().size_full().overflow_hidden().child(
                                                commit_graph::render_commit_graph(state, cx),
                                            )),
                                    )
                                    .child(resizable_panel().child(
                                        div().size_full().overflow_hidden().child(
                                            commit_inspector::render_commit_inspector(state, cx),
                                        ),
                                    ))
                                    // Git Output dock — same shell as Coder's
                                    // terminal: hidden panels draw no body and
                                    // no handle, so the split closes cleanly.
                                    .child(
                                        resizable_panel()
                                            .size(git_output::OUTPUT_HEIGHT)
                                            .size_range(
                                                git_output::OUTPUT_MIN..git_output::OUTPUT_MAX,
                                            )
                                            .visible(state.git_output_open)
                                            .child(
                                                div().size_full().overflow_hidden().child(
                                                    git_output::render_git_output(state, cx),
                                                ),
                                            ),
                                    ),
                            ),
                        ),
                    ),
            ),
        )
}

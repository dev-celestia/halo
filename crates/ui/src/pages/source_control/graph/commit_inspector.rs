//! Commit detail and diff inspector component.
//!
//! Renders commit summary header, inspector tabs (Changes vs Commit Details),
//! changed files list with porcelain badges (M/A/D), and embedded diff viewer.

use std::path::Path;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::components::traits::Size;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, GitInspectorTab};

use crate::pages::source_control::{author_avatar, diff_view};

const FILES_LIST_WIDTH: Pixels = px(260.);
const FILES_LIST_MIN: Pixels = px(200.);
const FILES_LIST_MAX: Pixels = px(420.);

pub fn render_commit_inspector(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let graph = &state.git_graph;
    let selected_sha = graph.selected_commit.as_deref();

    let commit = selected_sha.and_then(|sha| graph.commits.iter().find(|c| c.sha == sha));

    let Some(commit) = commit else {
        return div()
            .id("sc-commit-inspector-empty")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.border)
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("Select a commit in the graph to inspect changes.")
            .into_any_element();
    };

    let tab = graph.selected_inspector_tab;
    let files_count = graph.commit_files.len();

    div()
        .id("sc-commit-inspector")
        .size_full()
        .flex()
        .flex_col()
        .bg(theme.background)
        .border_t_1()
        .border_color(theme.border)
        // Inspector top bar: commit summary + inspector tabs
        .child(
            div()
                .h(px(36.))
                .px_3()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.15))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .child(author_avatar(
                            commit.author_name.clone(),
                            &commit.author_email,
                            Size::Size(px(22.)),
                        ))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .child(commit.author_name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_family(theme.mono_font_family.clone())
                                .text_color(theme.muted_foreground)
                                .child(commit.short_sha.clone()),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_xs()
                                .text_color(theme.foreground)
                                .font_weight(FontWeight::NORMAL)
                                .truncate()
                                .child(commit.subject.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new("tab-inspector-changes")
                                .variant(if tab == GitInspectorTab::Changes {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                })
                                .size(ButtonSize::Small)
                                .label(format!("Changes ({files_count})"))
                                .on_click(|_, _, cx| {
                                    Bridge::git_graph_select_inspector_tab(
                                        GitInspectorTab::Changes,
                                        cx,
                                    );
                                }),
                        )
                        .child(
                            Button::new("tab-inspector-details")
                                .variant(if tab == GitInspectorTab::Details {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                })
                                .size(ButtonSize::Small)
                                .label("Commit Details")
                                .on_click(|_, _, cx| {
                                    Bridge::git_graph_select_inspector_tab(
                                        GitInspectorTab::Details,
                                        cx,
                                    );
                                }),
                        ),
                ),
        )
        // Inspector body
        .child(div().flex_1().min_h_0().overflow_hidden().child(match tab {
            GitInspectorTab::Changes => render_changes_tab(state, cx).into_any_element(),
            GitInspectorTab::Details => render_details_tab(commit, cx).into_any_element(),
        }))
        .into_any_element()
}

fn render_changes_tab(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let graph = &state.git_graph;
    let files = &graph.commit_files;
    let selected_file = graph.selected_commit_file.as_deref();
    let selected_sha = graph.selected_commit.clone();

    ResizablePanelGroup::horizontal("sc-commit-inspector-split")
        .with_handle(true)
        .child(
            resizable_panel()
                .size(FILES_LIST_WIDTH)
                .size_range(FILES_LIST_MIN..FILES_LIST_MAX)
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .bg(theme.background)
                        .child(
                            div()
                                .flex_1()
                                .overflow_y_scrollbar()
                                .p_1p5()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .when(files.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .p_3()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child("No modified files in this commit."),
                                    )
                                })
                                .children(files.iter().enumerate().map(|(idx, f)| {
                                    let is_active = selected_file == Some(f.path.as_str());
                                    let sha_c = selected_sha.clone();
                                    let path_c = f.path.clone();

                                    let file_name = Path::new(&f.path)
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| f.path.clone());

                                    let (badge_bg, badge_text) = match f.status.as_str() {
                                        "A" => (theme.success.opacity(0.18), theme.success),
                                        "D" => (theme.danger.opacity(0.18), theme.danger),
                                        _ => (theme.warning.opacity(0.18), theme.warning),
                                    };

                                    div()
                                        .id(("inspector-file", idx))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .px_2()
                                        .py_1()
                                        .rounded(theme.radius_sm())
                                        .cursor_pointer()
                                        .when(is_active, |d| {
                                            d.bg(theme.primary.opacity(0.16))
                                                .font_weight(FontWeight::NORMAL)
                                        })
                                        .when(!is_active, |d| {
                                            d.hover(|h| h.bg(theme.muted.opacity(0.3)))
                                        })
                                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                            if let Some(sha) = &sha_c {
                                                let _ = Bridge::git_graph_select_commit_file(
                                                    sha, &path_c, cx,
                                                );
                                            }
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1p5()
                                                .min_w_0()
                                                .child(
                                                    div()
                                                        .size(px(16.))
                                                        .rounded(px(3.))
                                                        .bg(badge_bg)
                                                        .text_color(badge_text)
                                                        .text_xs()
                                                        .font_weight(FontWeight::NORMAL)
                                                        .font_family(theme.mono_font_family.clone())
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(f.status.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .flex_1()
                                                        .text_sm()
                                                        .truncate()
                                                        .child(file_name),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .text_xs()
                                                .font_family(theme.mono_font_family.clone())
                                                .whitespace_nowrap()
                                                .when(f.additions > 0, |d| {
                                                    d.child(
                                                        div()
                                                            .text_color(theme.success)
                                                            .child(format!("+{}", f.additions)),
                                                    )
                                                })
                                                .when(f.deletions > 0, |d| {
                                                    d.child(
                                                        div()
                                                            .text_color(theme.danger)
                                                            .child(format!("-{}", f.deletions)),
                                                    )
                                                }),
                                        )
                                })),
                        ),
                ),
        )
        .child(
            resizable_panel().child(div().size_full().min_w_0().overflow_hidden().child(
                if let Some(diff) = &graph.commit_diff {
                    diff_view::diff_pane(diff, state.git_diff_split, state.git_diff_wrap, cx)
                        .into_any_element()
                } else {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Select a file to view its diff.")
                        .into_any_element()
                },
            )),
        )
}

fn render_details_tab(commit: &halo_bridge::GitGraphCommit, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let authored = super::commit_graph::format_commit_date(commit.timestamp);

    div()
        .size_full()
        .overflow_y_scrollbar()
        .p_4()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(commit.subject.clone()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(author_avatar(
                            commit.author_name.clone(),
                            &commit.author_email,
                            Size::Small,
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .min_w_0()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(theme.foreground)
                                        .child(commit.author_name.clone()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .truncate()
                                        .child(if commit.author_email.is_empty() {
                                            "Author".to_string()
                                        } else {
                                            commit.author_email.clone()
                                        }),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_x_6()
                .gap_y_3()
                .p_3()
                .rounded(theme.radius_sm())
                .bg(theme.muted.opacity(0.18))
                .border_1()
                .border_color(theme.border)
                .child(detail_field("Commit SHA", commit.sha.clone(), true, theme))
                .child(detail_field(
                    "Parents",
                    if commit.parents.is_empty() {
                        "None (root commit)".to_string()
                    } else {
                        commit.parents.join(", ")
                    },
                    true,
                    theme,
                ))
                .child(detail_field("Date", authored, true, theme)),
        )
}

fn detail_field(
    label: &str,
    value: String,
    mono: bool,
    theme: &celestia_ui::theme::Theme,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_xs()
                .when(mono, |d| d.font_family(theme.mono_font_family.clone()))
                .text_color(theme.foreground)
                .child(value),
        )
}

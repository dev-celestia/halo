//! Top-level Source Control tool page.

use celestia_ui::components::composite::toolbar_menu::{ToolbarMenu, ToolbarMenuItem};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::traits::{Sizable as _, Size};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, SourceControlPane};

use super::pulls::render_pulls;
use super::render_review;
use crate::pages::coder::project_display_name;
use crate::pages::editors::ToolEditors;

pub fn render_source_control(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &mut App,
) -> impl IntoElement {
    let has_root = state.coder.root_path.is_some();
    let pane = state.source_control_pane;

    div()
        .id("source-control-page")
        .size_full()
        .min_h(px(480.))
        .flex()
        .flex_row()
        .overflow_hidden()
        .child(activity_rail(pane))
        .child(
            div()
                .flex_1()
                .h_full()
                .min_w_0()
                .overflow_hidden()
                .child(match pane {
                    SourceControlPane::Changes if has_root => {
                        render_review(state, editors, cx).into_any_element()
                    }
                    SourceControlPane::Changes => empty_state(state, cx).into_any_element(),
                    SourceControlPane::Graph => {
                        super::render_graph_workspace(state, editors, cx).into_any_element()
                    }
                    SourceControlPane::PullRequests => {
                        render_pulls(state, editors, cx).into_any_element()
                    }
                }),
        )
}

fn activity_rail(pane: SourceControlPane) -> impl IntoElement {
    ToolbarMenu::new("source-control-activity")
        .icon_only()
        .vertical()
        .with_size(Size::Large)
        .h_full()
        .rounded_none()
        .border_0()
        .border_r_1()
        .px_1()
        .py_2()
        .item(
            ToolbarMenuItem::new("sc-rail-changes", PhosphorIcon::GitDiff)
                .label("Changes")
                .active(pane == SourceControlPane::Changes)
                .on_click(|_, _, cx| {
                    Bridge::select_source_control_pane(SourceControlPane::Changes, cx);
                }),
        )
        .item(
            ToolbarMenuItem::new("sc-rail-graph", PhosphorIcon::GitFork)
                .label("Graph & Branches")
                .active(pane == SourceControlPane::Graph)
                .on_click(|_, _, cx| {
                    Bridge::select_source_control_pane(SourceControlPane::Graph, cx);
                }),
        )
        .item(
            ToolbarMenuItem::new("sc-rail-pulls", PhosphorIcon::GitPullRequest)
                .label("Pull Requests")
                .active(pane == SourceControlPane::PullRequests)
                .on_click(|_, _, cx| {
                    Bridge::select_source_control_pane(SourceControlPane::PullRequests, cx);
                }),
        )
}

fn empty_state(state: &BridgeState, cx: &App) -> impl IntoElement {
    let recent = &state.coder_projects;
    let theme = cx.theme();

    let mut body = div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .px_6()
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.foreground)
                .child("Source Control"),
        )
        .child(
            div()
                .max_w(px(420.))
                .text_xs()
                .text_color(theme.muted_foreground)
                .text_center()
                .child("Add a project to review changes, commit, and sync."),
        )
        .child(
            Button::new("source-control-open-folder")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Medium)
                .leading_icon(PhosphorIcon::FolderOpen)
                .label("Add Project…")
                .on_click(|_, _, cx| {
                    let _ = Bridge::coder_open_folder(cx);
                    let _ =
                        Bridge::focus_or_open_work_tab(halo_bridge::WorkKind::SourceControl, cx);
                }),
        );

    if !recent.is_empty() {
        let mut list = div()
            .w_full()
            .max_w(px(420.))
            .flex()
            .flex_col()
            .gap_1()
            .pt_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.muted_foreground)
                    .child("Recent Projects"),
            );
        for (idx, path) in recent.iter().take(8).enumerate() {
            let label = project_display_name(path);
            let open_path = path.clone();
            let path_label = path.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("sc-recent-{idx}")))
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .px_2()
                    .py_1p5()
                    .rounded(theme.radius_sm())
                    .border_1()
                    .border_color(theme.border.opacity(0.6))
                    .cursor_pointer()
                    .hover(|s| {
                        s.bg(theme.accent.opacity(0.12))
                            .border_color(theme.accent.opacity(0.35))
                    })
                    .on_click(move |_, _, cx| {
                        let _ = Bridge::coder_open_project(&open_path, cx);
                    })
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.foreground)
                            .child(label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(path_label),
                    ),
            );
        }
        body = body.child(list);
    }

    body
}

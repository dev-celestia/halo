//! Project footer for Source Control Graph — switch / add repository tabs.
//!
//! Lives at the bottom of the Branch Explorer (create + filter), not as a
//! top workspace toolbar.

use std::path::Path;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use halo_bridge::{Bridge, BridgeState};

use super::soft_chip;
use crate::pages::coder::project_display_name;

pub fn render_project_footer(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let graph = &state.git_graph;
    let tabs = &graph.repo_tabs;
    let active_idx = graph.active_repo_tab;

    let current_branch = graph
        .branches
        .iter()
        .find(|b| b.is_current)
        .map(|b| b.name.as_str());

    let recent = state.coder_projects.clone();
    let open_tabs = tabs.clone();

    div()
        .id("sc-repo-project-footer")
        .w_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .py_1p5()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.tab_bar)
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .overflow_x_hidden()
                .children(tabs.iter().enumerate().map(|(idx, path)| {
                    let is_active = idx == active_idx;
                    let name = Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| project_display_name(path));

                    let branch_name = if is_active {
                        current_branch.unwrap_or("HEAD")
                    } else {
                        "repo"
                    };

                    let can_close = tabs.len() > 1;

                    div()
                        .id(("repo-tab", idx))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .h(px(28.))
                        .px_2()
                        .min_w_0()
                        .rounded(theme.radius_sm())
                        .cursor_pointer()
                        .when(is_active, |d| {
                            d.bg(theme.background)
                                .text_color(theme.foreground)
                                .border_1()
                                .border_color(theme.border)
                        })
                        .when(!is_active, |d| {
                            d.text_color(theme.muted_foreground).hover(|h| {
                                h.bg(theme.muted.opacity(0.3)).text_color(theme.foreground)
                            })
                        })
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            Bridge::git_graph_select_repo_tab(idx, cx);
                        })
                        .child(
                            div()
                                .min_w_0()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .truncate()
                                .child(name),
                        )
                        .when(is_active, |d| {
                            d.child(soft_chip(
                                branch_name.to_string(),
                                theme.primary,
                                theme,
                            ))
                        })
                        .when(can_close, |d| {
                            d.child(
                                div()
                                    .id(("repo-tab-close", idx))
                                    .size(px(14.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(3.))
                                    .hover(|h| {
                                        h.bg(theme.muted.opacity(0.4)).text_color(theme.foreground)
                                    })
                                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                        cx.stop_propagation();
                                        Bridge::git_graph_close_repo_tab(idx, cx);
                                    })
                                    .child(
                                        Phosphor::new(PhosphorIcon::X)
                                            .size(px(10.))
                                            .color(theme.muted_foreground),
                                    ),
                            )
                        })
                }))
                .child(
                    Button::new("sc-repo-tab-add")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .leading_icon(PhosphorIcon::Plus)
                        .tooltip("Open or add repository")
                        .dropdown_menu({
                            let recent = recent.clone();
                            let open_tabs = open_tabs.clone();
                            move |menu, window, cx| {
                                build_new_tab_menu(menu, window, cx, &recent, &open_tabs)
                            }
                        }),
                )
                .child(div().flex_1().min_w_0())
                .child(
                    Button::new("sc-graph-output-btn")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .leading_icon(PhosphorIcon::TerminalWindow)
                        .tooltip(if state.git_output_open {
                            "Hide Git Output"
                        } else {
                            "Show Git Output"
                        })
                        .on_click(|_, _, cx| {
                            Bridge::git_output_toggle(cx);
                        }),
                )
                .child(soft_chip(
                    format!("{}", state.git_output.len()),
                    if state.git_output.is_empty() {
                        theme.muted_foreground
                    } else {
                        theme.danger
                    },
                    theme,
                )),
        )
}

fn build_new_tab_menu(
    menu: PopupMenu,
    _window: &mut Window,
    _cx: &mut Context<PopupMenu>,
    recent: &[String],
    open_tabs: &[String],
) -> PopupMenu {
    let mut menu = menu.item(
        PopupMenuItem::new("Open Repository Folder…")
            .icon(PhosphorIcon::FolderPlus)
            .on_click(|_, _, cx| {
                let _ = Bridge::coder_open_folder(cx);
            }),
    );

    let candidates: Vec<&String> = recent.iter().filter(|p| !open_tabs.contains(p)).collect();

    if !candidates.is_empty() {
        menu = menu.separator();
        for path in candidates.into_iter().take(8) {
            let p_clone = path.clone();
            let label = format!("Tab: {}", project_display_name(path));
            menu = menu.item(
                PopupMenuItem::new(label)
                    .icon(PhosphorIcon::GitFork)
                    .on_click(move |_, _, cx| {
                        let _ = Bridge::git_graph_open_repo_tab(&p_clone, cx);
                    }),
            );
        }
    }

    menu
}

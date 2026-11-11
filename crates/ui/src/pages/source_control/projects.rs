//! Project picker — add (open folder), switch, remove from recents, close.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_destructive;
use crate::pages::coder::project_display_name;

/// Toolbar dropdown for Source Control (and similar headers).
pub(crate) fn render_project_switcher(state: &BridgeState, _cx: &App) -> impl IntoElement {
    let current = state.coder.root_path.clone();
    let recent = state.coder_projects.clone();
    let label = current
        .as_deref()
        .map(project_display_name)
        .unwrap_or_else(|| "No project".into());
    let tooltip = current
        .clone()
        .unwrap_or_else(|| "Open or switch project".into());

    Button::new("sc-project-switcher")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::Folder)
        .label(label)
        .dropdown_caret(true)
        .tooltip(tooltip)
        .dropdown_menu({
            let recent = recent.clone();
            let current = current.clone();
            move |menu, window, cx| project_menu(menu, window, cx, &recent, current.as_deref())
        })
}

/// Status-bar chip that opens the same project menu.
pub(crate) fn render_project_chip(state: &BridgeState, cx: &App) -> impl IntoElement {
    let current = state.coder.root_path.clone();
    let recent = state.coder_projects.clone();
    let label = current
        .as_deref()
        .map(project_display_name)
        .unwrap_or_else(|| "No project".into());
    let tooltip = current
        .clone()
        .unwrap_or_else(|| "Open or switch project".into());

    div()
        .id("sc-status-project")
        .flex()
        .items_center()
        .rounded(cx.theme().radius_sm())
        .hover(|s| s.bg(cx.theme().accent.opacity(0.5)))
        .child(
            Button::new("sc-status-project-btn")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Folder)
                .label(label)
                .dropdown_caret(true)
                .tooltip(tooltip)
                .dropdown_menu({
                    let recent = recent.clone();
                    let current = current.clone();
                    move |menu, window, cx| {
                        project_menu(menu, window, cx, &recent, current.as_deref())
                    }
                }),
        )
}

fn project_menu(
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
    recent: &[String],
    current: Option<&str>,
) -> PopupMenu {
    let mut menu = menu.item(
        PopupMenuItem::new("Add Project…")
            .icon(PhosphorIcon::FolderPlus)
            .on_click(|_, _, cx| {
                let _ = Bridge::coder_open_folder(cx);
            }),
    );

    if current.is_some() {
        menu = menu.item(
            PopupMenuItem::new("Close Project")
                .icon(PhosphorIcon::X)
                .on_click(|_, _, cx| {
                    let _ = Bridge::coder_close_project(cx);
                }),
        );
    }

    if !recent.is_empty() {
        menu = menu.separator();
        for path in recent {
            let label = project_display_name(path);
            let is_current = current == Some(path.as_str());
            let switch_path = path.clone();
            menu = menu.item(PopupMenuItem::new(label).checked(is_current).on_click(
                move |_, _, cx| {
                    if !is_current {
                        let _ = Bridge::coder_open_project(&switch_path, cx);
                    }
                },
            ));
        }

        let remove_list: Vec<(String, String)> = recent
            .iter()
            .map(|p| (p.clone(), project_display_name(p)))
            .collect();
        menu = menu.separator().submenu(
            "Remove from Recents",
            window,
            cx,
            move |sub, _, _| {
                let mut sub = sub;
                for (path, name) in &remove_list {
                    let path = path.clone();
                    let name = name.clone();
                    sub = sub.item(
                        PopupMenuItem::new(name.clone())
                            .icon(PhosphorIcon::Trash)
                            .on_click(move |_, window, cx| {
                                let path = path.clone();
                                let name = name.clone();
                                confirm_destructive(
                                    window,
                                    cx,
                                    "Remove project?",
                                    format!(
                                        "\"{name}\" will be removed from recent projects. Files on disk are not deleted."
                                    ),
                                    "Remove",
                                    move |_, cx| {
                                        let _ = Bridge::coder_remove_recent_project(&path, cx);
                                    },
                                );
                            }),
                    );
                }
                sub
            },
        );
    }

    menu
}

//! File Explorer — sandboxed local workspace (Documents/Halo Files).
//!
//! Breadcrumbs, list/grid views, keyboard nav, import, rename, and
//! modal-confirmed delete. Wordlists and R2 Storage are separate apps.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, ExplorerViewMode, LocalItem};

use crate::components::confirm_delete;
use crate::pages::chrome::{PAGE_GAP, empty_state, format_bytes};
use crate::pages::editors::ToolEditors;
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};

pub fn render_file_explorer(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    div()
        .id("file-explorer-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_local(state, editors, cx))
}

// ── toolbar ────────────────────────────────────────────────────────────────

fn render_toolbar(
    breadcrumbs: Vec<(String, Vec<String>)>,
    is_at_root: bool,
    creating: bool,
    view_mode: ExplorerViewMode,
    loading: bool,
    editors: &ToolEditors,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();

    div()
        .w_full()
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.2))
        .child(
            Button::new("fe-local-up")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowLeft)
                .disabled(is_at_root || loading)
                .on_click(|_, _, cx| {
                    let _ = Bridge::file_explorer_navigate_up(cx);
                }),
        )
        .children(
            breadcrumbs
                .iter()
                .enumerate()
                .map(|(i, (label, segments))| {
                    let is_last = i + 1 == breadcrumbs.len();
                    let segments = segments.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(if i > 0 {
                            div()
                                .text_color(theme.muted_foreground)
                                .child(PhosphorIcon::CaretRight)
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        })
                        .child(if is_last {
                            div()
                                .max_w(px(160.))
                                .truncate()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.foreground)
                                .child(label.clone())
                                .into_any_element()
                        } else {
                            Button::new(SharedString::from(format!("fe-local-crumb-{i}")))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .label(label.clone())
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::file_explorer_navigate(segments.clone(), cx);
                                })
                                .into_any_element()
                        })
                        .into_any_element()
                }),
        )
        .child(div().flex_1())
        .children(if creating {
            vec![
                editors.fe_folder_name.clone().into_any_element(),
                Button::new("fe-folder-confirm")
                    .variant(ButtonVariant::Primary)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::Check)
                    .on_click({
                        let input = editors.fe_folder_name.clone();
                        move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            match Bridge::file_explorer_create_folder(&name, cx) {
                                Ok(()) => {
                                    input.update(cx, |s, cx| s.set_value("", window, cx));
                                }
                                Err(err) => {
                                    Bridge::state(cx).update(cx, |s| s.apply_err(err));
                                }
                            }
                        }
                    })
                    .into_any_element(),
                Button::new("fe-folder-cancel")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::X)
                    .on_click(|_, _, cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.file_explorer.local_creating = false;
                        });
                    })
                    .into_any_element(),
            ]
        } else {
            vec![
                Button::new("fe-local-new-folder")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::FolderPlus)
                    .label("New Folder")
                    .on_click(|_, _, cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.file_explorer.local_creating = true;
                        });
                    })
                    .into_any_element(),
            ]
        })
        .child(
            Button::new("fe-import")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::DownloadSimple)
                .label("Import")
                .on_click(|_, window, cx| {
                    let options = PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: true,
                        prompt: Some("Import files into the workspace".into()),
                    };
                    let dialog = cx.prompt_for_paths(options);
                    cx.spawn(async move |cx: &mut AsyncApp| {
                        if let Ok(Ok(Some(paths))) = dialog.await {
                            let _ = cx.update(|cx| Bridge::file_explorer_import(paths, cx));
                        }
                        let _ = window;
                    })
                    .detach();
                })
                .into_any_element(),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .rounded(theme.radius_sm())
                .border_1()
                .border_color(theme.border)
                .p(px(1.))
                .child(view_button(
                    "fe-view-list",
                    "List",
                    PhosphorIcon::List,
                    ExplorerViewMode::List,
                    view_mode,
                ))
                .child(view_button(
                    "fe-view-grid",
                    "Grid",
                    PhosphorIcon::SquaresFour,
                    ExplorerViewMode::Grid,
                    view_mode,
                )),
        )
        .child(div().w(px(160.)).child(editors.fe_search.clone()))
        .child(
            Button::new("fe-local-refresh")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .on_click(|_, _, cx| {
                    let _ = Bridge::file_explorer_refresh(cx);
                }),
        )
        .into_any_element()
}

fn view_button(
    id: &'static str,
    label: &'static str,
    icon: PhosphorIcon,
    mode: ExplorerViewMode,
    current: ExplorerViewMode,
) -> Button {
    let active = mode == current;
    Button::new(id)
        .variant(if active {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        })
        .size(ButtonSize::Small)
        .leading_icon(icon)
        .label(label)
        .on_click(move |_, _, cx| Bridge::file_explorer_set_view_mode(mode, cx))
}

// ── local workspace ────────────────────────────────────────────────────────

fn render_local(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let fe = &state.file_explorer;
    let breadcrumbs = fe.local_breadcrumbs();
    let is_at_root = fe.local_segments.is_empty();

    div()
        .w_full()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .child(render_toolbar(
            breadcrumbs,
            is_at_root,
            fe.local_creating,
            fe.view_mode,
            fe.local_loading,
            editors,
            cx,
        ))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(render_local_items(state, cx)),
        )
}

fn render_local_items(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let fe = &state.file_explorer;
    let items = fe.local_filtered();
    let selected = fe.local_selected.clone();

    if fe.local_loading {
        return div()
            .w_full()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .flex_col()
            .gap_2()
            .text_color(theme.muted_foreground)
            .child("Loading files…")
            .into_any_element();
    }
    if items.is_empty() {
        return div()
            .w_full()
            .h_full()
            .child(empty_state(
                "Empty folder",
                "Import files or create a folder to get started.",
                cx,
            ))
            .into_any_element();
    }

    let list = div()
        .id("fe-items")
        .w_full()
        .h_full()
        .overflow_y_scrollbar()
        .on_key_down(|event: &KeyDownEvent, _, cx| {
            let keystroke = &event.keystroke;
            if !keystroke.modifiers.alt
                && !keystroke.modifiers.control
                && !keystroke.modifiers.shift
                && !keystroke.modifiers.platform
            {
                let current = Bridge::state(cx)
                    .read(cx)
                    .file_explorer
                    .local_selected
                    .clone();
                let items = Bridge::state(cx).read(cx).file_explorer.local_filtered();
                match keystroke.key.as_str() {
                    "down" => move_selection(&items, current, 1, cx),
                    "up" => move_selection(&items, current, -1, cx),
                    "enter" => {
                        if let Some(sel) = &current {
                            let path = sel.clone();
                            open_item(&path, cx);
                        }
                    }
                    "escape" => Bridge::file_explorer_select(None, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }
        })
        .flex()
        .when(fe.view_mode == ExplorerViewMode::Grid, |this| {
            this.flex_wrap().p_2()
        })
        .when(fe.view_mode == ExplorerViewMode::List, |this| {
            this.flex_col()
        })
        .children(match fe.view_mode {
            ExplorerViewMode::Grid => items
                .iter()
                .map(|item| grid_card(item, selected.as_deref(), cx))
                .collect::<Vec<_>>(),
            ExplorerViewMode::List => {
                let mut rows = vec![list_header(cx).into_any_element()];
                rows.extend(
                    items
                        .iter()
                        .map(|item| list_row(item, selected.as_deref(), cx)),
                );
                rows
            }
        });

    list.into_any_element()
}

fn move_selection(items: &[LocalItem], current: Option<String>, delta: isize, cx: &mut App) {
    let Some(index) = items
        .iter()
        .position(|item| Some(item.path.clone()) == current)
    else {
        if let Some(first) = items.first() {
            Bridge::file_explorer_select(Some(first.path.clone()), cx);
        }
        return;
    };
    let next = (index as isize + delta).clamp(0, items.len() as isize - 1) as usize;
    if let Some(item) = items.get(next) {
        Bridge::file_explorer_select(Some(item.path.clone()), cx);
    }
}

fn open_item(path: &str, cx: &mut App) {
    let std_path = std::path::Path::new(path);
    let is_dir = std_path.is_dir();
    if is_dir {
        // Navigate: convert the absolute path back into sandbox segments.
        let root = Bridge::state(cx).read(cx).file_explorer.root_dir.clone();
        if let Some(root) = root {
            if let Ok(relative) = std_path.strip_prefix(&root) {
                let segments: Vec<String> = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                let _ = Bridge::file_explorer_navigate(segments, cx);
                return;
            }
        }
        let _ = Bridge::file_explorer_refresh(cx);
    } else {
        Bridge::file_explorer_open(path, cx);
    }
}

fn item_icon(item: &LocalItem) -> PhosphorIcon {
    if item.is_dir {
        PhosphorIcon::Folder
    } else {
        PhosphorIcon::FileText
    }
}

fn grid_card(item: &LocalItem, selected: Option<&str>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(item.path.as_str());
    let path = item.path.clone();
    let path_for_open = path.clone();
    let path_for_delete = path.clone();
    let item_name = item.name.clone();
    let item_is_dir = item.is_dir;

    div()
        .id(SharedString::from(format!("fe-card-{}", item.path)))
        .w(px(104.))
        .h(px(96.))
        .p_2()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(if is_selected {
            theme.primary.opacity(0.4)
        } else {
            theme.border
        })
        .bg(if is_selected {
            theme.primary.opacity(0.1)
        } else {
            theme.background
        })
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted.opacity(0.5)))
        .active(|s| s.opacity(0.9))
        .on_click({
            let path = path.clone();
            move |_, _, cx| Bridge::file_explorer_select(Some(path.clone()), cx)
        })
        .on_mouse_down(MouseButton::Left, {
            let path_for_open = path_for_open.clone();
            move |event: &MouseDownEvent, _, cx| {
                if event.click_count == 2 {
                    open_item(&path_for_open, cx);
                }
            }
        })
        .on_mouse_down(MouseButton::Right, {
            let path = path.clone();
            move |_, _, cx| Bridge::file_explorer_select(Some(path.clone()), cx)
        })
        .context_menu(move |mut menu, _, _| {
            let open_path = path_for_open.clone();
            let reveal_path = path_for_open.clone();
            let delete_path = path_for_delete.clone();
            let delete_name = item_name.clone();
            menu = menu.item(PopupMenuItem::new("Open").on_click(move |_, _, cx| {
                open_item(&open_path, cx);
            }));
            menu = menu.item(
                PopupMenuItem::new("Reveal in Finder").on_click(move |_, _, cx| {
                    Bridge::file_explorer_reveal(&reveal_path, cx);
                }),
            );
            menu = menu.separator();
            menu = menu.item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                confirm_delete(
                    window,
                    cx,
                    delete_title(item_is_dir),
                    delete_description(&delete_name, item_is_dir),
                    {
                        let path = delete_path.clone();
                        move |_, cx| {
                            let _ = Bridge::file_explorer_delete(&path, cx);
                        }
                    },
                );
            }));
            menu
        })
        .child(
            div()
                .text_color(if item.is_dir {
                    hsla(38. / 360., 0.9, 0.5, 1.)
                } else {
                    theme.muted_foreground
                })
                .child(item_icon(item)),
        )
        .child(
            div()
                .max_w(px(88.))
                .truncate()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .child(item.name.clone()),
        )
        .into_any_element()
}

fn list_header(cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(theme.muted_foreground)
        .bg(theme.muted.opacity(0.5))
        .child(div().w(px(220.)).child("NAME"))
        .child(div().w(px(60.)).child("TYPE"))
        .child(div().w(px(90.)).child("SIZE"))
        .child(div().w(px(130.)).child("MODIFIED"))
        .child(div().flex_1())
        .into_any_element()
}

fn list_row(item: &LocalItem, selected: Option<&str>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(item.path.as_str());
    let path = item.path.clone();
    let path_for_open = path.clone();
    let path_for_delete = path.clone();
    let item_name = item.name.clone();
    let item_name_for_menu = item_name.clone();
    let item_is_dir = item.is_dir;

    div()
        .id(SharedString::from(format!("fe-row-{}", item.path)))
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .font_family("Menlo")
        .text_xs()
        .border_b_1()
        .border_color(theme.border.opacity(0.4))
        .bg(if is_selected {
            theme.primary.opacity(0.1)
        } else {
            theme.background
        })
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted.opacity(0.3)))
        .on_click({
            let path = path.clone();
            move |_, _, cx| Bridge::file_explorer_select(Some(path.clone()), cx)
        })
        .on_mouse_down(MouseButton::Left, {
            let path_for_open = path_for_open.clone();
            move |event: &MouseDownEvent, _, cx| {
                if event.click_count == 2 {
                    open_item(&path_for_open, cx);
                }
            }
        })
        .on_mouse_down(MouseButton::Right, {
            let path = path.clone();
            move |_, _, cx| Bridge::file_explorer_select(Some(path.clone()), cx)
        })
        .context_menu(move |mut menu, _, _| {
            let open_path = path_for_open.clone();
            let delete_path = path_for_delete.clone();
            let delete_name = item_name_for_menu.clone();
            menu = menu.item(PopupMenuItem::new("Open").on_click(move |_, _, cx| {
                open_item(&open_path, cx);
            }));
            menu = menu.separator();
            menu = menu.item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                confirm_delete(
                    window,
                    cx,
                    delete_title(item_is_dir),
                    delete_description(&delete_name, item_is_dir),
                    {
                        let path = delete_path.clone();
                        move |_, cx| {
                            let _ = Bridge::file_explorer_delete(&path, cx);
                        }
                    },
                );
            }));
            menu
        })
        .child(
            div()
                .w(px(220.))
                .min_w_0()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_color(if item.is_dir {
                            hsla(38. / 360., 0.9, 0.5, 1.)
                        } else {
                            theme.muted_foreground
                        })
                        .child(item_icon(item)),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(theme.foreground)
                        .child(item.name.clone()),
                ),
        )
        .child(
            div()
                .w(px(60.))
                .text_color(theme.muted_foreground)
                .child(if item.is_dir { "DIR" } else { "FILE" }),
        )
        .child(
            div()
                .w(px(90.))
                .text_color(theme.muted_foreground)
                .child(item.size.map(format_bytes).unwrap_or_else(|| "—".into())),
        )
        .child(
            div()
                .w(px(130.))
                .text_color(theme.muted_foreground)
                .child(item.modified.clone().unwrap_or_else(|| "—".into())),
        )
        .child(div().flex_1())
        .child(
            div().flex().items_center().gap_1().pr_1().child(
                Button::icon(
                    SharedString::from(format!("fe-del-{}", item.path)),
                    PhosphorIcon::Trash,
                )
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .accessibility_label("Delete")
                .tooltip(if item_is_dir {
                    "Delete folder"
                } else {
                    "Delete file"
                })
                .on_click({
                    let path = path.clone();
                    let name = item_name.clone();
                    move |_, window, cx| {
                        let path = path.clone();
                        confirm_delete(
                            window,
                            cx,
                            delete_title(item_is_dir),
                            delete_description(&name, item_is_dir),
                            move |_, cx| {
                                let _ = Bridge::file_explorer_delete(&path, cx);
                            },
                        );
                    }
                }),
            ),
        )
        .into_any_element()
}

fn delete_title(is_dir: bool) -> &'static str {
    if is_dir {
        "Delete folder?"
    } else {
        "Delete file?"
    }
}

fn delete_description(name: &str, is_dir: bool) -> String {
    if is_dir {
        format!(
            "\"{name}\" and everything inside it will be permanently deleted. This cannot be undone."
        )
    } else {
        format!("\"{name}\" will be permanently deleted. This cannot be undone.")
    }
}

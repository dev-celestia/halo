//! Explorer file tree and its context menus.

use std::path::Path;
use std::sync::Arc;

use celestia_ui::components::composite::file_tree::FileTree;
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::{Input, InputState};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_component::scroll::{Scrollbar, ScrollbarAxis};
use halo_bridge::{
    Bridge, BridgeState, CoderExplorerInline, CoderExplorerInlineKind, CoderTreeEntry,
};

use crate::components::confirm_delete;
use crate::pages::editors::ToolEditors;
use crate::pages::source_control::{
    GitDecor, GitDecorMap, decor_map, rel_to_repo, repo_root, status_color, status_dot,
};

type EditorEntity = Entity<celestia_ui::components::CodeEditor>;

/// Parent directory for toolbar New File / New Folder (active path or workspace root).
pub(super) fn explorer_create_parent(state: &BridgeState) -> String {
    let root = state.coder.root_path.clone().unwrap_or_default();
    let Some(active) = state.coder.focused_active_path() else {
        return root;
    };
    if Path::new(active).is_dir() {
        active.to_string()
    } else {
        Path::new(active)
            .parent()
            .map(|p| p.display().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or(root)
    }
}

fn focus_inline_name(input: &Entity<InputState>, initial: &str, window: &mut Window, cx: &mut App) {
    input.update(cx, |s, cx| {
        s.set_value(initial.to_string(), window, cx);
        s.focus(window, cx);
    });
}

/// Start inline new-file naming under `parent_dir`.
pub(super) fn start_new_file(
    parent_dir: &str,
    inline_input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) {
    if Bridge::coder_new_file(parent_dir, cx).is_ok() {
        focus_inline_name(inline_input, "", window, cx);
    }
}

/// Start inline new-folder naming under `parent_dir`.
pub(super) fn start_new_folder(
    parent_dir: &str,
    inline_input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) {
    if Bridge::coder_new_folder(parent_dir, cx).is_ok() {
        focus_inline_name(inline_input, "", window, cx);
    }
}

/// Start inline rename for `path`.
pub(super) fn start_rename(
    path: &str,
    inline_input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) {
    let basename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();
    if Bridge::coder_rename(path, cx).is_ok() {
        focus_inline_name(inline_input, &basename, window, cx);
    }
}

/// Explorer row metrics. The rows are drawn by the host (not the kit's
/// `FileTree` rows, which are a fixed 22px and read airy next to a 12–13px
/// mono label), so the pitch can follow the configured editor font size.
const ROW_PAD_X: f32 = 6.;
const ROW_GAP: f32 = 8.;
/// Caret column for folders / spacer column for files — keeps labels aligned.
const ROW_CARET: f32 = 18.;
const CARET_ICON: f32 = 12.;

/// Row pitch for one tree row: the label size plus a thin band of padding.
fn row_height(font_size: f32) -> f32 {
    (font_size + 12.).max(22.)
}

/// Depth-first index of `target` among currently visible explorer rows.
fn visible_row_index(
    entries: &[CoderTreeEntry],
    expanded: &[String],
    target: &str,
) -> Option<usize> {
    fn walk(
        entries: &[CoderTreeEntry],
        expanded: &[String],
        target: &str,
        index: &mut usize,
    ) -> bool {
        for entry in entries {
            if entry.path == target {
                return true;
            }
            *index += 1;
            if entry.is_dir
                && expanded.iter().any(|p| p == &entry.path)
                && let Some(children) = &entry.children
                && walk(children, expanded, target, index)
            {
                return true;
            }
        }
        false
    }
    let mut index = 0;
    walk(entries, expanded, target, &mut index).then_some(index)
}

pub(super) fn render_file_tree(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let expanded = &state.coder.expanded;
    let active = state.coder.focused_active_path();
    let editor_pool = editors.coder_editors.clone();
    let inline_input = editors.coder_explorer_inline_name.clone();
    let inline = state.coder.explorer_inline.clone();
    let root = state.coder.root_path.clone().unwrap_or_default();
    let repo_root = repo_root(state);
    let git_map = decor_map(state);

    // One step under the editor face so the tree reads lighter than the buffer.
    let font_size_px = (f32::from(state.coder_settings.font_size) - 1.).max(10.);
    let font_size = px(font_size_px);
    let row_h = px(row_height(font_size_px));

    let active_ix = active.and_then(|path| visible_row_index(&state.coder.tree, expanded, path));
    editors
        .coder_explorer_scroll
        .reveal_active(active, active_ix, row_h);

    // Match Coder Settings → Editor typography. `FileTree` defaults to the
    // theme mono face; Styled refinements win because they apply last in render.
    let font_family = halo_bridge::font_family(&state.coder_settings.font_id)
        .map(SharedString::from)
        .unwrap_or_else(|| cx.theme().mono_font_family.clone());

    // The explorer panel already frames the tree, so the component's own card
    // border + radius only drew a second box inside it, and its 8px body inset
    // kept the rows off the panel edge. `FileTree` refines its root style last,
    // so the border/radius overrides win over the defaults set in `render`.
    let mut tree = FileTree::new("coder-file-tree")
        .border_0()
        .rounded(px(0.))
        .padded(false)
        .font_family(font_family)
        .text_size(font_size);

    if let Some(inline) = &inline
        && inline.parent_dir == root
        && matches!(
            inline.kind,
            CoderExplorerInlineKind::CreateFile | CoderExplorerInlineKind::CreateFolder
        )
    {
        tree = tree.child(render_inline_create_row(
            inline,
            inline_input.clone(),
            row_h,
            font_size,
            cx,
        ));
    }

    for entry in &state.coder.tree {
        tree = tree.child(render_tree_entry(
            entry,
            expanded,
            active,
            editor_pool.clone(),
            inline_input.clone(),
            inline.as_ref(),
            repo_root.clone(),
            git_map.clone(),
            row_h,
            font_size,
            cx,
        ));
    }

    // Blank-area menu lives on a sibling filler, not the tree root — nesting
    // `.context_menu` on both parent and row opens two stacked menus.
    let root_for_menu = root.clone();
    let inline_for_menu = inline_input.clone();
    let scroll = editors.coder_explorer_scroll.clone();

    // Own the ScrollHandle (unlike overflow_y_scrollbar's keyed internal one)
    // so we can reveal the active row when it sits below the fold.
    div()
        .id("coder-tree-scroll")
        .size_full()
        .relative()
        .child(
            div()
                .id("coder-tree-area")
                .size_full()
                .track_scroll(&scroll.handle)
                .overflow_y_scroll()
                .child(
                    div()
                        .id("coder-tree-inner")
                        .w_full()
                        .min_h_full()
                        .flex()
                        .flex_col()
                        .child(tree)
                        .child(
                            div()
                                .id("coder-tree-blank")
                                .w_full()
                                .flex_1()
                                .min_h(px(48.))
                                .context_menu(move |menu, _, _| {
                                    explorer_blank_menu(menu, &root_for_menu, &inline_for_menu)
                                }),
                        ),
                ),
        )
        .child(
            div().absolute().inset_0().child(
                Scrollbar::new(&scroll.handle)
                    .id("coder-tree-scrollbar")
                    .axis(ScrollbarAxis::Vertical)
                    .viewport_from_layout(),
            ),
        )
}

fn explorer_blank_menu(
    menu: PopupMenu,
    root: &str,
    inline_input: &Entity<InputState>,
) -> PopupMenu {
    let root = root.to_string();
    let inline_input = inline_input.clone();
    menu.item(PopupMenuItem::new("Undo").on_click(|_, _, cx| {
        let _ = Bridge::coder_undo(cx);
    }))
    .item(PopupMenuItem::new("Redo").on_click(|_, _, cx| {
        let _ = Bridge::coder_redo(cx);
    }))
    .separator()
    .item({
        let root = root.clone();
        let inline_input = inline_input.clone();
        PopupMenuItem::new("New File").on_click(move |_, window, cx| {
            start_new_file(&root, &inline_input, window, cx);
        })
    })
    .item({
        let root = root.clone();
        let inline_input = inline_input.clone();
        PopupMenuItem::new("New Folder").on_click(move |_, window, cx| {
            start_new_folder(&root, &inline_input, window, cx);
        })
    })
    .separator()
    .item({
        let root = root.clone();
        PopupMenuItem::new("Copy Path").on_click(move |_, _, cx| {
            let _ = Bridge::coder_copy_path(&root, cx);
        })
    })
    .item({
        let root = root.clone();
        PopupMenuItem::new("Refresh").on_click(move |_, _, cx| {
            let _ = Bridge::coder_reload_dir(&root, cx);
        })
    })
}

fn format_history_label(saved_at_secs: u64, id: &str) -> String {
    // Prefer a readable UTC timestamp; fall back to the raw id.
    let secs = saved_at_secs as i64;
    let datetime = chrono::DateTime::from_timestamp(secs, 0);
    match datetime {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => id.to_string(),
    }
}

fn explorer_entry_menu(
    menu: PopupMenu,
    path: &str,
    is_dir: bool,
    editors: &[EditorEntity],
    inline_input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let path = path.to_string();
    let parent = Path::new(&path)
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| path.clone());
    let target_dir = if is_dir { path.clone() } else { parent.clone() };
    let editors = editors.to_vec();
    let inline_input = inline_input.clone();

    let mut menu = menu
        .item(PopupMenuItem::new("Undo").on_click(|_, _, cx| {
            let _ = Bridge::coder_undo(cx);
        }))
        .item(PopupMenuItem::new("Redo").on_click(|_, _, cx| {
            let _ = Bridge::coder_redo(cx);
        }))
        .separator();

    if !is_dir {
        menu = menu
            .item({
                let path = path.clone();
                let editors = editors.clone();
                PopupMenuItem::new("Open").on_click(move |_, window, cx| {
                    let _ = Bridge::coder_open_file(&path, &editors, window, cx);
                })
            })
            .item({
                let path = path.clone();
                let editors = editors.clone();
                PopupMenuItem::new("Open to the Side").on_click(move |_, window, cx| {
                    let _ = Bridge::coder_open_file_to_side(&path, &editors, window, cx);
                })
            });
        let history = Bridge::coder_list_local_history(&path, cx);
        if history.is_empty() {
            menu = menu.item(PopupMenuItem::new("Local History").disabled(true));
        } else {
            let path_hist = path.clone();
            let editors_hist = editors.clone();
            menu = menu.submenu("Local History", window, cx, move |sub, _, _| {
                let mut sub = sub;
                for entry in &history {
                    let label = format_history_label(entry.saved_at_secs, &entry.id);
                    let path = path_hist.clone();
                    let id = entry.id.clone();
                    let editors = editors_hist.clone();
                    sub = sub.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ =
                            Bridge::coder_restore_local_history(&path, &id, &editors, window, cx);
                    }));
                }
                sub
            });
        }
        menu = menu.separator();
    }

    menu.item({
        let target_dir = target_dir.clone();
        let inline_input = inline_input.clone();
        PopupMenuItem::new("New File").on_click(move |_, window, cx| {
            start_new_file(&target_dir, &inline_input, window, cx);
        })
    })
    .item({
        let target_dir = target_dir.clone();
        let inline_input = inline_input.clone();
        PopupMenuItem::new("New Folder").on_click(move |_, window, cx| {
            start_new_folder(&target_dir, &inline_input, window, cx);
        })
    })
    .separator()
    .item({
        let path = path.clone();
        PopupMenuItem::new("Copy").on_click(move |_, _, cx| {
            let _ = Bridge::coder_copy_name(&path, cx);
        })
    })
    .item({
        let path = path.clone();
        PopupMenuItem::new("Copy Path").on_click(move |_, _, cx| {
            let _ = Bridge::coder_copy_path(&path, cx);
        })
    })
    .item({
        let path = path.clone();
        PopupMenuItem::new("Copy Relative Path").on_click(move |_, _, cx| {
            let _ = Bridge::coder_copy_relative_path(&path, cx);
        })
    })
    .separator()
    .item({
        let path = path.clone();
        let inline_input = inline_input.clone();
        PopupMenuItem::new("Rename").on_click(move |_, window, cx| {
            start_rename(&path, &inline_input, window, cx);
        })
    })
    .item({
        let path = path.clone();
        PopupMenuItem::new("Duplicate").on_click(move |_, _, cx| {
            let _ = Bridge::coder_duplicate(&path, cx);
        })
    })
    .separator()
    .item({
        let path = path.clone();
        PopupMenuItem::element(|_, cx| div().text_color(cx.theme().danger).child("Delete"))
            .on_click(move |_, window, cx| {
                let path = path.clone();
                let title = if is_dir {
                    "Delete folder?"
                } else {
                    "Delete file?"
                };
                let body = if is_dir {
                    format!(
                        "Move \"{path}\" and its contents to trash? You can Undo from the explorer menu."
                    )
                } else {
                    format!(
                        "Move \"{path}\" to trash? You can Undo from the explorer menu."
                    )
                };
                confirm_delete(window, cx, title, body, move |_window, cx| {
                    let _ = Bridge::coder_delete(&path, cx);
                });
            })
    })
}

fn render_inline_name_field(
    input: Entity<InputState>,
    row_h: Pixels,
    font_size: Pixels,
) -> impl IntoElement {
    div()
        .id("coder-explorer-inline-name")
        .flex_1()
        .min_w_0()
        .h(row_h)
        .flex()
        .items_center()
        .text_size(font_size)
        .on_key_down(|event: &KeyDownEvent, _, cx| {
            if event.keystroke.key.as_str() == "escape" {
                let _ = Bridge::coder_explorer_inline_cancel(cx);
                cx.stop_propagation();
            }
        })
        .child(Input::new(&input).appearance(false).cleanable(false))
}

fn render_inline_create_row(
    inline: &CoderExplorerInline,
    input: Entity<InputState>,
    row_h: Pixels,
    font_size: Pixels,
    cx: &App,
) -> AnyElement {
    let is_dir = matches!(inline.kind, CoderExplorerInlineKind::CreateFolder);
    let theme = cx.theme();
    div()
        .id("coder-explorer-inline-create")
        .w_full()
        .h(row_h)
        .px(px(ROW_PAD_X))
        .rounded(px(2.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(ROW_GAP))
        .bg(theme.muted)
        .child(div().size(px(ROW_CARET)).flex_shrink_0())
        .child(if is_dir {
            crate::explorer_icons::folder_icon("", false).into_any_element()
        } else {
            crate::explorer_icons::file_icon("").into_any_element()
        })
        .child(render_inline_name_field(input, row_h, font_size))
        .into_any_element()
}

fn is_renaming(inline: Option<&CoderExplorerInline>, path: &str) -> bool {
    matches!(
        inline.map(|i| &i.kind),
        Some(CoderExplorerInlineKind::Rename { from }) if from == path
    )
}

fn is_creating_under(inline: Option<&CoderExplorerInline>, parent: &str) -> bool {
    inline.is_some_and(|i| {
        i.parent_dir == parent
            && matches!(
                i.kind,
                CoderExplorerInlineKind::CreateFile | CoderExplorerInlineKind::CreateFolder
            )
    })
}

fn render_tree_entry(
    entry: &CoderTreeEntry,
    expanded: &[String],
    active: Option<&str>,
    editors: Vec<EditorEntity>,
    inline_input: Entity<InputState>,
    inline: Option<&CoderExplorerInline>,
    repo_root: String,
    git_map: Arc<GitDecorMap>,
    row_h: Pixels,
    font_size: Pixels,
    cx: &App,
) -> AnyElement {
    let path = entry.path.clone();
    let name = entry.name.clone();
    let is_expanded = expanded.iter().any(|p| p == &path);
    let is_selected = active == Some(path.as_str());
    let renaming = is_renaming(inline, &path);
    let rel = rel_to_repo(&path, &repo_root).unwrap_or_default();
    let decor = if rel.is_empty() {
        GitDecor::default()
    } else {
        git_map.lookup(&rel)
    };
    // Ignored wins: no dirty tint / dot on excluded paths.
    let show_status = !decor.ignored && decor.status.is_some() && !renaming;
    let status = decor.status.unwrap_or("");
    let tint = if show_status {
        Some(status_color(status, cx))
    } else {
        None
    };

    if entry.is_dir {
        let menu_path = path.clone();
        let menu_editors = editors.clone();
        let menu_inline = inline_input.clone();
        let toggle_path = path.clone();
        let theme = cx.theme();
        let name_color = tint.unwrap_or(if decor.ignored {
            theme.muted_foreground
        } else {
            theme.foreground
        });
        let caret = if is_expanded {
            PhosphorIcon::CaretDown
        } else {
            PhosphorIcon::CaretRight
        };

        let name_child: AnyElement = if renaming {
            render_inline_name_field(inline_input.clone(), row_h, font_size).into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .text_size(font_size)
                .font_weight(FontWeight::THIN)
                .text_color(name_color)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(name.clone())
                .into_any_element()
        };

        let mut label = div()
            .id(ElementId::Name(format!("{path}-label").into()))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(ROW_GAP))
            .when(decor.ignored && !renaming, |d| d.opacity(0.5))
            .child(crate::explorer_icons::folder_icon(&name, is_expanded))
            .child(name_child);

        if !renaming {
            label = label.cursor_pointer().on_click({
                let toggle_path = toggle_path.clone();
                move |_, _, cx| {
                    let _ = Bridge::coder_toggle_dir(&toggle_path, cx);
                }
            });
        }

        let row = div()
            .id(ElementId::Name(format!("{path}-row").into()))
            .w_full()
            .h(row_h)
            .px(px(ROW_PAD_X))
            .rounded(px(2.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(ROW_GAP))
            .when(is_selected || renaming, |d| d.bg(theme.muted))
            .when(!is_selected && !renaming, |d| {
                d.hover(|s| s.bg(theme.muted.opacity(0.5)))
            })
            .child(
                div()
                    .id(ElementId::Name(format!("{path}-toggle").into()))
                    .size(px(ROW_CARET))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .flex_shrink_0()
                    .child(
                        Phosphor::new(caret)
                            .size(px(CARET_ICON))
                            .color(theme.muted_foreground),
                    )
                    .on_click({
                        let toggle_path = toggle_path.clone();
                        move |_, _, cx| {
                            let _ = Bridge::coder_toggle_dir(&toggle_path, cx);
                        }
                    }),
            )
            .child(label)
            .when(show_status, |d| {
                d.child(
                    div()
                        .id(ElementId::Name(format!("{path}-git").into()))
                        .ml_auto()
                        .flex()
                        .items_center()
                        .child(status_dot(status, cx)),
                )
            })
            .context_menu(move |menu, window, cx| {
                explorer_entry_menu(
                    menu,
                    &menu_path,
                    true,
                    &menu_editors,
                    &menu_inline,
                    window,
                    cx,
                )
            });

        let mut column = div()
            .id(ElementId::Name(path.clone().into()))
            .w_full()
            .flex()
            .flex_col()
            .child(row);

        if is_expanded {
            let creating_here = is_creating_under(inline, &path);
            let mut child_elements: Vec<AnyElement> = Vec::new();
            if creating_here && let Some(inline) = inline {
                child_elements.push(render_inline_create_row(
                    inline,
                    inline_input.clone(),
                    row_h,
                    font_size,
                    cx,
                ));
            }
            if let Some(children) = &entry.children {
                for child in children {
                    child_elements.push(render_tree_entry(
                        child,
                        expanded,
                        active,
                        editors.clone(),
                        inline_input.clone(),
                        inline,
                        repo_root.clone(),
                        git_map.clone(),
                        row_h,
                        font_size,
                        cx,
                    ));
                }
            }
            if !child_elements.is_empty() {
                // 8+4 ≈ 12px/level so the guide sits under the caret column.
                column = column.child(
                    div()
                        .id(ElementId::Name(format!("{path}-children").into()))
                        .ml(px(8.))
                        .pl(px(4.))
                        .border_l_1()
                        .border_color(theme.border)
                        .flex()
                        .flex_col()
                        .children(child_elements),
                );
            }
        }
        column.into_any_element()
    } else {
        let menu_path = path.clone();
        let menu_editors = editors.clone();
        let menu_inline = inline_input.clone();
        let theme = cx.theme();
        let name_color = tint.unwrap_or(if decor.ignored {
            theme.muted_foreground
        } else {
            theme.foreground
        });

        let name_child: AnyElement = if renaming {
            render_inline_name_field(inline_input, row_h, font_size).into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .text_size(font_size)
                .font_weight(FontWeight::THIN)
                .text_color(name_color)
                .when(decor.ignored && tint.is_none(), |d| d.opacity(0.85))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(name.clone())
                .into_any_element()
        };

        let mut row = div()
            .id(ElementId::Name(path.clone().into()))
            .w_full()
            .h(row_h)
            .px(px(ROW_PAD_X))
            .rounded(px(2.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(ROW_GAP))
            .when(is_selected || renaming, |d| d.bg(theme.muted))
            .when(!is_selected && !renaming, |d| {
                d.hover(|s| s.bg(theme.muted.opacity(0.5)))
            })
            // Spacer aligns with the folder caret column.
            .child(div().size(px(ROW_CARET)).flex_shrink_0())
            .child(
                div()
                    .when(decor.ignored && !renaming, |d| d.opacity(0.5))
                    .child(crate::explorer_icons::file_icon(&name)),
            )
            .child(name_child)
            .when(show_status, |d| {
                d.child(
                    div()
                        .id(ElementId::Name(format!("{path}-git").into()))
                        .ml_auto()
                        .flex()
                        .items_center()
                        .child(status_dot(status, cx)),
                )
            });

        if !renaming {
            row = row.cursor_pointer().on_click({
                let path = path.clone();
                let editors = editors.clone();
                move |_, window, cx| {
                    let _ = Bridge::coder_open_file(&path, &editors, window, cx);
                }
            });
        }

        row.context_menu(move |menu, window, cx| {
            explorer_entry_menu(
                menu,
                &menu_path,
                false,
                &menu_editors,
                &menu_inline,
                window,
                cx,
            )
        })
        .into_any_element()
    }
}

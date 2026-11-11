//! Left-rail collection switcher + folder/request tree (Coder explorer style).

use celestia_ui::components::composite::file_tree::FileTree;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::Input;
use celestia_ui::components::primitive::select::Select;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AsyncApp, ClipboardItem, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, PathPromptOptions, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, hsla, px,
};
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{Sizable as _, Size};
use halo_bridge::{Bridge, BridgeState, StashDto, StashEndpointDto};

use crate::components::{confirm_delete, prompt_text};
use crate::pages::chrome::radius_xs;
use crate::pages::colors::{method_color, method_short};
use crate::pages::editors::ToolEditors;

const ROW_H: f32 = 22.;
const ROW_PAD_X: f32 = 4.;
const ROW_GAP: f32 = 4.;
const ROW_CARET: f32 = 14.;
const CARET_ICON: f32 = 11.;
const INDENT: f32 = 12.;
const METHOD_BADGE_W: f32 = 34.;
const METHOD_BADGE_H: f32 = 14.;

/// Insomnia-like violet for folder / service marks in the tree.
fn folder_mark() -> Hsla {
    hsla(268. / 360., 0.55, 0.62, 1.)
}

pub fn render_folder_tree(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let editor_filter = editors
        .tree_filter
        .read(cx)
        .value()
        .trim()
        .to_ascii_lowercase();
    let filter = if !editor_filter.is_empty() {
        editor_filter
    } else {
        state.tree_filter.trim().to_ascii_lowercase()
    };
    let selected = state.selected_node_id.clone();
    let collapsed = state.collapsed_stashes.clone();
    let active_collection = state.active_collection_id.clone();

    let tops: Vec<StashDto> = state
        .stashes
        .iter()
        .filter(|s| stash_in_collection(s, active_collection.as_deref(), &state.stashes))
        .filter(|s| match s.parent_id.as_deref() {
            Some(pid) => !state.stashes.iter().any(|p| p.id == pid),
            None => true,
        })
        .filter(|s| subtree_matches(&filter, s, &state.stashes, &state.endpoints))
        .cloned()
        .collect();

    let accent = theme.accent;
    let has_collections = !state.collections.is_empty();
    let empty_collection = tops.is_empty() && filter.is_empty();

    let mut tree = FileTree::new("repeater-file-tree")
        .border_0()
        .rounded(px(0.))
        .padded(false)
        .font_family(theme.font_family.clone())
        .text_xs();

    for stash in &tops {
        tree = tree.child(render_stash_subtree(
            stash.clone(),
            0,
            &state.stashes,
            &state.endpoints,
            &filter,
            &collapsed,
            selected.as_deref(),
            accent,
            cx,
        ));
    }

    div()
        .id("folders-tree")
        .size_full()
        .flex()
        .flex_col()
        .gap_1p5()
        .bg(theme.background)
        .child(render_tree_header(editors, state, cx))
        .child(
            div()
                .id("folders-tree-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .px_2()
                .when(!has_collections, |el| {
                    el.child(crate::pages::chrome::empty_state(
                        "No collections yet",
                        "Create a collection to organize folders and requests.",
                        cx,
                    ))
                })
                .when(has_collections && empty_collection, |el| {
                    el.child(crate::pages::chrome::empty_state(
                        "No folders yet",
                        "Add a folder, then save requests into it.",
                        cx,
                    ))
                })
                .when(
                    has_collections && !filter.is_empty() && tops.is_empty(),
                    |el| {
                        el.child(crate::pages::chrome::empty_state(
                            "No matches",
                            "Try a different filter in this collection.",
                            cx,
                        ))
                    },
                )
                .when(has_collections && !tops.is_empty(), |el| el.child(tree)),
        )
}

fn stash_in_collection(stash: &StashDto, active: Option<&str>, stashes: &[StashDto]) -> bool {
    let Some(active) = active else {
        return true;
    };
    if stash.collection_id.as_deref() == Some(active) {
        return true;
    }
    // Nested folders may only carry membership via ancestors.
    let mut parent = stash.parent_id.clone();
    let mut guard = 0;
    while let Some(pid) = parent {
        if guard > 64 {
            break;
        }
        guard += 1;
        if let Some(p) = stashes.iter().find(|s| s.id == pid) {
            if p.collection_id.as_deref() == Some(active) {
                return true;
            }
            parent = p.parent_id.clone();
        } else {
            break;
        }
    }
    false
}

fn render_tree_header(editors: &ToolEditors, state: &BridgeState, _cx: &App) -> impl IntoElement {
    let has_active = state.active_collection_id.is_some();

    // Collection + filter share this rail so they group with the tree
    // (space, not a full-width bar above the split). Padding matches the
    // request toolbar (`px_2` / `py_1p5`) for one shared horizontal edge.
    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .py_1p5()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div().flex_1().min_w_0().child(
                        Select::new(&editors.repeater_collection)
                            .id("repeater-collection-select")
                            .with_size(Size::Small)
                            .placeholder("Select collection")
                            .accessibility_label("Active collection"),
                    ),
                )
                .child(
                    Button::icon(
                        "collection-add",
                        Phosphor::new(PhosphorIcon::Plus).size(px(14.)),
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .accessibility_label("New collection")
                    .tooltip("New collection")
                    .on_click(|_, window, cx| {
                        open_new_collection_dialog(window, cx);
                    }),
                )
                .child(
                    Button::icon(
                        "collection-import",
                        Phosphor::new(PhosphorIcon::UploadSimple).size(px(14.)),
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .accessibility_label("Import Postman")
                    .tooltip("Import Postman")
                    .on_click(|_, window, cx| {
                        open_postman_import(window, cx);
                    }),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&editors.tree_filter)
                            .with_size(Size::Small)
                            .cleanable(true),
                    ),
                )
                .child(
                    Button::new("tree-add")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .leading_icon(PhosphorIcon::Plus)
                        .tooltip("Add folder or request")
                        .disabled(!has_active)
                        .dropdown_menu(|menu, _, _| {
                            menu.item(
                                PopupMenuItem::new("Add folder")
                                    .icon(PhosphorIcon::FolderPlus)
                                    .on_click(|_, _, cx| {
                                        add_folder_for_selection(cx);
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Add request")
                                    .icon(PhosphorIcon::Plus)
                                    .on_click(|_, _, cx| {
                                        add_request_for_selection(cx);
                                    }),
                            )
                        }),
                ),
        )
}

pub(crate) fn open_new_collection_dialog(window: &mut Window, cx: &mut App) {
    open_rename_dialog(
        "New collection",
        "Collection",
        window,
        cx,
        move |name, cx| {
            let _ = Bridge::create_collection(name, cx);
        },
    );
}

/// Pick one or more Postman JSON files, read them off the UI thread, then save.
pub(crate) fn open_postman_import(window: &mut Window, cx: &mut App) {
    let dialog = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("Import Postman".into()),
    });
    let background = cx.background_executor().clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let paths = match dialog.await {
            Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
            Ok(Err(err)) => {
                let _ = cx.update(|cx| {
                    Bridge::state(cx).update(cx, |state| {
                        state.apply_err(halo_bridge::BridgeError::message(format!(
                            "Import dialog failed: {err}"
                        )));
                    });
                });
                return;
            }
            _ => return,
        };
        let read = background
            .spawn(async move { read_postman_documents(paths) })
            .await;
        let documents = match read {
            Ok(documents) => documents,
            Err(err) => {
                let _ = cx.update(|cx| {
                    Bridge::state(cx).update(cx, |state| {
                        state.apply_err(halo_bridge::BridgeError::message(err));
                    });
                });
                return;
            }
        };
        let _ = cx.update(|cx| {
            let _ = Bridge::import_postman_files(documents, cx);
        });
        let _ = window;
    })
    .detach();
}

fn read_postman_documents(paths: Vec<std::path::PathBuf>) -> Result<Vec<(String, String)>, String> {
    let mut documents = Vec::with_capacity(paths.len());
    for path in paths {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let text = std::fs::read_to_string(&path).map_err(|err| format!("{name}: {err}"))?;
        documents.push((name, text));
    }
    Ok(documents)
}

/// True when a request's name, URL, or method contains the filter text.
fn endpoint_matches(filter: &str, ep: &StashEndpointDto) -> bool {
    ep.name.to_ascii_lowercase().contains(filter)
        || ep.url.to_ascii_lowercase().contains(filter)
        || ep.method.to_ascii_lowercase().contains(filter)
}

/// True when the folder itself, one of its requests, or any nested folder
/// matches the filter — so ancestors of a match stay visible.
fn subtree_matches(
    filter: &str,
    stash: &StashDto,
    stashes: &[StashDto],
    endpoints: &[StashEndpointDto],
) -> bool {
    if filter.is_empty() {
        return true;
    }
    if stash.name.to_ascii_lowercase().contains(filter) {
        return true;
    }
    if endpoints
        .iter()
        .any(|e| e.stash_id == stash.id && endpoint_matches(filter, e))
    {
        return true;
    }
    stashes.iter().any(|child| {
        child.parent_id.as_deref() == Some(stash.id.as_str())
            && subtree_matches(filter, child, stashes, endpoints)
    })
}

#[allow(clippy::too_many_arguments)]
fn render_stash_subtree(
    stash: StashDto,
    depth: usize,
    stashes: &[StashDto],
    endpoints: &[StashEndpointDto],
    filter: &str,
    collapsed: &[String],
    selected: Option<&str>,
    accent: Hsla,
    cx: &App,
) -> AnyElement {
    let stash_id = stash.id.clone();
    let stash_name = stash.name.clone();
    let stash_node = format!("stash-{}", stash.id);
    let is_selected = selected == Some(stash_node.as_str());
    let theme = cx.theme();

    let children: Vec<StashDto> = stashes
        .iter()
        .filter(|s| s.parent_id.as_deref() == Some(stash_id.as_str()))
        .filter(|s| subtree_matches(filter, s, stashes, endpoints))
        .cloned()
        .collect();

    let requests: Vec<StashEndpointDto> = endpoints
        .iter()
        .filter(|e| e.stash_id == stash_id)
        .filter(|e| {
            filter.is_empty()
                || stash_name.to_ascii_lowercase().contains(filter)
                || endpoint_matches(filter, e)
        })
        .cloned()
        .collect();

    let is_open = if !filter.is_empty() {
        true
    } else {
        !collapsed.iter().any(|id| id == &stash_id)
    };

    let chevron = if is_open {
        PhosphorIcon::CaretDown
    } else {
        PhosphorIcon::CaretRight
    };
    let folder_icon = if is_open {
        PhosphorIcon::FolderOpen
    } else {
        PhosphorIcon::Folder
    };

    // Indent guide: 1px rail centered on the caret tip (pad + half caret − half border).
    let guide_ml = ROW_PAD_X + ROW_CARET / 2. - 0.5;
    let mut children_el = div()
        .id(SharedString::from(format!("stash-children-{stash_id}")))
        .w_full()
        .ml(px(guide_ml))
        .pl(px(INDENT / 2.))
        .border_l_1()
        .border_color(theme.border.opacity(0.45))
        .flex()
        .flex_col();

    if is_open {
        for child in children {
            children_el = children_el.child(render_stash_subtree(
                child,
                depth + 1,
                stashes,
                endpoints,
                filter,
                collapsed,
                selected,
                accent,
                cx,
            ));
        }
        for ep in requests {
            children_el =
                children_el.child(render_endpoint_row(ep, depth + 1, selected, accent, cx));
        }
    }

    let menu_id = stash_id.clone();
    let menu_name = stash_name.clone();
    let toggle_id = stash_id.clone();
    // Collections use the explorer package-folder mark; nested rows stay a folder.
    let mark_icon = folder_icon;

    div()
        .id(SharedString::from(stash_node.clone()))
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .id(SharedString::from(format!("stash-row-{stash_id}")))
                .w_full()
                .h(px(ROW_H))
                .px(px(ROW_PAD_X))
                .flex()
                .items_center()
                .gap(px(ROW_GAP))
                .rounded(theme.radius_sm())
                .cursor_pointer()
                .when(is_selected, |el| el.bg(accent.opacity(0.14)))
                .when(!is_selected, |el| {
                    el.hover(|el| el.bg(theme.muted.opacity(0.5)))
                })
                .on_click({
                    let node = stash_node.clone();
                    let t_id = toggle_id.clone();
                    move |_, _, cx| {
                        Bridge::select_folder_node(Some(node.clone()), cx);
                        toggle_stash_collapse(&t_id, cx);
                    }
                })
                .context_menu(move |menu, _, _| {
                    let rename_id = menu_id.clone();
                    let rename_name = menu_name.clone();
                    let delete_id = menu_id.clone();
                    let delete_name = menu_name.clone();
                    let add_id = menu_id.clone();
                    menu.item(
                        PopupMenuItem::new("New request")
                            .icon(PhosphorIcon::Plus)
                            .on_click(move |_, _, cx| {
                                create_request_in_stash(&add_id, cx);
                            }),
                    )
                    .item(
                        PopupMenuItem::new("New folder")
                            .icon(PhosphorIcon::FolderPlus)
                            .on_click({
                                let parent = menu_id.clone();
                                move |_, _, cx| {
                                    let n = Bridge::state(cx).read(cx).stashes.len() + 1;
                                    let _ = Bridge::create_stash_in(
                                        Some(&parent),
                                        &format!("Folder {n}"),
                                        cx,
                                    );
                                }
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("Rename")
                            .icon(PhosphorIcon::PencilSimple)
                            .on_click(move |_, window, cx| {
                                open_rename_stash_dialog(&rename_id, &rename_name, window, cx);
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("Delete")
                            .icon(PhosphorIcon::Trash)
                            .on_click(move |_, window, cx| {
                                confirm_delete(
                                    window,
                                    cx,
                                    "Delete folder?",
                                    format!(
                                        "{delete_name} and everything inside it (subfolders and saved requests) will be removed. This cannot be undone."
                                    ),
                                    {
                                        let id = delete_id.clone();
                                        move |_, cx| {
                                            let _ = Bridge::delete_stash(&id, cx);
                                        }
                                    },
                                );
                            }),
                    )
                })
                .child(
                    div()
                        .w(px(ROW_CARET))
                        .h(px(ROW_CARET))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_none()
                        .child(
                            Phosphor::new(chevron)
                                .size(px(CARET_ICON))
                                .color(theme.muted_foreground),
                        ),
                )
                .child(if depth == 0 {
                    let stem = if is_open {
                        "folder_package_open"
                    } else {
                        "folder_package"
                    };
                    crate::explorer_icons::explorer_icon(format!("explorer-icon/{stem}.svg"))
                        .into_any_element()
                } else {
                    Phosphor::new(mark_icon)
                        .size(px(13.))
                        .color(folder_mark())
                        .into_any_element()
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(stash_name),
                ),
        )
        .when(is_open, |el| el.child(children_el))
        .into_any_element()
}

fn request_label(ep: &StashEndpointDto) -> String {
    if !ep.name.trim().is_empty() {
        return ep.name.clone();
    }
    url_path_label(&ep.url)
}

fn url_path_label(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return "/".into();
    }
    // Prefer path (+ query) so the tree reads like Insomnia request rows.
    if let Some(scheme) = trimmed.find("://") {
        let after = &trimmed[scheme + 3..];
        if let Some(slash) = after.find('/') {
            return after[slash..].to_string();
        }
        return "/".into();
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn method_badge(method: &str, theme: &celestia_ui::theme::Theme) -> impl IntoElement {
    let col = method_color(method);
    div()
        .w(px(METHOD_BADGE_W))
        .h(px(METHOD_BADGE_H))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .px(px(2.))
        .rounded(radius_xs(theme))
        .bg(col.opacity(0.16))
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(col)
        .child(method_short(method))
}

fn render_endpoint_row(
    ep: StashEndpointDto,
    _depth: usize,
    selected: Option<&str>,
    accent: Hsla,
    cx: &App,
) -> AnyElement {
    let ep_node = format!("ep-{}", ep.id);
    let selected_ep = selected == Some(ep_node.as_str());
    let ep_id = ep.id.clone();
    let method = ep.method.clone();
    let title = request_label(&ep);
    let menu_id = ep_id.clone();
    let menu_title = title.clone();
    let menu_url = ep.url.clone();
    let theme = cx.theme();

    div()
        .id(SharedString::from(ep_node.clone()))
        .w_full()
        .h(px(ROW_H))
        .px(px(ROW_PAD_X))
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .when(selected_ep, |el| el.bg(accent.opacity(0.14)))
        .when(!selected_ep, |el| {
            el.hover(|el| el.bg(theme.muted.opacity(0.5)))
        })
        .on_click({
            let id = ep_id.clone();
            move |_, _, cx| {
                let _ = Bridge::load_endpoint(&id, cx);
            }
        })
        .context_menu(move |menu, _window, _cx| {
            let rename_id = menu_id.clone();
            let rename_title = menu_title.clone();
            let url = menu_url.clone();
            let delete_id = menu_id.clone();
            let delete_title = menu_title.clone();
            menu.item(
                PopupMenuItem::new("Rename")
                    .icon(PhosphorIcon::PencilSimple)
                    .on_click(move |_, window, cx| {
                        open_rename_endpoint_dialog(&rename_id, &rename_title, window, cx);
                    }),
            )
            .item(
                PopupMenuItem::new("Copy URL")
                    .icon(PhosphorIcon::LinkSimple)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                    }),
            )
            .separator()
            .item(
                PopupMenuItem::new("Delete")
                    .icon(PhosphorIcon::Trash)
                    .on_click(move |_, window, cx| {
                        confirm_delete(
                            window,
                            cx,
                            "Delete request?",
                            format!(
                                "{delete_title} will be removed from this folder. This cannot be undone."
                            ),
                            {
                                let id = delete_id.clone();
                                move |_, cx| {
                                    let _ = Bridge::delete_endpoint(&id, cx);
                                }
                            },
                        );
                    }),
            )
        })
        // Align under folder labels (caret + mark width).
        .child(div().w(px(ROW_CARET)).flex_none())
        .child(method_badge(&method, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title),
        )
        .into_any_element()
}

fn open_rename_stash_dialog(stash_id: &str, current_name: &str, window: &mut Window, cx: &mut App) {
    let id = stash_id.to_string();
    open_rename_dialog(
        "Rename folder",
        current_name,
        window,
        cx,
        move |name, cx| {
            let _ = Bridge::rename_stash(&id, name, cx);
        },
    );
}

fn open_rename_endpoint_dialog(
    endpoint_id: &str,
    current_name: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let id = endpoint_id.to_string();
    open_rename_dialog(
        "Rename request",
        current_name,
        window,
        cx,
        move |name, cx| {
            let _ = Bridge::rename_endpoint(&id, name, cx);
        },
    );
}

fn open_rename_dialog(
    title: &'static str,
    current_name: &str,
    window: &mut Window,
    cx: &mut App,
    on_ok: impl Fn(&str, &mut App) + 'static,
) {
    prompt_text(
        window,
        cx,
        title,
        current_name,
        if title.starts_with("New") {
            "Create"
        } else {
            "Rename"
        },
        on_ok,
    );
}

fn toggle_stash_collapse(stash_id: &str, cx: &mut App) {
    let mut collapsed = Bridge::state(cx).read(cx).collapsed_stashes.clone();
    if let Some(pos) = collapsed.iter().position(|id| id == stash_id) {
        collapsed.remove(pos);
    } else {
        collapsed.push(stash_id.to_string());
    }
    Bridge::set_collapsed_stashes(collapsed, cx);
}

fn add_folder_for_selection(cx: &mut App) {
    let parent = Bridge::state(cx)
        .read(cx)
        .selected_node_id
        .as_deref()
        .and_then(|sel| sel.strip_prefix("stash-").map(str::to_string));
    // Only nest under a folder that belongs to the active collection.
    let parent = parent.and_then(|pid| {
        let state = Bridge::state(cx).read(cx);
        let active = state.active_collection_id.as_deref();
        state
            .stashes
            .iter()
            .find(|s| s.id == pid)
            .filter(|s| stash_in_collection(s, active, &state.stashes))
            .map(|_| pid)
    });
    let n = Bridge::state(cx).read(cx).stashes.len() + 1;
    let _ = Bridge::create_stash_in(parent.as_deref(), &format!("Folder {n}"), cx);
}

fn add_request_for_selection(cx: &mut App) {
    let active = Bridge::state(cx).read(cx).active_collection_id.clone();
    let (target_stash, is_new) = {
        let s = Bridge::state(cx).read(cx);
        let from_sel = s.selected_node_id.as_deref().and_then(|sel| {
            if sel.starts_with("stash-") {
                Some(sel.trim_start_matches("stash-").to_string())
            } else if sel.starts_with("ep-") {
                let ep_id = sel.trim_start_matches("ep-");
                s.endpoints
                    .iter()
                    .find(|e| e.id == ep_id)
                    .map(|e| e.stash_id.clone())
            } else {
                None
            }
        });
        let in_collection = |id: &str| {
            s.stashes
                .iter()
                .find(|st| st.id == id)
                .is_some_and(|st| stash_in_collection(st, active.as_deref(), &s.stashes))
        };
        let from_sel = from_sel.filter(|id| in_collection(id));
        let fallback = s
            .stashes
            .iter()
            .find(|st| {
                st.parent_id.is_none() && stash_in_collection(st, active.as_deref(), &s.stashes)
            })
            .map(|st| st.id.clone());
        match from_sel.or(fallback) {
            Some(id) => (Some(id), false),
            None => (None, true),
        }
    };
    if is_new {
        let _ = Bridge::create_stash("Folder 1", cx);
        let first_id = {
            let s = Bridge::state(cx).read(cx);
            s.stashes
                .iter()
                .find(|st| {
                    st.parent_id.is_none() && stash_in_collection(st, active.as_deref(), &s.stashes)
                })
                .map(|st| st.id.clone())
        };
        if let Some(first_id) = first_id {
            let _ = Bridge::create_endpoint(&first_id, "Request 1", cx);
        }
    } else if let Some(stash_id) = target_stash {
        create_request_in_stash(&stash_id, cx);
    }
}

fn create_request_in_stash(stash_id: &str, cx: &mut App) {
    let n = Bridge::state(cx).read(cx).endpoints.len() + 1;
    let _ = Bridge::create_endpoint(stash_id, &format!("Request {n}"), cx);
    let mut collapsed = Bridge::state(cx).read(cx).collapsed_stashes.clone();
    if let Some(pos) = collapsed.iter().position(|id| id == stash_id) {
        collapsed.remove(pos);
        Bridge::set_collapsed_stashes(collapsed, cx);
    }
}

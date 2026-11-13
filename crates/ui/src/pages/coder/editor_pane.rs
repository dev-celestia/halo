//! Editor column — tab strip on top, editor (or image preview) below.
//! Supports nested splits (max depth / pane cap) with per-pane floating controls.
//! The terminal dock lives in `workspace` as a resizable bottom panel.

use std::path::Path;

use celestia_ui::components::Tooltip;
use celestia_ui::components::composite::tab_bar::{TabBar, TabBarItem};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::{ActiveTheme as _, Theme};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use gpui_component::text::{TextView, TextViewStyle};
use halo_bridge::{Bridge, BridgeState, CoderFileKind, CoderLayout, CoderSplitAxis};

use super::image_preview;
use super::lang;
use crate::pages::editors::ToolEditors;

const ZOOM_STEP: i16 = 25;
const ZOOM_MIN: u16 = 25;
const ZOOM_MAX: u16 = 400;

#[derive(Clone)]
struct PaneTabInfo {
    path: String,
    kind: CoderFileKind,
    dirty: bool,
    zoom_pct: u16,
}

pub(super) fn render_editor_pane(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    div()
        .size_full()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(render_layout(&state.coder.layout, state, editors, cx))
}

fn files_for_pane(state: &BridgeState, pane_id: u8) -> Vec<PaneTabInfo> {
    let listed = state.coder.pane_tabs(pane_id);
    if pane_id == 0 && listed.is_empty() && !state.coder.is_split() {
        // Single-pane before tabs were seeded: show all buffers without cloning file contents.
        return state
            .coder
            .open_files
            .iter()
            .map(|f| PaneTabInfo {
                path: f.path.clone(),
                kind: f.kind,
                dirty: f.dirty,
                zoom_pct: f.zoom_pct,
            })
            .collect();
    }
    listed
        .iter()
        .filter_map(|path| {
            state
                .coder
                .open_files
                .iter()
                .find(|f| &f.path == path)
                .map(|f| PaneTabInfo {
                    path: f.path.clone(),
                    kind: f.kind,
                    dirty: f.dirty,
                    zoom_pct: f.zoom_pct,
                })
        })
        .collect()
}

fn render_layout(
    layout: &CoderLayout,
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> AnyElement {
    match layout {
        CoderLayout::Leaf(id) => render_editor_group(state, editors, *id, cx).into_any_element(),
        CoderLayout::Split {
            axis,
            first,
            second,
        } => {
            let a = render_layout(first, state, editors, cx);
            let b = render_layout(second, state, editors, cx);
            let key = format!("coder-split-{}-{}", first.first_leaf(), second.first_leaf());
            match axis {
                CoderSplitAxis::Horizontal => ResizablePanelGroup::horizontal(key)
                    .with_handle(true)
                    .child(resizable_panel().child(a))
                    .child(resizable_panel().child(b))
                    .into_any_element(),
                CoderSplitAxis::Vertical => ResizablePanelGroup::vertical(key)
                    .with_handle(true)
                    .child(resizable_panel().child(a))
                    .child(resizable_panel().child(b))
                    .into_any_element(),
            }
        }
    }
}

fn render_editor_group(
    state: &BridgeState,
    editors: &ToolEditors,
    group: u8,
    cx: &App,
) -> impl IntoElement {
    let files = files_for_pane(state, group);
    let active = state.coder.pane_active(group).map(str::to_string);
    let editor = editors.coder_editor(group).clone();
    let editor_pool = editors.coder_editors.clone();
    let focused = state.coder.focused_pane == group;
    let can_split = state.coder.can_split_pane(group) && state.coder.pane_active(group).is_some();
    let can_close = state.coder.is_split();
    let theme = cx.theme();
    let active_image = active.as_ref().and_then(|path| {
        files
            .iter()
            .find(|f| &f.path == path)
            .filter(|f| image_preview::is_image_tab(f.kind))
            .map(|f| (f.path.clone(), f.zoom_pct))
    });
    let md_path = active.as_ref().and_then(|path| {
        if Bridge::coder_is_markdown_path(path) && active_image.is_none() {
            Some(path.clone())
        } else {
            None
        }
    });
    let md_preview = md_path
        .as_ref()
        .is_some_and(|path| Bridge::coder_markdown_preview_active(path, cx));
    let md_preview_body = if md_preview {
        md_path.as_ref().and_then(|path| {
            state
                .coder
                .open_files
                .iter()
                .find(|f| &f.path == path)
                .map(|f| f.content.clone())
        })
    } else {
        None
    };
    let pane_empty = files.is_empty();
    let root_path = state.coder.root_path.clone();

    div()
        .id(SharedString::from(format!("coder-group-{group}")))
        .relative()
        .size_full()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .when(focused && state.coder.is_split(), |d| {
            d.border_l_2().border_color(theme.primary.opacity(0.55))
        })
        .on_mouse_down(MouseButton::Left, {
            let editors = editor_pool.clone();
            move |_, _, cx| {
                let _ = Bridge::coder_focus_group(group, &editors, cx);
            }
        })
        // Empty panes skip the tab strip so its underline does not sit alone.
        .when(!pane_empty, |d| {
            d.child(render_group_tab_bar(
                editors,
                group,
                files,
                active.clone(),
                cx,
            ))
        })
        .child(
            div()
                .relative()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(if pane_empty {
                    render_empty_pane(group, root_path.as_deref(), editor_pool.clone(), cx)
                        .into_any_element()
                } else if let Some((path, zoom_pct)) = active_image.clone() {
                    image_preview::render_image_preview(&path, zoom_pct, cx).into_any_element()
                } else if let Some(content) = md_preview_body {
                    render_markdown_preview(content, cx).into_any_element()
                } else {
                    div().size_full().min_h_0().child(editor).into_any_element()
                })
                .child(render_pane_floating_bar(
                    group,
                    can_split,
                    can_close,
                    state.coder_settings.minimap,
                    state.coder_settings.word_wrap,
                    active_image,
                    md_path,
                    md_preview,
                    editor_pool,
                    theme,
                )),
        )
}

fn render_markdown_preview(content: String, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let display_text = if content.trim().is_empty() {
        "# Empty file\n\nStart typing Markdown in the editor, then toggle preview.".to_string()
    } else {
        content
    };
    let style = TextViewStyle::default().paragraph_gap(rems(0.75));

    div()
        .id("coder-markdown-preview")
        .size_full()
        .overflow_y_scroll()
        .p_6()
        .bg(theme.background)
        .child(
            div().w_full().max_w(px(820.)).mx_auto().child(
                TextView::markdown("coder-md-preview-text", display_text)
                    .style(style)
                    .selectable(true)
                    .w_full()
                    .min_w_0(),
            ),
        )
}

const PANE_ICON: f32 = 16.;

/// Clickable glyph with no button padding. `active` is a stronger color, not a fill.
fn pane_icon(
    id: impl Into<ElementId>,
    icon: PhosphorIcon,
    tip: impl Into<SharedString>,
    enabled: bool,
    active: bool,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let tip = tip.into();
    let color = if active {
        theme.foreground
    } else {
        theme.muted_foreground
    };
    let mut el = div()
        .id(id)
        .flex_shrink_0()
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .child(Phosphor::new(icon).size(px(PANE_ICON)).color(color));
    if enabled {
        el = el.cursor_pointer().on_click(on_click);
    } else {
        drop(on_click);
        el = el.opacity(0.35);
    }
    el
}

fn render_pane_floating_bar(
    group: u8,
    can_split: bool,
    can_close: bool,
    minimap: bool,
    word_wrap: bool,
    image: Option<(String, u16)>,
    markdown_path: Option<String>,
    markdown_preview: bool,
    editors: Vec<Entity<celestia_ui::components::CodeEditor>>,
    theme: &Theme,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let show_text_toggles = image.is_none();

    // One floating pill: zoom (images) or wrap/minimap/preview (text), then split/close.
    let mut bar = div()
        .id(SharedString::from(format!("coder-pane-controls-{group}")))
        .absolute()
        .bottom(px(12.))
        .right(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(6.))
        .py(px(4.))
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover.opacity(0.92))
        .shadow_md();

    if let Some((path, zoom_pct)) = image {
        let path_zoom = path.clone();
        let path_reset = path.clone();
        let name = Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        let can_zoom_out = zoom_pct > ZOOM_MIN;
        let can_zoom_in = zoom_pct < ZOOM_MAX;
        let can_reset = zoom_pct != 100;
        bar = bar
            .child(pane_icon(
                SharedString::from(format!("coder-image-zoom-out-{group}")),
                PhosphorIcon::MagnifyingGlassMinus,
                "Zoom out",
                can_zoom_out,
                false,
                theme,
                {
                    let path = path_zoom.clone();
                    move |_, _, cx| {
                        let _ = Bridge::coder_image_zoom_by(&path, -ZOOM_STEP, cx);
                    }
                },
            ))
            .child({
                let label = format!("{zoom_pct}%");
                div()
                    .id(SharedString::from(format!("coder-image-zoom-reset-{group}")))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .tooltip(|window, cx| Tooltip::new("Reset zoom").build(window, cx))
                    .when(can_reset, |d| {
                        d.cursor_pointer().on_click(move |_, _, cx| {
                            let _ = Bridge::coder_image_zoom_set(&path_reset, 100, cx);
                        })
                    })
                    .when(!can_reset, |d| d.opacity(0.35))
                    .child(label)
            })
            .child(pane_icon(
                SharedString::from(format!("coder-image-zoom-in-{group}")),
                PhosphorIcon::MagnifyingGlassPlus,
                "Zoom in",
                can_zoom_in,
                false,
                theme,
                move |_, _, cx| {
                    let _ = Bridge::coder_image_zoom_by(&path_zoom, ZOOM_STEP, cx);
                },
            ))
            .child(
                div()
                    .max_w(px(120.))
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .ml_1()
                    .mr_1()
                    .child(name),
            );
    }

    if show_text_toggles {
        bar = bar
            .child(pane_icon(
                SharedString::from(format!("coder-word-wrap-{group}")),
                PhosphorIcon::TextAlignLeft,
                if word_wrap {
                    "Unwrap lines (Alt+Z)"
                } else {
                    "Wrap lines (Alt+Z)"
                },
                true,
                word_wrap,
                theme,
                |_, _, cx| {
                    Bridge::update_coder_settings(cx, move |s| {
                        s.word_wrap = !s.word_wrap;
                    });
                },
            ))
            .child(pane_icon(
                SharedString::from(format!("coder-minimap-{group}")),
                PhosphorIcon::MapTrifold,
                if minimap {
                    "Hide minimap"
                } else {
                    "Show minimap"
                },
                true,
                minimap,
                theme,
                |_, _, cx| {
                    Bridge::update_coder_settings(cx, move |s| {
                        s.minimap = !s.minimap;
                    });
                },
            ));
        if let Some(path) = markdown_path {
            bar = bar.child(pane_icon(
                SharedString::from(format!("coder-md-preview-{group}")),
                PhosphorIcon::Eye,
                if markdown_preview {
                    "Show Markdown source"
                } else {
                    "Preview Markdown"
                },
                true,
                markdown_preview,
                theme,
                {
                    let editors = editors.clone();
                    move |_, _, cx| {
                        let _ = Bridge::coder_toggle_markdown_preview(&path, &editors, group, cx);
                    }
                },
            ));
        }
    }

    bar.child(pane_icon(
        SharedString::from(format!("coder-split-right-{group}")),
        PhosphorIcon::SquareSplitHorizontal,
        "Split Right",
        can_split,
        false,
        theme,
        {
            let editors = editors.clone();
            move |_, window, cx| {
                let _ = Bridge::coder_split_pane(
                    group,
                    CoderSplitAxis::Horizontal,
                    &editors,
                    window,
                    cx,
                );
            }
        },
    ))
    .child(pane_icon(
        SharedString::from(format!("coder-split-down-{group}")),
        PhosphorIcon::SquareSplitVertical,
        "Split Down",
        can_split,
        false,
        theme,
        {
            let editors = editors.clone();
            move |_, window, cx| {
                let _ = Bridge::coder_split_pane(
                    group,
                    CoderSplitAxis::Vertical,
                    &editors,
                    window,
                    cx,
                );
            }
        },
    ))
    .child(pane_icon(
        SharedString::from(format!("coder-close-pane-{group}")),
        PhosphorIcon::X,
        "Close Pane",
        can_close,
        false,
        theme,
        {
            let editors = editors.clone();
            move |_, window, cx| {
                let _ = Bridge::coder_close_pane(group, &editors, window, cx);
            }
        },
    ))
}

/// Welcome shown in a pane that has no document tabs.
fn render_empty_pane(
    group: u8,
    root: Option<&str>,
    editors: Vec<Entity<celestia_ui::components::CodeEditor>>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let files = root.map(lang::existing_quick_files).unwrap_or_default();

    let mut shortcuts = div()
        .w_full()
        .max_w(px(480.))
        .flex()
        .flex_row()
        .flex_wrap()
        .justify_center()
        .gap(px(8.));
    for (index, (label, abs)) in files.into_iter().enumerate() {
        let editors = editors.clone();
        let icon_name = Path::new(&label)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&label)
            .to_string();
        shortcuts = shortcuts.child(
            Button::new(SharedString::from(format!("coder-empty-file-{group}-{index}")))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Small)
                .leading_icon(lang::file_mark(&icon_name, px(14.)))
                .label(label)
                .on_click(move |_, window, cx| {
                    let _ = Bridge::coder_focus_group(group, &editors, cx);
                    let _ = Bridge::coder_open_file(&abs, &editors, window, cx);
                }),
        );
    }

    div()
        .id(SharedString::from(format!("coder-empty-pane-{group}")))
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(16.))
        .p(px(24.))
        .child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child("No file open"),
        )
        .child(
            Button::new(SharedString::from(format!("coder-empty-goto-{group}")))
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::MagnifyingGlass)
                .label("Go to File")
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::actions::ToggleCoderQuickOpen), cx);
                }),
        )
        .child(shortcuts)
}

/// Previous TabBar content height: 4px top inset + 30px chips.
const FILE_TAB_BAR_HEIGHT: Pixels = px(34.);
/// Shorter chips; wrapper keeps [`FILE_TAB_BAR_HEIGHT`].
const FILE_TAB_HEIGHT: Pixels = px(26.);

fn render_group_tab_bar(
    editors: &ToolEditors,
    group: u8,
    files: Vec<PaneTabInfo>,
    active: Option<String>,
    cx: &App,
) -> impl IntoElement {
    let editor = editors.coder_editor(group).clone();
    let tab_scroll = editors.coder_tab_scroll(group);
    let tab_count = files.len();
    let has_others = tab_count > 1 && active.is_some();

    let active_ix = active
        .as_ref()
        .and_then(|path| files.iter().position(|f| &f.path == path));
    tab_scroll.reveal_active(active.as_deref(), active_ix);

    let menu = Button::new(SharedString::from(format!("coder-tabs-menu-{group}")))
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconSm)
        .leading_icon(PhosphorIcon::DotsThreeVertical)
        .tooltip("Manage tabs")
        .dropdown_menu_with_anchor(Anchor::TopRight, {
            let editor = editor.clone();
            let active = active.clone();
            move |menu, _, _| {
                let mut menu = menu;
                if let Some(path) = active.clone() {
                    let editor_close = editor.clone();
                    menu = menu.item(PopupMenuItem::new("Close").on_click(move |_, window, cx| {
                        let _ = Bridge::coder_close_tab_in_group(
                            &path,
                            &editor_close,
                            group,
                            window,
                            cx,
                        );
                    }));
                }
                {
                    let editor_others = editor.clone();
                    let keep = active.clone();
                    menu = menu.item(
                        PopupMenuItem::new("Close Others")
                            .disabled(!has_others)
                            .on_click(move |_, window, cx| {
                                let Some(keep) = keep.as_ref() else {
                                    return;
                                };
                                let _ = Bridge::coder_close_other_tabs_in_group(
                                    keep,
                                    &editor_others,
                                    group,
                                    window,
                                    cx,
                                );
                            }),
                    );
                }
                {
                    let editor_all = editor.clone();
                    menu = menu.item(PopupMenuItem::new("Close All").on_click(
                        move |_, window, cx| {
                            let _ = Bridge::coder_close_all_tabs_in_group(
                                &editor_all,
                                group,
                                window,
                                cx,
                            );
                        },
                    ));
                }
                menu
            }
        });

    let mut bar = TabBar::new(format!("coder-tabs-{group}"))
        .h(FILE_TAB_BAR_HEIGHT)
        .tab_height(FILE_TAB_HEIGHT)
        .active(active.clone().unwrap_or_default())
        // Document tabs are neutral: the active chip and the strip's bottom rule
        // use the plain border tone, not the primary accent. `border_color`
        // refines the component's own `border_b_1` primary rule.
        .active_border(cx.theme().window_border)
        .border_color(cx.theme().window_border)
        .track_scroll(&tab_scroll.handle)
        // Keep the ⋮ inside the strip so the border-bottom continues under it.
        .trailing(menu);

    for file in files {
        let label = Path::new(&file.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file.path.as_str())
            .to_string();
        let dirty = if file.kind == CoderFileKind::Image {
            false
        } else {
            file.dirty
        };
        bar = bar.item(
            TabBarItem::new(file.path, label.clone())
                .leading(lang::file_mark(&label, px(14.)))
                .dirty(dirty)
                .closable(true),
        );
    }

    bar.on_select({
        let editor = editor.clone();
        move |id, window, cx| {
            let path = id.to_string();
            let _ = Bridge::coder_select_tab_in_group(&path, &editor, group, window, cx);
        }
    })
    .on_close({
        let editor = editor.clone();
        move |id, window, cx| {
            let path = id.to_string();
            let _ = Bridge::coder_close_tab_in_group(&path, &editor, group, window, cx);
        }
    })
}

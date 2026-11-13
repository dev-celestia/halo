//! Notes sidebar: a compact header (search, active note, library) and the note list.

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, NoteRecord, NotesViewMode};

use super::super::editors::ToolEditors;
use super::chrome::{NOTES_INLINE_PAD, NOTES_SIDEBAR_WIDTH};
use super::helpers::{count_words_and_chars, format_relative_time};

pub fn render_notes_sidebar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let open_notes = state.filtered_open_notes();
    let active = state.active_note();
    let total_notes = state.notes.len();
    let has_search = !state.notes_search_query.trim().is_empty();

    div()
        .id("notes-sidebar")
        .w(NOTES_SIDEBAR_WIDTH)
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .min_h_0()
        .overflow_hidden()
        .bg(theme.muted.opacity(0.1))
        .border_r_1()
        .border_color(theme.border)
        .child(render_notes_header(
            state,
            editors,
            active,
            total_notes,
            has_search,
            cx,
        ))
        .child(
            div()
                .id("notes-sidebar-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap_0p5()
                .px_2()
                .py_2()
                .children(open_notes.into_iter().map(|note| {
                    let is_active = state.notes_active_id.as_deref() == Some(note.id.as_str());
                    render_note_row(note, is_active, cx)
                })),
        )
}

fn render_notes_header(
    state: &BridgeState,
    editors: &ToolEditors,
    active: Option<&NoteRecord>,
    total_notes: usize,
    has_search: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let search = editors.notes_search.clone();
    let search_apply = search.clone();

    div()
        .id("notes-sidebar-top")
        .w_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap_2()
        .px(NOTES_INLINE_PAD)
        .pt_2()
        .pb_2()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .w_full()
                .min_w_0()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&editors.notes_search)),
                )
                .when(has_search, |this| {
                    let search_clear = search.clone();
                    this.child(
                        Button::new("notes-search-clear")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::X)
                            .tooltip("Clear search")
                            .on_click(move |_, window, cx| {
                                search_clear.update(cx, |s, cx| {
                                    s.set_value("", window, cx);
                                });
                                let _ = Bridge::set_notes_search(String::new(), cx);
                            }),
                    )
                })
                .child(
                    Button::new("notes-search-apply")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::MagnifyingGlass)
                        .tooltip("Filter open notes")
                        .on_click(move |_, _, cx| {
                            let q = search_apply.read(cx).value().to_string();
                            let _ = Bridge::set_notes_search(q, cx);
                        }),
                )
                .child(
                    Button::new("sidebar-new-note-btn")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Plus)
                        .tooltip("New note")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::create_note(cx);
                        }),
                ),
        )
        .when_some(active.cloned(), |this, note| {
            this.child(render_active_note_controls(&note, state, editors, cx))
        })
        .child(
            div().flex().items_center().child(
                Button::new("sidebar-library-btn")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::FolderSimple)
                    .label(format!("Library ({total_notes})"))
                    .tooltip("Browse and organize all saved notes")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::set_notes_library_open(true, cx);
                    }),
            ),
        )
}

fn render_note_row(note: &NoteRecord, is_active: bool, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let note_id = note.id.clone();
    let note_title = note.name.clone();
    let select_id = note_id.clone();
    let close_id = note_id.clone();
    let rename_id = note_id.clone();
    let left_id = note_id.clone();
    let right_id = note_id.clone();
    let title_tip: SharedString = note_title.clone().into();

    let row = div()
        .id(SharedString::from(format!("note-sidebar-row-{}", note_id)))
        .w_full()
        .h(px(32.))
        .px_2()
        .flex()
        .items_center()
        .gap_1p5()
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .bg(if is_active {
            theme.accent.opacity(0.14)
        } else {
            theme.transparent
        })
        .hover(|this| {
            if is_active {
                this
            } else {
                this.bg(theme.muted.opacity(0.35))
            }
        })
        .on_click(move |_, _, cx| {
            let _ = Bridge::open_note(&select_id, cx);
        })
        .child(
            div()
                .size(px(14.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .text_color(if is_active {
                    theme.primary
                } else {
                    theme.muted_foreground
                })
                .child(PhosphorIcon::FileText),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "note-sidebar-title-{}",
                    note_id
                )))
                .flex_1()
                .min_w_0()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(if is_active {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .overflow_hidden()
                .whitespace_nowrap()
                .tooltip(move |window, cx| Tooltip::new(title_tip.clone()).build(window, cx))
                .child(note_title),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "note-sidebar-close-{}",
                    close_id
                )))
                .size(px(18.))
                .flex_none()
                .rounded_sm()
                .flex()
                .items_center()
                .justify_center()
                .hover(|this| this.bg(theme.muted).text_color(theme.foreground))
                .on_click(move |_, _, cx| {
                    let _ = Bridge::close_note_tab(&close_id, cx);
                })
                .child(PhosphorIcon::X),
        );

    row.context_menu(move |menu, _window, _cx| {
        let r_id = rename_id.clone();
        let l_id = left_id.clone();
        let rg_id = right_id.clone();
        let c_id = rename_id.clone();

        menu.item(PopupMenuItem::new("Rename").on_click(move |_, _, cx| {
            let _ = Bridge::set_notes_editing_title(Some(r_id.clone()), cx);
        }))
        .separator()
        .item(
            PopupMenuItem::new("Close notes above").on_click(move |_, _, cx| {
                let _ = Bridge::close_notes_to_left(&l_id, cx);
            }),
        )
        .item(
            PopupMenuItem::new("Close notes below").on_click(move |_, _, cx| {
                let _ = Bridge::close_notes_to_right(&rg_id, cx);
            }),
        )
        .separator()
        .item(PopupMenuItem::new("Close note").on_click(move |_, _, cx| {
            let _ = Bridge::close_note_tab(&c_id, cx);
        }))
    })
}

fn render_active_note_controls(
    active_note: &NoteRecord,
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let is_editing_title = state.notes_editing_title_id.as_deref() == Some(&active_note.id);
    let note_id = active_note.id.clone();
    let note_name = active_note.name.clone();
    let note_body = active_note.note.clone();
    let (words, chars) = count_words_and_chars(&note_body);
    let updated_str = format_relative_time(&active_note.updated_at);
    let current_view_mode = state.notes_view_mode;

    let rename_input = editors.notes_rename.clone();
    let rename_for_start = rename_input.clone();
    let id_for_submit = note_id.clone();
    let id_for_start = note_id.clone();
    let name_for_start = note_name.clone();
    let note_clone_for_copy = note_body.clone();
    let note_clone_for_export = active_note.clone();
    let note_clone_for_promote = active_note.clone();
    let title_tip: SharedString = note_name.clone().into();

    let stats_tip: SharedString =
        format!("{words} words · {chars} chars · Updated {updated_str}").into();

    div()
        .id("notes-sidebar-toolbar")
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(if is_editing_title {
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(div().w_full().child(Input::new(&editors.notes_rename)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            Button::new("note-rename-submit")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Check)
                                .tooltip("Save Title")
                                .on_click(move |_, _, cx| {
                                    let new_name = rename_input.read(cx).value().to_string();
                                    let _ = Bridge::rename_note(&id_for_submit, &new_name, cx);
                                }),
                        )
                        .child(
                            Button::new("note-rename-cancel")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::X)
                                .tooltip("Cancel")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::set_notes_editing_title(None, cx);
                                }),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .min_w_0()
                .child(
                    div()
                        .id("note-sidebar-active-title")
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .tooltip(move |window, cx| {
                            Tooltip::new(title_tip.clone()).build(window, cx)
                        })
                        .child(note_name),
                )
                .child(
                    Button::new("note-rename-start")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::PencilSimple)
                        .tooltip("Rename Note")
                        .on_click(move |_, window, cx| {
                            rename_for_start.update(cx, |s, cx| {
                                s.set_value(name_for_start.clone(), window, cx);
                            });
                            let _ = Bridge::set_notes_editing_title(Some(id_for_start.clone()), cx);
                        }),
                )
                .into_any_element()
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(
                    div()
                        .id("note-sidebar-stats")
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .tooltip(move |window, cx| {
                            Tooltip::new(stats_tip.clone()).build(window, cx)
                        })
                        .child(format!("{words} words · {chars} chars")),
                )
                .child(
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .child(format!("Updated {updated_str}")),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .min_w_0()
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .p_0p5()
                        .rounded_lg()
                        .bg(theme.muted.opacity(0.3))
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            Button::new("mode-edit-btn")
                                .variant(if current_view_mode == NotesViewMode::Edit {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                })
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Code)
                                .tooltip("Editor")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::set_notes_view_mode(NotesViewMode::Edit, cx);
                                }),
                        )
                        .child(
                            Button::new("mode-split-btn")
                                .variant(if current_view_mode == NotesViewMode::Split {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                })
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Columns)
                                .tooltip("Split")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::set_notes_view_mode(NotesViewMode::Split, cx);
                                }),
                        )
                        .child(
                            Button::new("mode-preview-btn")
                                .variant(if current_view_mode == NotesViewMode::Preview {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                })
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Eye)
                                .tooltip("Preview")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::set_notes_view_mode(NotesViewMode::Preview, cx);
                                }),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Save")
                                .child(
                                    div()
                                        .px_1()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(theme.muted.opacity(0.35))
                                        .border_1()
                                        .border_color(theme.border)
                                        .text_xs()
                                        .font_weight(FontWeight::NORMAL)
                                        .child("⌘S"),
                                ),
                        )
                        .child(
                            Button::new("note-actions-menu")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::DotsThreeVertical)
                                .tooltip("Note actions")
                                .dropdown_menu_with_anchor(Anchor::BottomRight, {
                                    move |menu, _, _| {
                                        let copy = note_clone_for_copy.clone();
                                        let export = note_clone_for_export.clone();
                                        let promote = note_clone_for_promote.clone();
                                        menu.item(PopupMenuItem::new("Copy note").on_click(
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy.clone(),
                                                ));
                                                let _ = Bridge::state(cx).update(cx, |s| {
                                                    s.apply_ok_msg(
                                                        "Note copied to clipboard".into(),
                                                    );
                                                });
                                            },
                                        ))
                                        .item(PopupMenuItem::new("Export as Markdown").on_click(
                                            move |_, _, cx| {
                                                let title = &export.name;
                                                let content = &export.note;
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    content.clone(),
                                                ));
                                                let _ = Bridge::state(cx).update(cx, |s| {
                                                    s.apply_ok_msg(format!(
                                                        "Exported \"{title}\" to clipboard"
                                                    ));
                                                });
                                            },
                                        ))
                                        .item(PopupMenuItem::new("Promote to Memory").on_click(
                                            move |_, _, cx| {
                                                let _ =
                                                    Bridge::promote_note_to_memory(&promote, cx);
                                            },
                                        ))
                                        .separator()
                                        .item(
                                            PopupMenuItem::new("Drawing Studio").on_click(
                                                |_, _, cx| {
                                                    let _ =
                                                        Bridge::set_notes_drawing_open(true, cx);
                                                },
                                            ),
                                        )
                                    }
                                }),
                        ),
                ),
        )
}

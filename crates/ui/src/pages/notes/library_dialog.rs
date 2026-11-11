//! Saved Notes Library modal dialog: search, filter pills, sort options, and note cards.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, NoteRecord, NotesFilterTab, NotesSortOption};

use super::empty_states::render_library_empty_state;
use super::helpers::{extract_snippet, format_relative_time};
use crate::components::confirm_delete;
use crate::pages::editors::ToolEditors;

pub fn render_saved_notes_dialog(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let (all_count, open_count, closed_count) = state.library_counts();
    let filtered_notes = state.filtered_and_sorted_library_notes();
    let total_shown = filtered_notes.len();
    let current_filter = state.notes_library_filter;
    let current_sort = state.notes_library_sort;
    let active_tab_id = state.notes_active_id.as_deref();

    let search_clear = editors.notes_library_search.clone();
    let has_search = !state.notes_library_search.trim().is_empty();

    div()
        .id("saved-notes-dialog-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background.opacity(0.65))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            let _ = Bridge::set_notes_library_open(false, cx);
        })
        .child(
            div()
                .id("saved-notes-dialog-panel")
                .role(Role::Dialog)
                .aria_label("Saved Notes Library")
                .w(px(780.))
                .max_w(relative(0.95))
                .h(px(580.))
                .max_h(relative(0.92))
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(theme.radius_xl())
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .shadow(vec![BoxShadow {
                    offset: point(px(0.), px(18.)),
                    blur_radius: px(48.),
                    spread_radius: px(0.),
                    inset: false,
                    color: hsla(0., 0., 0., 0.35),
                }])
                .on_mouse_down(MouseButton::Left, |_, _, _| {})
                // Dialog Header
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_6()
                        .py_4()
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.1))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(theme.foreground)
                                        .child("Saved Notes Library"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(format!(
                                            "Manage, search, and reopen all your saved notes ({all_count} total)"
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Button::new("lib-header-new-note")
                                        .variant(ButtonVariant::Primary)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::Plus)
                                        .label("New Note")
                                        .on_click(|_, _, cx| {
                                            let _ = Bridge::create_note(cx);
                                        }),
                                )
                                .child(
                                    Button::new("lib-header-close")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::X)
                                        .tooltip("Close Library")
                                        .on_click(|_, _, cx| {
                                            let _ = Bridge::set_notes_library_open(false, cx);
                                        }),
                                ),
                        ),
                )
                // Dialog Toolbar: Search, Filter Tabs, Sort
                .child(
                    div()
                        .w_full()
                        .px_6()
                        .py_3()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.04))
                        // Search bar
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .w_full()
                                .max_w(px(260.))
                                .child(
                                    div()
                                        .flex_1()
                                        .child(Input::new(&editors.notes_library_search)),
                                )
                                .when(has_search, |this| {
                                    this.child(
                                        Button::new("lib-search-clear")
                                            .variant(ButtonVariant::Ghost)
                                            .size(ButtonSize::Small)
                                            .leading_icon(PhosphorIcon::X)
                                            .tooltip("Clear")
                                            .on_click(move |_, window, cx| {
                                                search_clear.update(cx, |s, cx| s.set_value("", window, cx));
                                                let _ = Bridge::set_notes_library_search(String::new(), cx);
                                            }),
                                    )
                                }),
                        )
                        // Filter Pills: All / Open / Closed
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .p_0p5()
                                .rounded_lg()
                                .bg(theme.muted.opacity(0.3))
                                .border_1()
                                .border_color(theme.border)
                                .child(
                                    Button::new("filter-pill-all")
                                        .variant(if current_filter == NotesFilterTab::All {
                                            ButtonVariant::Secondary
                                        } else {
                                            ButtonVariant::Ghost
                                        })
                                        .size(ButtonSize::Small)
                                        .label(format!("All ({all_count})"))
                                        .on_click(|_, _, cx| {
                                            let _ = Bridge::set_notes_library_filter(NotesFilterTab::All, cx);
                                        }),
                                )
                                .child(
                                    Button::new("filter-pill-open")
                                        .variant(if current_filter == NotesFilterTab::Open {
                                            ButtonVariant::Secondary
                                        } else {
                                            ButtonVariant::Ghost
                                        })
                                        .size(ButtonSize::Small)
                                        .label(format!("Open ({open_count})"))
                                        .on_click(|_, _, cx| {
                                            let _ = Bridge::set_notes_library_filter(NotesFilterTab::Open, cx);
                                        }),
                                )
                                .child(
                                    Button::new("filter-pill-closed")
                                        .variant(if current_filter == NotesFilterTab::Closed {
                                            ButtonVariant::Secondary
                                        } else {
                                            ButtonVariant::Ghost
                                        })
                                        .size(ButtonSize::Small)
                                        .label(format!("Closed ({closed_count})"))
                                        .on_click(|_, _, cx| {
                                            let _ = Bridge::set_notes_library_filter(NotesFilterTab::Closed, cx);
                                        }),
                                ),
                        )
                        // Sort selector button (cycles sort order)
                        .child(
                            Button::new("lib-sort-btn")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::ArrowsDownUp)
                                .label(current_sort.label())
                                .tooltip("Change sort order")
                                .on_click(move |_, _, cx| {
                                    let next_sort = match current_sort {
                                        NotesSortOption::UpdatedDesc => NotesSortOption::UpdatedAsc,
                                        NotesSortOption::UpdatedAsc => NotesSortOption::TitleAsc,
                                        NotesSortOption::TitleAsc => NotesSortOption::TitleDesc,
                                        NotesSortOption::TitleDesc => NotesSortOption::CreatedDesc,
                                        NotesSortOption::CreatedDesc => NotesSortOption::UpdatedDesc,
                                    };
                                    let _ = Bridge::set_notes_library_sort(next_sort, cx);
                                }),
                        ),
                )
                // Notes List / Grid
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .p_6()
                        .child(if filtered_notes.is_empty() {
                            render_library_empty_state(cx).into_any_element()
                        } else {
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .children(filtered_notes.into_iter().map(|note| {
                                    render_library_note_card(note, state, active_tab_id, cx)
                                }))
                                .into_any_element()
                        }),
                )
                // Footer
                .child(
                    div()
                        .w_full()
                        .px_6()
                        .py_3()
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_t_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.1))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("Showing {total_shown} of {all_count} saved notes")),
                        )
                        .child(
                            Button::new("lib-footer-done")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .label("Done")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::set_notes_library_open(false, cx);
                                }),
                        ),
                ),
        )
}

fn render_library_note_card(
    note: &NoteRecord,
    state: &BridgeState,
    active_tab_id: Option<&str>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let note_id = note.id.clone();
    let is_open_in_tab = state.notes_open_tab_ids.contains(&note.id);
    let is_active_tab = active_tab_id == Some(&note.id);
    let note_name = note.name.clone();

    let snippet = extract_snippet(&note.note, 130);
    let updated_str = format_relative_time(&note.updated_at);

    let id_for_open = note_id.clone();
    let id_for_dup = note_id.clone();
    let id_for_del = note_id.clone();
    let note_for_export = note.clone();
    let note_for_promote = note.clone();

    div()
        .id(format!("note-card-{}", note.id))
        .w_full()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(if is_active_tab {
            theme.primary.opacity(0.6)
        } else {
            theme.border
        })
        .bg(if is_active_tab {
            theme.primary.opacity(0.04)
        } else {
            theme.background
        })
        .hover(|this| {
            if !is_active_tab {
                this.bg(theme.muted.opacity(0.15))
            } else {
                this
            }
        })
        .flex()
        .flex_col()
        .gap_2()
        // Top row: Title + Badges + Open/Switch button
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id(format!("lib-note-title-{}", note.id))
                                .text_sm()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.foreground)
                                .cursor_pointer()
                                .hover(|this| this.text_color(theme.primary))
                                .on_click({
                                    let open_id = id_for_open.clone();
                                    move |_, _, cx| {
                                        let _ = Bridge::open_note(&open_id, cx);
                                    }
                                })
                                .child(note.name.clone()),
                        )
                        .when(is_active_tab, |this| {
                            this.child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .bg(theme.primary.opacity(0.15))
                                    .text_color(theme.primary)
                                    .child("Active Tab"),
                            )
                        })
                        .when(is_open_in_tab && !is_active_tab, |this| {
                            this.child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .bg(theme.muted.opacity(0.4))
                                    .text_color(theme.muted_foreground)
                                    .child("Open in Tab"),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("Updated {updated_str}")),
                        )
                        .child(
                            Button::new(format!("card-open-{}", note.id))
                                .variant(if is_open_in_tab {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Primary
                                })
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::ArrowSquareOut)
                                .label(if is_active_tab {
                                    "Current"
                                } else if is_open_in_tab {
                                    "Switch"
                                } else {
                                    "Open"
                                })
                                .on_click({
                                    let open_id = id_for_open;
                                    move |_, _, cx| {
                                        let _ = Bridge::open_note(&open_id, cx);
                                    }
                                }),
                        ),
                ),
        )
        // Snippet preview
        .child(
            div()
                .id(format!("lib-note-snippet-{}", note.id))
                .text_xs()
                .line_height(px(18.))
                .text_color(theme.muted_foreground)
                .cursor_pointer()
                .on_click({
                    let open_id = note_id.clone();
                    move |_, _, cx| {
                        let _ = Bridge::open_note(&open_id, cx);
                    }
                })
                .child(snippet),
        )
        // Bottom actions row
        .child(
            div()
                .w_full()
                .pt_2()
                .border_t_1()
                .border_color(theme.border.opacity(0.5))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new(format!("card-dup-{}", note.id))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Copy)
                                .tooltip("Duplicate Note")
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::duplicate_note(&id_for_dup, cx);
                                }),
                        )
                        .child(
                            Button::new(format!("card-export-{}", note.id))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::DownloadSimple)
                                .tooltip("Export (.md)")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        note_for_export.note.clone(),
                                    ));
                                    let _ = Bridge::state(cx).update(cx, |s| {
                                        s.apply_ok_msg(format!(
                                            "Exported \"{}\" to clipboard",
                                            note_for_export.name
                                        ));
                                    });
                                }),
                        )
                        .child(
                            Button::new(format!("card-promote-{}", note.id))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Brain)
                                .tooltip("Promote to Memory")
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::promote_note_to_memory(&note_for_promote, cx);
                                }),
                        )
                        .child(
                            Button::icon(
                                format!("card-trash-{}", note.id),
                                PhosphorIcon::Trash,
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .accessibility_label("Delete note")
                            .tooltip("Delete Note")
                            .on_click({
                                let name = note_name.clone();
                                move |_, window, cx| {
                                    confirm_delete(
                                        window,
                                        cx,
                                        "Delete note permanently?",
                                        format!(
                                            "\"{name}\" and its content will be permanently deleted. This cannot be undone."
                                        ),
                                        {
                                            let id = id_for_del.clone();
                                            move |_, cx| {
                                                let _ = Bridge::delete_note(&id, cx);
                                            }
                                        },
                                    );
                                }
                            }),
                        ),
                ),
        )
}

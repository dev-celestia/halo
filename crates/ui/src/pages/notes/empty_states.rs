//! Empty state displays for Notes: no open tabs, search miss, and library empty.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::Bridge;

use super::super::editors::ToolEditors;

/// Displayed when there are no note tabs currently open in the workspace.
pub fn render_notes_empty_state(total_notes: usize, cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .id("notes-empty-state")
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .p_8()
        .child(
            div()
                .size(px(64.))
                .rounded_2xl()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.primary.opacity(0.12))
                .border_1()
                .border_color(theme.primary.opacity(0.3))
                .text_color(theme.primary)
                .child(PhosphorIcon::FileText),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("No Note Tabs Open"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Create a new note or browse your saved notes library."),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("empty-new-note")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Plus)
                        .label("New Note")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::create_note(cx);
                        }),
                )
                .child(
                    Button::new("empty-open-library")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::FolderSimple)
                        .label(format!("Saved Notes Library ({total_notes})"))
                        .on_click(|_, _, cx| {
                            let _ = Bridge::set_notes_library_open(true, cx);
                        }),
                ),
        )
}

/// Displayed when open-tab search filter returns zero matches.
pub fn render_notes_search_empty(
    query: &str,
    total_notes: usize,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let search = editors.notes_search.clone();

    div()
        .id("notes-search-empty")
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .p_8()
        .child(
            div()
                .size(px(56.))
                .rounded_2xl()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.muted.opacity(0.5))
                .border_1()
                .border_color(theme.border)
                .text_color(theme.muted_foreground)
                .child(PhosphorIcon::MagnifyingGlass),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("No Matching Open Tabs"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("No open tabs match \"{query}\".")),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("search-clear-btn")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::X)
                        .label("Clear Search")
                        .on_click(move |_, window, cx| {
                            search.update(cx, |s, cx| s.set_value("", window, cx));
                            let _ = Bridge::set_notes_search(String::new(), cx);
                        }),
                )
                .child(
                    Button::new("search-open-library-btn")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::FolderSimple)
                        .label(format!("Saved Notes Library ({total_notes})"))
                        .on_click(|_, _, cx| {
                            let _ = Bridge::set_notes_library_open(true, cx);
                        }),
                ),
        )
}

/// Displayed inside the library modal dialog when filters yield no rows.
pub fn render_library_empty_state(cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .id("library-empty-state")
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .py_12()
        .child(
            div()
                .size(px(48.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.muted.opacity(0.4))
                .border_1()
                .border_color(theme.border)
                .text_color(theme.muted_foreground)
                .child(PhosphorIcon::FileText),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child("No notes found matching your criteria."),
        )
        .child(
            Button::new("lib-empty-new-note")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Plus)
                .label("Create New Note")
                .on_click(|_, _, cx| {
                    let _ = Bridge::create_note(cx);
                }),
        )
}

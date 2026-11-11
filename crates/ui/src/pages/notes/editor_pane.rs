//! Note editor pane supporting Editor, Split, and Preview view modes.

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::text::{TextView, TextViewStyle};
use halo_bridge::{BridgeState, NoteRecord, NotesViewMode};

use super::super::editors::ToolEditors;

pub fn render_notes_editor_pane(
    active_note: &NoteRecord,
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let view_mode = state.notes_view_mode;
    let note_body = active_note.note.clone();

    div()
        .id("notes-editor-pane")
        .size_full()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .bg(theme.background)
        .child(match view_mode {
            NotesViewMode::Edit => render_code_editor(editors).into_any_element(),
            NotesViewMode::Split => ResizablePanelGroup::horizontal("notes-editor-split")
                .with_handle(true)
                .child(
                    resizable_panel()
                        .size(px(520.))
                        .size_range(px(260.)..Pixels::MAX)
                        .child(render_code_editor(editors)),
                )
                .child(resizable_panel().child(render_markdown_preview(note_body, cx)))
                .into_any_element(),
            NotesViewMode::Preview => render_markdown_preview(note_body, cx).into_any_element(),
        })
}

fn render_code_editor(editors: &ToolEditors) -> impl IntoElement {
    div()
        .id("notes-monaco-wrapper")
        .size_full()
        .overflow_hidden()
        .child(editors.notes_editor.clone())
}

fn render_markdown_preview(content: String, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let display_text = if content.trim().is_empty() {
        "# Empty Note\n\nStart typing in the editor to see your formatted markdown preview here."
            .to_string()
    } else {
        content
    };

    let style = TextViewStyle::default().paragraph_gap(rems(0.75));

    div()
        .id("notes-preview-pane")
        .size_full()
        .overflow_y_scroll()
        .p_6()
        .bg(theme.background)
        .child(
            div().w_full().max_w(px(768.)).mx_auto().child(
                TextView::markdown("note-preview-text", display_text)
                    .style(style)
                    .selectable(true)
                    .w_full()
                    .min_w_0(),
            ),
        )
}

//! Raw HTTP wire inspector — view and edit the selected transaction, then act on it.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use gpui::*;
use halo_bridge::format_json_body;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_delete;
use crate::pages::chrome::panel;
use crate::pages::editors::ToolEditors;

pub fn render_request_panel(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let selected = state
        .selected_paused_id
        .as_deref()
        .and_then(|id| state.paused.iter().find(|row| row.id == id));

    let (title, subtitle) = if let Some(row) = selected {
        if row.has_response {
            (
                format!(
                    "Raw Response — {} {}",
                    row.status_code.unwrap_or(200),
                    row.status_text.as_deref().unwrap_or("OK")
                ),
                row.url.clone(),
            )
        } else {
            (
                format!("Raw Request — {} {}", row.method, row.host),
                row.url.clone(),
            )
        }
    } else {
        (
            "Raw Inspector".to_string(),
            "Select an item from the queue to view and edit its wire representation.".to_string(),
        )
    };

    let target_id = selected.map(|row| row.id.clone());
    let is_response = selected.map(|row| row.has_response).unwrap_or(false);
    let has_selection = target_id.is_some();
    let editor = editors.intercept_editor.clone();

    let mut actions = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            Button::new("editor-fwd-btn")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .label("Forward")
                .disabled(!has_selection)
                .on_click({
                    let editor = editor.clone();
                    let id = target_id.clone();
                    move |_, _, cx| {
                        if let Some(id) = &id {
                            let raw = editor.read(cx).text(cx);
                            let _ = Bridge::forward_paused_modified(id, &raw, false, cx);
                        }
                    }
                }),
        );

    if has_selection && !is_response {
        actions = actions.child(
            Button::new("editor-icp-res-btn")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .label("Intercept Response")
                .on_click({
                    let editor = editor.clone();
                    let id = target_id.clone();
                    move |_, _, cx| {
                        if let Some(id) = &id {
                            let raw = editor.read(cx).text(cx);
                            let _ = Bridge::forward_paused_modified(id, &raw, true, cx);
                        }
                    }
                }),
        );
    }

    actions = actions.child(
        Button::icon("editor-drop-btn", PhosphorIcon::Trash)
            .variant(ButtonVariant::Destructive)
            .size(ButtonSize::Small)
            .accessibility_label("Drop intercepted item")
            .tooltip("Drop")
            .disabled(!has_selection)
            .on_click({
                let id = target_id.clone();
                move |_, window, cx| {
                    let Some(id) = id.clone() else { return };
                    confirm_delete(
                        window,
                        cx,
                        "Drop intercepted item?",
                        "This item will be discarded and never forwarded. This cannot be undone.",
                        move |_, cx| {
                            let _ = Bridge::drop_paused(&id, cx);
                        },
                    );
                }
            }),
    );

    let action_bar = div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_2()
        .child(
            Button::new("intercept-format-btn")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Code)
                .label("Format Body")
                .disabled(!has_selection)
                .on_click({
                    let editor = editor.clone();
                    move |_, window, cx| {
                        let raw = editor.read(cx).text(cx);
                        let formatted = format_json_body(&raw);
                        if formatted != raw {
                            editor.update(cx, |e, cx| {
                                e.set_text(formatted, window, cx);
                            });
                        }
                    }
                }),
        )
        .child(actions);

    panel(
        title,
        Some(subtitle.into()),
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(action_bar)
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(editor),
            ),
        cx,
    )
}

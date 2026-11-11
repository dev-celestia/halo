//! Raw HTTP wire format editor panel for inspecting and modifying in-flight requests and responses.

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
    let selected_id = state.selected_paused_id.as_deref();
    let selected_row = selected_id.and_then(|id| state.paused.iter().find(|p| p.id == id));
    let editor = editors.intercept_editor.clone();

    let (title, subtitle, is_response, target_id) = if let Some(row) = selected_row {
        if row.has_response {
            (
                format!(
                    "Raw Response — {} {}",
                    row.status_code.unwrap_or(200),
                    row.status_text.as_deref().unwrap_or("OK")
                ),
                row.url.clone(),
                true,
                Some(row.id.clone()),
            )
        } else {
            (
                format!("Raw Request — {} {}", row.method, row.host),
                row.url.clone(),
                false,
                Some(row.id.clone()),
            )
        }
    } else {
        (
            "Raw Inspector".into(),
            "Select an item from the queue to view and edit its wire representation.".into(),
            false,
            None,
        )
    };

    let id_fwd = target_id.clone();
    let id_icp_res = target_id.clone();
    let id_drop = target_id.clone();

    let action_bar = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(
            Button::new("intercept-format-btn")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Code)
                .label("Format Body")
                .disabled(selected_row.is_none())
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
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .children(if let Some(id) = id_fwd {
                    vec![
                        Button::new("editor-fwd-btn")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Small)
                            .label("Forward")
                            .on_click({
                                let editor = editor.clone();
                                move |_, _, cx| {
                                    let raw = editor.read(cx).text(cx);
                                    let _ = Bridge::forward_paused_modified(&id, &raw, false, cx);
                                }
                            })
                            .into_any_element(),
                    ]
                } else {
                    vec![]
                })
                .children(if !is_response && id_icp_res.is_some() {
                    let id = id_icp_res.unwrap();
                    vec![
                        Button::new("editor-icp-res-btn")
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Small)
                            .label("Intercept Response")
                            .on_click({
                                let editor = editor.clone();
                                move |_, _, cx| {
                                    let raw = editor.read(cx).text(cx);
                                    let _ = Bridge::forward_paused_modified(&id, &raw, true, cx);
                                }
                            })
                            .into_any_element(),
                    ]
                } else {
                    vec![]
                })
                .children(if let Some(id) = id_drop {
                    vec![
                        Button::icon("editor-drop-btn", PhosphorIcon::Trash)
                            .variant(ButtonVariant::Destructive)
                            .size(ButtonSize::Small)
                            .accessibility_label("Drop intercepted item")
                            .tooltip("Drop")
                            .on_click(move |_, window, cx| {
                                let id = id.clone();
                                confirm_delete(
                                    window,
                                    cx,
                                    "Drop intercepted item?",
                                    "This item will be discarded and never forwarded. This cannot be undone.",
                                    move |_, cx| {
                                        let _ = Bridge::drop_paused(&id, cx);
                                    },
                                );
                            })
                            .into_any_element(),
                    ]
                } else {
                    vec![]
                }),
        );

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

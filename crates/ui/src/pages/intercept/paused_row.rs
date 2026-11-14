//! A single row in the paused queue: summary slots plus per-item actions.

use celestia_ui::components::primitive::badge::{Badge, BadgeVariant};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::caption::Caption;
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::item::{
    Item, ItemContent, ItemDescription, ItemFooter, ItemHeader, ItemSize, ItemTitle, ItemVariant,
};
use gpui::*;
use halo_bridge::{Bridge, PausedRow};

use crate::components::confirm_delete;
use crate::pages::editors::ToolEditors;

/// Method badge tone — stock variants only, no ad-hoc colours.
fn method_badge(method: &str) -> Badge {
    let variant = match method.to_ascii_uppercase().as_str() {
        "GET" => BadgeVariant::Secondary,
        "POST" | "PUT" | "PATCH" => BadgeVariant::Info,
        "DELETE" => BadgeVariant::Destructive,
        _ => BadgeVariant::Outline,
    };
    Badge::new(method.to_string()).variant(variant)
}

fn time_label(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .map(|time| {
            time.trim_end_matches('Z')
                .get(..8)
                .unwrap_or(time)
                .to_string()
        })
        .unwrap_or_else(|| timestamp.to_string())
}

pub(super) fn render_paused_row(
    row: &PausedRow,
    is_selected: bool,
    editors: &ToolEditors,
    _cx: &App,
) -> impl IntoElement {
    let id = row.id.clone();
    let select_id = id.clone();
    let has_response = row.has_response;
    let host = row.host.clone();
    let raw_request = row.raw_request.clone();
    let raw_response = row.raw_response.clone();
    let editor = editors.intercept_editor.clone();

    let (status, status_variant) = if let Some(code) = row.status_code {
        (
            format!("Response {code} Paused"),
            if code < 400 {
                BadgeVariant::Success
            } else {
                BadgeVariant::Destructive
            },
        )
    } else if has_response {
        ("Response Paused".to_string(), BadgeVariant::Info)
    } else {
        ("Request Paused".to_string(), BadgeVariant::Warning)
    };

    // Each row owns its actions so a queued request can be acted on directly.
    // Only the selected row uses the live editor buffer; every other row acts
    // on its own captured wire text (the editor may still hold another row).
    // Left-aligned so a wrapped second line stays flush with the title above
    // instead of being spread across the row by the footer's `justify_between`.
    let mut footer = ItemFooter::new().justify_start().flex_wrap().child(
        Button::new(SharedString::from(format!("fwd-{id}")))
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Small)
            .label("Forward")
            .on_click({
                let editor = editor.clone();
                let raw_request = raw_request.clone();
                let raw_response = raw_response.clone();
                let id = id.clone();
                move |_, _, cx| {
                    let raw_text = if is_selected {
                        editor.read(cx).text(cx)
                    } else if has_response {
                        raw_response.clone().unwrap_or_else(|| raw_request.clone())
                    } else {
                        raw_request.clone()
                    };
                    let _ = Bridge::forward_paused_modified(&id, &raw_text, false, cx);
                }
            }),
    );

    if !has_response {
        footer = footer.child(
            Button::new(SharedString::from(format!("icp-res-{id}")))
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .label("Intercept Response")
                .on_click({
                    let editor = editor.clone();
                    let raw_request = raw_request.clone();
                    let id = id.clone();
                    move |_, _, cx| {
                        let raw_text = if is_selected {
                            editor.read(cx).text(cx)
                        } else {
                            raw_request.clone()
                        };
                        let _ = Bridge::forward_paused_modified(&id, &raw_text, true, cx);
                    }
                }),
        );
    }

    footer = footer
        .child(
            Button::icon(
                SharedString::from(format!("drop-{id}")),
                PhosphorIcon::Trash,
            )
            .variant(ButtonVariant::Destructive)
            .size(ButtonSize::Small)
            .accessibility_label("Drop intercepted item")
            .tooltip("Drop")
            .on_click({
                let id = id.clone();
                move |_, window, cx| {
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
                }
            }),
        )
        .child(
            Button::icon(
                SharedString::from(format!("dc-{id}")),
                PhosphorIcon::EyeSlash,
            )
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Small)
            .accessibility_label("Don't capture this host")
            .tooltip("Don't Capture")
            .on_click({
                let id = id.clone();
                let host = host.clone();
                move |_, _, cx| {
                    let _ = Bridge::dont_capture_host(&id, &host, cx);
                }
            }),
        )
        .child(
            Button::icon(
                SharedString::from(format!("rep-{id}")),
                PhosphorIcon::Repeat,
            )
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Small)
            .accessibility_label("Send to Repeater")
            .tooltip("Repeater")
            .on_click({
                let id = id.clone();
                move |_, _, cx| {
                    let _ = Bridge::send_paused_to_repeater(&id, cx);
                }
            }),
        );

    Item::new()
        .id(SharedString::from(format!("paused-{}", row.id)))
        .variant(if is_selected {
            ItemVariant::Muted
        } else {
            ItemVariant::Outline
        })
        .size(ItemSize::Sm)
        .on_click(move |_, _, cx| {
            Bridge::select_paused(Some(select_id.clone()), cx);
        })
        .child(
            ItemHeader::new()
                .child(method_badge(&row.method))
                .child(Badge::new(status).variant(status_variant))
                .child(Caption::new(time_label(&row.timestamp))),
        )
        .child(
            ItemContent::new()
                .child(ItemTitle::new(format!("{} {}", row.host, row.path)).truncate())
                .child(ItemDescription::new(row.url.clone()).truncate()),
        )
        .child(footer)
}

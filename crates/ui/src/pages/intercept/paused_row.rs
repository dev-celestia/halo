//! Single row item in intercepted paused requests queue.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, PausedRow};

use crate::components::confirm_delete;
use crate::pages::colors::method_color;
use crate::pages::editors::ToolEditors;

pub fn render_paused_row(
    row: &PausedRow,
    is_selected: bool,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let border = if is_selected {
        theme.primary
    } else {
        theme.border
    };
    let bg = if is_selected {
        Some(theme.accent)
    } else {
        None
    };

    let id = row.id.clone();
    let id_fwd = row.id.clone();
    let id_icp_res = row.id.clone();
    let id_drop = row.id.clone();
    let id_dc = row.id.clone();
    let id_rep = row.id.clone();
    let host = row.host.clone();
    let has_response = row.has_response;
    let editor = editors.intercept_editor.clone();

    div()
        .w_full()
        .p_3()
        .rounded(theme.radius)
        .border_1()
        .border_color(border)
        .when_some(bg, |this, color| this.bg(color))
        .flex()
        .flex_col()
        .gap_2()
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, {
            let id = id.clone();
            move |_, _, cx| {
                Bridge::select_paused(Some(id.clone()), cx);
            }
        })
        .child(
            // Header line: method badge + status badge + time
            div()
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
                                .px_1p5()
                                .py_0p5()
                                .rounded(theme.radius_sm())
                                .bg(theme.background)
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(method_color(&row.method))
                                .child(row.method.clone()),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(theme.radius_sm())
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(if has_response {
                                    theme.primary
                                } else {
                                    theme.warning
                                })
                                .child(if let Some(status) = row.status_code {
                                    format!("Response {status} Paused")
                                } else if has_response {
                                    "Response Paused".into()
                                } else {
                                    "Request Paused".into()
                                }),
                        ),
                )
                .child(div().text_xs().text_color(muted).child(
                    if let Some(time) = row.timestamp.split('T').nth(1) {
                        time.trim_end_matches('Z')
                            .get(..8)
                            .unwrap_or(time)
                            .to_string()
                    } else {
                        row.timestamp.clone()
                    },
                )),
        )
        .child(
            // Host & URL line
            div()
                .w_full()
                .min_w_0()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(format!("{} {}", row.host, row.path)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(row.url.clone()),
                ),
        )
        .child(
            // Action buttons row
            div()
                .flex()
                .flex_wrap()
                .gap_1p5()
                .items_center()
                .child(
                    Button::new(SharedString::from(format!("fwd-{id_fwd}")))
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .label("Forward")
                        .on_click({
                            let editor = editor.clone();
                            let raw_request = row.raw_request.clone();
                            let raw_response = row.raw_response.clone();
                            move |_, _, cx| {
                                let raw_text = if is_selected {
                                    editor.read(cx).text(cx)
                                } else if has_response {
                                    raw_response.clone().unwrap_or(raw_request.clone())
                                } else {
                                    raw_request.clone()
                                };
                                let _ =
                                    Bridge::forward_paused_modified(&id_fwd, &raw_text, false, cx);
                            }
                        }),
                )
                .children(if !has_response {
                    vec![
                        Button::new(SharedString::from(format!("icp-res-{id_icp_res}")))
                            .variant(ButtonVariant::Secondary)
                            .size(ButtonSize::Small)
                            .label("Intercept Response")
                            .on_click({
                                let editor = editor.clone();
                                let raw_request = row.raw_request.clone();
                                move |_, _, cx| {
                                    let raw_text = if is_selected {
                                        editor.read(cx).text(cx)
                                    } else {
                                        raw_request.clone()
                                    };
                                    let _ = Bridge::forward_paused_modified(
                                        &id_icp_res,
                                        &raw_text,
                                        true,
                                        cx,
                                    );
                                }
                            })
                            .into_any_element(),
                    ]
                } else {
                    vec![]
                })
                .child(
                    Button::icon(
                        SharedString::from(format!("drop-{id_drop}")),
                        PhosphorIcon::Trash,
                    )
                    .variant(ButtonVariant::Destructive)
                    .size(ButtonSize::Small)
                    .accessibility_label("Drop intercepted item")
                    .tooltip("Drop")
                    .on_click(move |_, window, cx| {
                        let id = id_drop.clone();
                        confirm_delete(
                            window,
                            cx,
                            "Drop intercepted item?",
                            "This item will be discarded and never forwarded. This cannot be undone.",
                            move |_, cx| {
                                let _ = Bridge::drop_paused(&id, cx);
                            },
                        );
                    }),
                )
                .child(
                    Button::new(SharedString::from(format!("dc-{id_dc}")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Don't Capture")
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::dont_capture_host(&id_dc, &host, cx);
                        }),
                )
                .child(
                    Button::new(SharedString::from(format!("rep-{id_rep}")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::PaperPlaneTilt)
                        .label("Repeater")
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::send_paused_to_repeater(&id_rep, cx);
                        }),
                ),
        )
}

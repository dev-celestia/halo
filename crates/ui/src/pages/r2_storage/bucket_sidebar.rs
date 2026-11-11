//! Bucket list sidebar and bucket rows.

use super::*;

pub(super) fn render_bucket_sidebar(r2: &R2View, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w(px(210.))
        .flex_none()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .px_2p5()
                .py_1p5()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.4))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child("Buckets"),
                )
                .child(
                    div()
                        .px_1p5()
                        .rounded(theme.radius_sm())
                        .border_1()
                        .border_color(theme.border)
                        .text_xs()
                        .font_family("Menlo")
                        .child(r2.buckets.len().to_string()),
                ),
        )
        .child(
            div()
                .id("r2-buckets")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap_0p5()
                .p_1p5()
                .children(if r2.buckets.is_empty() {
                    vec![
                        div()
                            .px_2()
                            .py_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(if r2.loading {
                                "Loading buckets…"
                            } else {
                                "No buckets yet."
                            })
                            .into_any_element(),
                    ]
                } else {
                    r2.buckets
                        .iter()
                        .map(|name| bucket_row(name, r2.bucket.as_deref(), cx))
                        .collect()
                }),
        )
        .child(
            div()
                .p_2()
                .border_t_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Create bucket"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(editors.r2_bucket_name.clone()),
                        )
                        .child(
                            Button::new("r2-create-bucket")
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Plus)
                                .disabled(r2.busy)
                                .on_click({
                                    let input = editors.r2_bucket_name.clone();
                                    move |_, window, cx| {
                                        let name = input.read(cx).value().to_string();
                                        match Bridge::r2_create_bucket(&name, cx) {
                                            Ok(()) => {
                                                input.update(cx, |s, cx| {
                                                    s.set_value("", window, cx)
                                                });
                                            }
                                            Err(err) => {
                                                Bridge::state(cx).update(cx, |s| s.apply_err(err));
                                            }
                                        }
                                    }
                                }),
                        ),
                ),
        )
        .into_any_element()
}

pub(super) fn bucket_row(name: &str, current: Option<&str>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let selected = current == Some(name);
    let open = name.to_string();
    let delete = name.to_string();

    div()
        .id(SharedString::from(format!("r2-bucket-{name}")))
        .w_full()
        .flex()
        .items_center()
        .gap_1()
        .rounded(theme.radius_sm())
        .hover(|s| s.bg(theme.muted.opacity(0.4)))
        .child(
            div()
                .id(SharedString::from(format!("r2-bucket-open-{name}")))
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_1p5()
                .px_2()
                .py_1p5()
                .rounded(theme.radius_sm())
                .cursor_pointer()
                .bg(if selected {
                    theme.primary.opacity(0.1)
                } else {
                    theme.background
                })
                .on_click(move |_, _, cx| {
                    let _ = Bridge::r2_open_bucket(&open, cx);
                })
                .child(
                    div()
                        .text_color(if selected {
                            theme.primary
                        } else {
                            theme.muted_foreground
                        })
                        .child(PhosphorIcon::Cube),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child(name.to_string()),
                ),
        )
        .child(
            Button::icon(
                SharedString::from(format!("r2-bucket-del-{name}")),
                PhosphorIcon::Trash,
            )
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Small)
            .accessibility_label("Delete bucket")
            .tooltip("Delete bucket")
            .on_click({
                let name = delete.clone();
                move |_, window, cx| {
                    let name = name.clone();
                    confirm_delete(
                        window,
                        cx,
                        "Delete bucket?",
                        format!(
                            "\"{name}\" must be empty before it can be deleted. This cannot be undone."
                        ),
                        move |_, cx| {
                            let _ = Bridge::r2_delete_bucket(&name, cx);
                        },
                    );
                }
            }),
        )
        .into_any_element()
}

// ── objects ────────────────────────────────────────────────────────────────

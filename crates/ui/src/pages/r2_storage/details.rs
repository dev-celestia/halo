//! Object details pane: metadata rows, presign, public URL, upload card.

use super::*;

pub(super) fn render_details(r2: &R2View, cx: &App) -> AnyElement {
    let selected = r2
        .selected_key
        .as_ref()
        .and_then(|key| r2.files.iter().find(|file| &file.key == key))
        .cloned();

    let content: AnyElement = match selected {
        Some(file) => file_details(r2, &file, cx),
        None => div()
            .size_full()
            .child(empty_state(
                "No object selected",
                "Select a file to see its details and actions.",
                cx,
            ))
            .into_any_element(),
    };

    div()
        .w(px(300.))
        .flex_none()
        .h_full()
        .min_h_0()
        .child(panel("Details", None, content, cx))
        .into_any_element()
}

pub(super) fn file_details(r2: &R2View, file: &R2FileEntry, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let key = file.key.clone();
    let public = public_url(r2, &key);

    div()
        .id("r2-details")
        .size_full()
        .overflow_y_scrollbar()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(PhosphorIcon::FileText),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child(file.name.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(detail_row("Key", &file.key, cx))
                .child(detail_row("Size", &size_label(file.size), cx))
                .child(detail_row(
                    "Modified",
                    &modified_label(file.last_modified_ms),
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.muted_foreground)
                        .child("Actions"),
                )
                .child(
                    Button::new("r2-detail-download")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::DownloadSimple)
                        .label("Download & Open")
                        .on_click({
                            let key = key.clone();
                            move |_, _, cx| {
                                let _ = Bridge::r2_download_and_open(&key, cx);
                            }
                        }),
                )
                .child(
                    Button::new("r2-detail-copy-url")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Copy)
                        .label("Copy Public URL")
                        .disabled(public.is_none())
                        .on_click({
                            let key = key.clone();
                            move |_, _, cx| copy_public_url(&key, cx)
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Presigned URL"),
                        )
                        .child(
                            div().flex().items_center().gap_1().children(
                                [
                                    ("15m", 900u64),
                                    ("1h", 3_600),
                                    ("24h", 86_400),
                                    ("7d", 604_800),
                                ]
                                .into_iter()
                                .map(|(label, seconds)| presign_button(&key, label, seconds)),
                            ),
                        ),
                )
                .child(
                    Button::new("r2-detail-delete")
                        .variant(ButtonVariant::Destructive)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Trash)
                        .label("Delete")
                        .on_click({
                            let key = key.clone();
                            let name = file.name.clone();
                            move |_, window, cx| {
                                let key = key.clone();
                                confirm_delete(
                                    window,
                                    cx,
                                    "Delete object?",
                                    format!("\"{name}\" will be permanently deleted."),
                                    move |_, cx| {
                                        let _ = Bridge::r2_delete_object(&key, cx);
                                    },
                                );
                            }
                        }),
                ),
        )
        .into_any_element()
}

pub(super) fn detail_row(label: &str, value: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_xs()
                .font_family("Menlo")
                .child(value.to_string()),
        )
        .into_any_element()
}

pub(super) fn presign_button(key: &str, label: &str, seconds: u64) -> Button {
    let key = key.to_string();
    let label = label.to_string();
    let button_label = label.clone();
    Button::new(SharedString::from(format!("r2-presign-{seconds}")))
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Small)
        .label(button_label)
        .on_click(
            move |_, _, cx| match Bridge::r2_presign_url(&key, seconds, cx) {
                Ok(url) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(url));
                    Bridge::state(cx).update(cx, |s| {
                        s.apply_ok_msg(format!("Presigned URL copied ({label})"));
                    });
                }
                Err(err) => {
                    Bridge::state(cx).update(cx, |s| s.apply_err(err));
                }
            },
        )
}

pub(super) fn copy_public_url(key: &str, cx: &mut App) {
    let url = {
        let state = Bridge::state(cx);
        let state = state.read(cx);
        public_url(&state.r2, key)
    };
    match url {
        Some(url) => {
            cx.write_to_clipboard(ClipboardItem::new_string(url));
            Bridge::state(cx).update(cx, |s| {
                s.apply_ok_msg("Public URL copied".into());
            });
        }
        None => {
            Bridge::state(cx).update(cx, |s| {
                s.apply_err(halo_bridge::BridgeError::message(
                    "No public endpoint for this bucket",
                ));
            });
        }
    }
}

pub(super) fn public_url(r2: &R2View, key: &str) -> Option<String> {
    let credentials = r2.credentials.as_ref()?;
    let bucket = r2.bucket.as_deref()?;
    match credentials.custom_endpoint_url.as_deref() {
        Some(endpoint) if !endpoint.trim().is_empty() => Some(format!(
            "{}/{}/{}",
            endpoint.trim_end_matches('/'),
            bucket,
            key
        )),
        _ => Some(format!(
            "https://{}.r2.cloudflarestorage.com/{}/{}",
            credentials.account_id, bucket, key
        )),
    }
}

// ── transfer status ────────────────────────────────────────────────────────

pub(super) fn upload_card(upload: &R2UploadProgress, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .absolute()
        .right(px(16.))
        .bottom(px(16.))
        .w(px(280.))
        .p_3()
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_color(theme.primary)
                        .child(PhosphorIcon::UploadSimple),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child(format!("Uploading {}", upload.file_name)),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family("Menlo")
                        .text_color(theme.muted_foreground)
                        .child(format!("{}%", upload.progress)),
                ),
        )
        .child(
            div()
                .h(px(4.))
                .w_full()
                .rounded_full()
                .bg(theme.muted)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(theme.primary)
                        .w(relative(upload.progress.min(100) as f32 / 100.0)),
                ),
        )
        .into_any_element()
}

pub(super) fn size_label(size: Option<i64>) -> String {
    size.map(|bytes| format_bytes(bytes.max(0) as u64))
        .unwrap_or_else(|| "—".into())
}

pub(super) fn modified_label(ms: Option<i64>) -> String {
    use chrono::{Local, TimeZone as _};
    let Some(ms) = ms else {
        return "—".into();
    };
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".into())
}

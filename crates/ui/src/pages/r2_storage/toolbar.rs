//! Workspace frame and the R2 toolbar (breadcrumb, search, view mode).

use super::*;

pub(super) fn render_workspace(r2: &R2View, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(
            div()
                .w_full()
                .flex_none()
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .child(render_toolbar(r2, editors, cx)),
        )
        .children(r2.error.as_ref().map(|message| error_banner(message, cx)))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .flex()
                .gap(px(PAGE_GAP))
                .overflow_hidden()
                .child(render_bucket_sidebar(r2, editors, cx))
                .child(render_objects(r2, cx))
                .child(render_details(r2, cx)),
        )
        .into_any_element()
}

// ── toolbar ────────────────────────────────────────────────────────────────

pub(super) fn render_toolbar(r2: &R2View, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let breadcrumbs = r2.breadcrumbs();
    let at_root = r2.prefix.is_empty();
    let has_bucket = r2.bucket.is_some();

    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .child(
            Button::new("r2-up")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowLeft)
                .disabled(at_root || r2.loading)
                .on_click(|_, _, cx| {
                    let _ = Bridge::r2_navigate_up(cx);
                }),
        )
        .children(breadcrumbs.iter().enumerate().map(|(i, (label, prefix))| {
            let is_last = i + 1 == breadcrumbs.len();
            let prefix = prefix.clone();
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(if i > 0 {
                    div()
                        .text_color(theme.muted_foreground)
                        .child(PhosphorIcon::CaretRight)
                        .into_any_element()
                } else {
                    div().into_any_element()
                })
                .child(if is_last {
                    div()
                        .max_w(px(180.))
                        .truncate()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(label.clone())
                        .into_any_element()
                } else {
                    Button::new(SharedString::from(format!("r2-crumb-{i}")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label(label.clone())
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::r2_navigate(&prefix, cx);
                        })
                        .into_any_element()
                })
                .into_any_element()
        }))
        .when(breadcrumbs.is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Select a bucket"),
            )
        })
        .child(div().flex_1())
        .children(if r2.creating {
            vec![
                editors.r2_folder_name.clone().into_any_element(),
                Button::new("r2-folder-confirm")
                    .variant(ButtonVariant::Primary)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::Check)
                    .on_click({
                        let input = editors.r2_folder_name.clone();
                        move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            match Bridge::r2_create_folder(&name, cx) {
                                Ok(()) => {
                                    input.update(cx, |s, cx| s.set_value("", window, cx));
                                    Bridge::state(cx).update(cx, |s| {
                                        s.r2.creating = false;
                                    });
                                }
                                Err(err) => {
                                    Bridge::state(cx).update(cx, |s| s.apply_err(err));
                                }
                            }
                        }
                    })
                    .into_any_element(),
                Button::new("r2-folder-cancel")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::X)
                    .on_click(|_, _, cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.r2.creating = false;
                        });
                    })
                    .into_any_element(),
            ]
        } else {
            vec![
                Button::new("r2-new-folder")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::FolderPlus)
                    .label("New Folder")
                    .disabled(!has_bucket)
                    .on_click(|_, _, cx| {
                        Bridge::state(cx).update(cx, |s| {
                            s.r2.creating = true;
                        });
                    })
                    .into_any_element(),
            ]
        })
        .child(
            Button::new("r2-upload")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::UploadSimple)
                .label("Upload")
                .disabled(!has_bucket || r2.busy)
                .on_click(|_, window, cx| {
                    let options = PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: true,
                        prompt: Some("Upload files to R2".into()),
                    };
                    let dialog = cx.prompt_for_paths(options);
                    cx.spawn(async move |cx: &mut AsyncApp| {
                        if let Ok(Ok(Some(paths))) = dialog.await {
                            let _ = cx.update(|cx| {
                                for path in paths {
                                    if let Err(err) = Bridge::r2_upload(path, cx) {
                                        Bridge::state(cx).update(cx, |s| s.apply_err(err));
                                    }
                                }
                            });
                        }
                        let _ = window;
                    })
                    .detach();
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .rounded(theme.radius_sm())
                .border_1()
                .border_color(theme.border)
                .p(px(1.))
                .child(view_button(
                    "r2-view-list",
                    "List",
                    PhosphorIcon::List,
                    ExplorerViewMode::List,
                    r2.view_mode,
                ))
                .child(view_button(
                    "r2-view-grid",
                    "Grid",
                    PhosphorIcon::SquaresFour,
                    ExplorerViewMode::Grid,
                    r2.view_mode,
                )),
        )
        .child(div().w(px(170.)).child(editors.r2_search.clone()))
        .child(
            Button::new("r2-refresh")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .disabled(!has_bucket)
                .on_click(|_, _, cx| {
                    let _ = Bridge::r2_refresh(cx);
                }),
        )
        .into_any_element()
}

pub(super) fn view_button(
    id: &'static str,
    label: &'static str,
    icon: PhosphorIcon,
    mode: ExplorerViewMode,
    current: ExplorerViewMode,
) -> Button {
    let active = mode == current;
    Button::new(id)
        .variant(if active {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        })
        .size(ButtonSize::Small)
        .leading_icon(icon)
        .label(label)
        .on_click(move |_, _, cx| Bridge::r2_set_view_mode(mode, cx))
}

// ── bucket sidebar ─────────────────────────────────────────────────────────

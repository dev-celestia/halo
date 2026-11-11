//! Object list: selection key handling, rows, and cards.

use super::*;

pub(super) fn object_entries(r2: &R2View) -> Vec<(bool, String)> {
    let mut entries: Vec<(bool, String)> = r2
        .filtered_folders()
        .into_iter()
        .map(|folder| (true, folder.prefix))
        .collect();
    entries.extend(
        r2.filtered_files()
            .into_iter()
            .map(|file| (false, file.key)),
    );
    entries
}

pub(super) fn on_objects_key(event: &KeyDownEvent, _: &mut Window, cx: &mut App) {
    let keystroke = &event.keystroke;
    if keystroke.modifiers.alt
        || keystroke.modifiers.control
        || keystroke.modifiers.shift
        || keystroke.modifiers.platform
    {
        return;
    }
    let (current, entries) = {
        let state = Bridge::state(cx);
        let state = state.read(cx);
        (state.r2.selected_key.clone(), object_entries(&state.r2))
    };
    match keystroke.key.as_str() {
        "down" => move_selection(&entries, current, 1, cx),
        "up" => move_selection(&entries, current, -1, cx),
        "enter" => {
            if let Some(selected) = &current {
                match entries.iter().find(|(_, value)| value == selected) {
                    Some((true, value)) => {
                        let _ = Bridge::r2_navigate(value, cx);
                    }
                    Some((false, value)) => {
                        let _ = Bridge::r2_download_and_open(value, cx);
                    }
                    None => {}
                }
            }
        }
        "escape" => Bridge::r2_select(None, cx),
        _ => return,
    }
    cx.stop_propagation();
}

pub(super) fn move_selection(
    entries: &[(bool, String)],
    current: Option<String>,
    delta: isize,
    cx: &mut App,
) {
    let Some(index) = entries
        .iter()
        .position(|(_, value)| Some(value) == current.as_ref())
    else {
        if let Some((_, value)) = entries.first() {
            Bridge::r2_select(Some(value.clone()), cx);
        }
        return;
    };
    let next = (index as isize + delta).clamp(0, entries.len() as isize - 1) as usize;
    if let Some((_, value)) = entries.get(next) {
        Bridge::r2_select(Some(value.clone()), cx);
    }
}

pub(super) fn render_objects(r2: &R2View, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let folders = r2.filtered_folders();
    let files = r2.filtered_files();
    let selected = r2.selected_key.clone();

    let empty = if r2.bucket.is_none() {
        Some(
            empty_state(
                "No bucket selected",
                "Pick a bucket in the sidebar or create one.",
                cx,
            )
            .into_any_element(),
        )
    } else if r2.loading {
        Some(centered_hint("Loading objects…", cx))
    } else if folders.is_empty() && files.is_empty() {
        Some(
            empty_state(
                "Empty prefix",
                "Upload files or create a folder to get started.",
                cx,
            )
            .into_any_element(),
        )
    } else {
        None
    };

    let body: AnyElement = match empty {
        Some(empty) => div().size_full().child(empty).into_any_element(),
        None => {
            let mut list = div()
                .id("r2-objects")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .on_key_down(on_objects_key)
                .flex()
                .gap_2();
            list = match r2.view_mode {
                ExplorerViewMode::List => list.flex_col(),
                ExplorerViewMode::Grid => list.flex_wrap().p_2(),
            };
            match r2.view_mode {
                ExplorerViewMode::List => list
                    .child(list_header(cx))
                    .children(folders.iter().map(|folder| folder_row(folder, cx)))
                    .children(
                        files
                            .iter()
                            .map(|file| file_row(file, selected.as_deref(), cx)),
                    )
                    .into_any_element(),
                ExplorerViewMode::Grid => list
                    .children(folders.iter().map(|folder| folder_card(folder, cx)))
                    .children(
                        files
                            .iter()
                            .map(|file| file_card(file, selected.as_deref(), cx)),
                    )
                    .into_any_element(),
            }
        }
    };

    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .w_full()
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_2p5()
                .py_1p5()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.4))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child(match &r2.bucket {
                            Some(bucket) => format!("{bucket}{}", r2.prefix),
                            None => "Objects".into(),
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family("Menlo")
                        .text_color(theme.muted_foreground)
                        .child(format!(
                            "{} folders · {} objects",
                            folders.len(),
                            files.len()
                        )),
                ),
        )
        .child(body)
        .into_any_element()
}

pub(super) fn list_header(cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(theme.muted_foreground)
        .bg(theme.muted.opacity(0.5))
        .child(div().flex_1().child("NAME"))
        .child(div().w(px(90.)).child("SIZE"))
        .child(div().w(px(130.)).child("MODIFIED"))
        .child(div().w(px(70.)))
        .into_any_element()
}

pub(super) fn folder_row(folder: &R2FolderEntry, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let prefix = folder.prefix.clone();

    div()
        .id(SharedString::from(format!("r2-folder-{}", folder.prefix)))
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .font_family("Menlo")
        .text_xs()
        .border_b_1()
        .border_color(theme.border.opacity(0.4))
        .bg(theme.background)
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted.opacity(0.3)))
        .on_click(move |_, _, cx| {
            let _ = Bridge::r2_navigate(&prefix, cx);
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_color(hsla(38. / 360., 0.9, 0.5, 1.))
                        .child(PhosphorIcon::Folder),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(folder.name.clone()),
                ),
        )
        .child(
            div()
                .w(px(90.))
                .text_color(theme.muted_foreground)
                .child("—"),
        )
        .child(
            div()
                .w(px(130.))
                .text_color(theme.muted_foreground)
                .child("—"),
        )
        .child(div().w(px(70.)))
        .into_any_element()
}

pub(super) fn file_row(file: &R2FileEntry, selected: Option<&str>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(file.key.as_str());
    let key = file.key.clone();
    let key_for_open = file.key.clone();
    let key_for_menu = file.key.clone();
    let delete_key = file.key.clone();
    let delete_name = file.name.clone();

    div()
        .id(SharedString::from(format!("r2-row-{}", file.key)))
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .font_family("Menlo")
        .text_xs()
        .border_b_1()
        .border_color(theme.border.opacity(0.4))
        .bg(if is_selected {
            theme.primary.opacity(0.1)
        } else {
            theme.background
        })
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted.opacity(0.3)))
        .on_click({
            let key = key.clone();
            move |_, _, cx| Bridge::r2_select(Some(key.clone()), cx)
        })
        .on_mouse_down(MouseButton::Left, {
            let key_for_open = key_for_open.clone();
            move |event: &MouseDownEvent, _, cx| {
                if event.click_count == 2 {
                    let _ = Bridge::r2_download_and_open(&key_for_open, cx);
                }
            }
        })
        .on_mouse_down(MouseButton::Right, {
            let key = key.clone();
            move |_, _, cx| Bridge::r2_select(Some(key.clone()), cx)
        })
        .context_menu(move |mut menu, _, _| {
            let open_key = key_for_menu.clone();
            let copy_key = key_for_menu.clone();
            let delete_key = delete_key.clone();
            let delete_name = delete_name.clone();
            menu = menu.item(
                PopupMenuItem::new("Download & Open").on_click(move |_, _, cx| {
                    let _ = Bridge::r2_download_and_open(&open_key, cx);
                }),
            );
            menu = menu.item(
                PopupMenuItem::new("Copy Public URL").on_click(move |_, _, cx| {
                    copy_public_url(&copy_key, cx);
                }),
            );
            menu = menu.separator();
            menu = menu.item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                confirm_delete(
                    window,
                    cx,
                    "Delete object?",
                    format!("\"{delete_name}\" will be permanently deleted."),
                    {
                        let key = delete_key.clone();
                        move |_, cx| {
                            let _ = Bridge::r2_delete_object(&key, cx);
                        }
                    },
                );
            }));
            menu
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(PhosphorIcon::FileText),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(theme.foreground)
                        .child(file.name.clone()),
                ),
        )
        .child(
            div()
                .w(px(90.))
                .text_color(theme.muted_foreground)
                .child(size_label(file.size)),
        )
        .child(
            div()
                .w(px(130.))
                .text_color(theme.muted_foreground)
                .child(modified_label(file.last_modified_ms)),
        )
        .child(
            div()
                .w(px(70.))
                .flex()
                .items_center()
                .justify_end()
                .gap_1()
                .child(
                    Button::icon(
                        SharedString::from(format!("r2-dl-{}", file.key)),
                        PhosphorIcon::DownloadSimple,
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .accessibility_label("Download and open")
                    .tooltip("Download & Open")
                    .on_click({
                        let key = file.key.clone();
                        move |_, _, cx| {
                            let _ = Bridge::r2_download_and_open(&key, cx);
                        }
                    }),
                )
                .child(
                    Button::icon(
                        SharedString::from(format!("r2-del-{}", file.key)),
                        PhosphorIcon::Trash,
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .accessibility_label("Delete object")
                    .tooltip("Delete")
                    .on_click({
                        let key = file.key.clone();
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

pub(super) fn folder_card(folder: &R2FolderEntry, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let prefix = folder.prefix.clone();
    div()
        .id(SharedString::from(format!("r2-card-{}", folder.prefix)))
        .w(px(104.))
        .h(px(96.))
        .p_2()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted.opacity(0.5)))
        .on_click(move |_, _, cx| {
            let _ = Bridge::r2_navigate(&prefix, cx);
        })
        .child(
            div()
                .text_color(hsla(38. / 360., 0.9, 0.5, 1.))
                .child(PhosphorIcon::Folder),
        )
        .child(
            div()
                .max_w(px(88.))
                .truncate()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .child(folder.name.clone()),
        )
        .into_any_element()
}

pub(super) fn file_card(file: &R2FileEntry, selected: Option<&str>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(file.key.as_str());
    let key = file.key.clone();
    let key_for_open = file.key.clone();

    div()
        .id(SharedString::from(format!("r2-card-{}", file.key)))
        .w(px(104.))
        .h(px(96.))
        .p_2()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(if is_selected {
            theme.primary.opacity(0.4)
        } else {
            theme.border
        })
        .bg(if is_selected {
            theme.primary.opacity(0.1)
        } else {
            theme.background
        })
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted.opacity(0.5)))
        .on_click(move |_, _, cx| Bridge::r2_select(Some(key.clone()), cx))
        .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
            if event.click_count == 2 {
                let _ = Bridge::r2_download_and_open(&key_for_open, cx);
            }
        })
        .child(
            div()
                .text_color(theme.muted_foreground)
                .child(PhosphorIcon::FileText),
        )
        .child(
            div()
                .max_w(px(88.))
                .truncate()
                .text_xs()
                .child(file.name.clone()),
        )
        .into_any_element()
}

// ── details ────────────────────────────────────────────────────────────────

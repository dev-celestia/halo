//! File list row, section header, ignored toggle, and selection checkbox.

use std::path::Path;

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::ContextMenuExt as _;
use halo_bridge::{Bridge, GitFile};

use super::list::{GIT_FILE_ROW_H, GIT_SECTION_ROW_H, SectionKind};
use super::menus::changes_row_menu;
use crate::components::confirm_destructive;

pub(crate) fn section_header(
    title: &SharedString,
    kind: SectionKind,
    count: usize,
    cx: &App,
) -> impl IntoElement {
    let bulk = match kind {
        SectionKind::Staged => Some(("sc-unstage-all-hdr", "Unstage All", false)),
        SectionKind::Unstaged | SectionKind::Untracked => {
            Some(("sc-stage-all-hdr", "Stage All", true))
        }
        SectionKind::Ignored => None,
    };

    let mut header = div()
        .w_full()
        .h(GIT_SECTION_ROW_H)
        .px_3()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(cx.theme().muted_foreground)
                .child(format!("{title} ({count})")),
        );

    if let Some((id, label, stage)) = bulk {
        let btn_id = if kind == SectionKind::Untracked {
            "sc-stage-all-untracked"
        } else {
            id
        };
        header = header.child(
            Button::new(btn_id)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::XSmall)
                .label(label)
                .on_click(move |_, _, cx| {
                    if stage {
                        let _ = Bridge::git_stage_all(cx);
                    } else {
                        let _ = Bridge::git_unstage_all(cx);
                    }
                }),
        );
    }

    header
}

pub(crate) fn ignored_toggle(count: usize, open: bool, cx: &App) -> impl IntoElement {
    let chevron = if open {
        PhosphorIcon::CaretDown
    } else {
        PhosphorIcon::CaretRight
    };

    div()
        .id("sc-ignored-toggle")
        .w_full()
        .h(GIT_SECTION_ROW_H)
        .px_3()
        .flex()
        .items_center()
        .gap_1()
        .cursor_pointer()
        .hover(|s| s.bg(cx.theme().muted.opacity(0.35)))
        .on_click(|_, _, cx| {
            Bridge::git_toggle_ignored(cx);
        })
        .child(
            Phosphor::new(chevron)
                .size(px(12.))
                .color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(cx.theme().muted_foreground)
                .child(format!("Ignored ({count})")),
        )
}

pub(crate) fn changes_file_row(
    file: &GitFile,
    kind: SectionKind,
    repo_root: &str,
    editors: &[Entity<celestia_ui::components::CodeEditor>],
    selected: bool,
    viewing: bool,
    cx: &App,
) -> impl IntoElement {
    let path = file.path.clone();
    let abs_str = Path::new(repo_root).join(&path).display().to_string();
    let name = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let status = file.status.clone();
    let status_color = status_color(&status, cx);
    let ignored = kind == SectionKind::Ignored;
    let discardable = !ignored && kind != SectionKind::Staged;
    let editors = editors.to_vec();
    let hover_group = SharedString::from(format!("sc-row-{}-{path}", kind.key()));

    let mut row = div()
        .id(SharedString::from(format!("sc-{}-{}", kind.key(), path)))
        .group(hover_group.clone())
        .w_full()
        .h(GIT_FILE_ROW_H)
        .flex()
        .items_center()
        .gap_1p5()
        .px_3()
        .hover(|s| s.bg(cx.theme().muted.opacity(0.45)));

    if viewing {
        row = row
            .bg(cx.theme().primary.opacity(0.08))
            .border_l_2()
            .border_color(cx.theme().primary);
    } else if selected {
        row = row.bg(cx.theme().primary.opacity(0.1));
    }

    row = row
        .child(
            div()
                .w(px(18.))
                .flex_none()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(status_color)
                .child(status),
        )
        .child(
            div()
                .id(SharedString::from(format!("sc-open-{path}")))
                .flex()
                .items_center()
                .flex_1()
                .min_w_0()
                .cursor_pointer()
                .tooltip({
                    let path = path.clone();
                    move |window, cx| Tooltip::new(path.clone()).build(window, cx)
                })
                .on_click({
                    let path = path.clone();
                    let staged = kind.is_staged();
                    move |_, _, cx| {
                        if ignored {
                            return;
                        }
                        let _ = Bridge::git_diff(&path, staged, cx);
                    }
                })
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .text_color(if ignored {
                            cx.theme().muted_foreground
                        } else {
                            cx.theme().foreground
                        })
                        .child(name),
                ),
        );

    if !ignored && (file.additions > 0 || file.deletions > 0) {
        let mut counts = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .text_xs()
            .font_family(cx.theme().mono_font_family.clone());
        if file.additions > 0 {
            counts = counts.child(
                div()
                    .text_color(cx.theme().success)
                    .child(format!("+{}", file.additions)),
            );
        }
        if file.deletions > 0 {
            counts = counts.child(
                div()
                    .text_color(cx.theme().danger)
                    .child(format!("-{}", file.deletions)),
            );
        }
        row = row.child(counts);
    }

    if !ignored {
        if kind.is_staged() {
            row = row.child(
                Button::new(SharedString::from(format!("sc-unstage-{path}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .leading_icon(PhosphorIcon::Minus)
                    .tooltip("Unstage")
                    .on_click({
                        let path = path.clone();
                        move |_, _, cx| {
                            let _ = Bridge::git_unstage(&path, cx);
                        }
                    }),
            );
        } else {
            row = row.child(
                Button::new(SharedString::from(format!("sc-stage-{path}")))
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .leading_icon(PhosphorIcon::Plus)
                    .tooltip("Stage")
                    .on_click({
                        let path = path.clone();
                        move |_, _, cx| {
                            let _ = Bridge::git_stage(&path, cx);
                        }
                    }),
            );
        }
    }

    if discardable {
        let untracked = kind == SectionKind::Untracked;
        row = row.child(
            div()
                .flex_none()
                .w(px(0.))
                .h(px(16.))
                .overflow_hidden()
                .group_hover(hover_group, |style| style.w(px(16.)))
                .child(
                    Button::icon(
                        SharedString::from(format!("sc-discard-{path}")),
                        if untracked {
                            PhosphorIcon::Trash
                        } else {
                            PhosphorIcon::ArrowUUpLeft
                        },
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconXs)
                    .accessibility_label(if untracked {
                        "Delete file"
                    } else {
                        "Discard changes"
                    })
                    .tooltip(if untracked {
                        "Delete File"
                    } else {
                        "Discard Changes"
                    })
                    .on_click({
                        let path = path.clone();
                        move |_, window, cx| {
                            let path = path.clone();
                            let (title, body, action) = if untracked {
                                (
                                    "Delete file?",
                                    format!(
                                        "\"{path}\" will be deleted from disk. This cannot be undone."
                                    ),
                                    "Delete",
                                )
                            } else {
                                (
                                    "Discard changes?",
                                    format!(
                                        "Uncommitted changes to \"{path}\" will be permanently discarded. This cannot be undone."
                                    ),
                                    "Discard",
                                )
                            };
                            confirm_destructive(window, cx, title, body, action, move |_, cx| {
                                let _ = Bridge::git_discard(&path, cx);
                            });
                        }
                    }),
                ),
        );
    }

    if !ignored {
        row = row.child(select_toggle(
            format!("sc-sel-{}-{path}", kind.key()),
            selected,
            path.clone(),
            cx,
        ));
    }

    let menu_path = path.clone();
    let menu_abs = abs_str.clone();
    let menu_editors = editors.clone();
    row.context_menu(move |menu, _, _| {
        changes_row_menu(
            menu,
            &menu_path,
            &menu_abs,
            kind,
            discardable,
            &menu_editors,
        )
    })
}

pub(crate) fn select_toggle(
    id: String,
    selected: bool,
    path: String,
    cx: &App,
) -> impl IntoElement {
    let mut toggle = div()
        .id(SharedString::from(id))
        .flex_none()
        .w(px(16.))
        .h(px(16.))
        .rounded(px(3.))
        .border_1()
        .border_color(if selected {
            cx.theme().primary
        } else {
            cx.theme().border
        })
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .on_click(move |_, _, cx| {
            Bridge::git_toggle_select(&path, cx);
        });

    if selected {
        toggle = toggle.bg(cx.theme().primary).child(
            Phosphor::new(PhosphorIcon::Check)
                .size(px(11.))
                .color(cx.theme().primary_foreground),
        );
    }
    toggle
}

pub(crate) fn status_color(status: &str, cx: &App) -> Hsla {
    match status {
        "A" | "?" => cx.theme().success,
        "D" | "U" => cx.theme().danger,
        "M" | "R" | "C" | "T" => cx.theme().warning,
        _ => cx.theme().muted_foreground,
    }
}

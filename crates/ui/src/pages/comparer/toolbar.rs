//! Comparer toolbar — mode chips, Swap, Copy A/B/Diff, Clear All.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, DiffMode, RowKind};

use crate::pages::editors::ToolEditors;

pub(super) fn render_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let mode = state.comparer.diff_mode;
    let (removed, added) = change_counts(&state.comparer.rows);

    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.4))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Mode:"),
                )
                .children(DiffMode::ALL.iter().map(|m| {
                    let selected = *m == mode;
                    Button::new(SharedString::from(format!("cmp-mode-{}", m.label())))
                        .variant(if selected {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        })
                        .size(ButtonSize::Small)
                        .label(m.label())
                        .on_click(move |_, _, cx| Bridge::comparer_set_mode(*m, cx))
                        .into_any_element()
                }))
                .child(div().w(px(1.)).h(px(14.)).bg(theme.border))
                .child(
                    Button::new("cmp-swap")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::ArrowsLeftRight)
                        .label("Swap A/B")
                        .disabled(!state.comparer.has_content())
                        .on_click({
                            let a = editors.cmp_a.clone();
                            let b = editors.cmp_b.clone();
                            move |_, window, cx| {
                                let text_a = a.read(cx).text(cx);
                                let text_b = b.read(cx).text(cx);
                                a.update(cx, |e, cx| e.set_text(text_b, window, cx));
                                b.update(cx, |e, cx| e.set_text(text_a, window, cx));
                            }
                        }),
                )
                .when(removed + added > 0, |this| {
                    this.child(div().w(px(1.)).h(px(14.)).bg(theme.border))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .text_xs()
                                .child(
                                    div()
                                        .text_color(hsla(0., 0.7, 0.42, 0.95))
                                        .child(format!("−{removed}")),
                                )
                                .child(
                                    div()
                                        .text_color(hsla(145. / 360., 0.7, 0.35, 0.95))
                                        .child(format!("+{added}")),
                                ),
                        )
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    Button::new("cmp-copy-a")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Copy)
                        .label("Copy A")
                        .disabled(state.comparer.value_a.is_empty())
                        .on_click({
                            let text = state.comparer.value_a.clone();
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }
                        }),
                )
                .child(
                    Button::new("cmp-copy-b")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Copy)
                        .label("Copy B")
                        .disabled(state.comparer.value_b.is_empty())
                        .on_click({
                            let text = state.comparer.value_b.clone();
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }
                        }),
                )
                .child(
                    Button::new("cmp-copy-diff")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::GitDiff)
                        .label("Copy Diff")
                        .disabled(!state.comparer.has_diff())
                        .on_click({
                            let text = halo_bridge::comparer_unified_text(&state.comparer.rows);
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }
                        }),
                )
                .child(
                    Button::new("cmp-clear")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Trash)
                        .label("Clear All")
                        .disabled(!state.comparer.has_content())
                        .on_click({
                            let a = editors.cmp_a.clone();
                            let b = editors.cmp_b.clone();
                            move |_, window, cx| {
                                Bridge::comparer_clear(cx);
                                a.update(cx, |e, cx| e.set_text("", window, cx));
                                b.update(cx, |e, cx| e.set_text("", window, cx));
                            }
                        }),
                ),
        )
}

fn change_counts(rows: &[halo_bridge::DiffRow]) -> (usize, usize) {
    let mut removed = 0usize;
    let mut added = 0usize;
    for row in rows {
        if row.left_kind == RowKind::Removed {
            removed += 1;
        }
        if row.right_kind == RowKind::Added {
            added += 1;
        }
    }
    (removed, added)
}

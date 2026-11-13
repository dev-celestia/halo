//! Quick Open modal overlay (Go to File, ⌘P / Ctrl+P) in Coder.
//!
//! Provides a floating palette at the top center of Coder to search and jump to
//! any file in the workspace with fuzzy matching, keyboard navigation, and
//! file-type icons.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState};

use crate::explorer_icons;
use crate::pages::editors::ToolEditors;

pub(super) fn render_quick_open(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let matches = &state.coder.quick_open_matches;
    let selected_index = state.coder.quick_open_selected;
    let query = &state.coder.quick_open_query;

    div()
        .id("coder-quick-open-overlay")
        .key_context("CoderQuickOpen")
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .pt(px(48.))
        .bg(theme.background.opacity(0.45))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            Bridge::coder_quick_open_close(cx);
        })
        .child(
            div()
                .id("coder-quick-open-modal")
                .w(px(580.))
                .max_w(relative(0.92))
                .flex()
                .flex_col()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_lg()
                .overflow_hidden()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Search Input Header
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.15))
                        .child(
                            Phosphor::new(PhosphorIcon::MagnifyingGlass)
                                .size(px(16.))
                                .color(theme.muted_foreground),
                        )
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&editors.coder_quick_open_input)),
                        )
                        .child(
                            Button::new("coder-quick-open-close")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Icon)
                                .leading_icon(PhosphorIcon::X)
                                .tooltip("Close (Esc)")
                                .on_click(|_, _, cx| {
                                    Bridge::coder_quick_open_close(cx);
                                }),
                        ),
                )
                // Results List
                .child({
                    let mut list = div()
                        .id("coder-quick-open-results")
                        .w_full()
                        .max_h(px(320.))
                        .overflow_y_scrollbar()
                        .flex()
                        .flex_col()
                        .p(px(4.));

                    if matches.is_empty() {
                        list = list.child(
                            div()
                                .w_full()
                                .py_8()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap_1()
                                .child(
                                    Phosphor::new(PhosphorIcon::MagnifyingGlass)
                                        .size(px(28.))
                                        .color(theme.muted_foreground.opacity(0.5)),
                                )
                                .child(div().text_sm().text_color(theme.muted_foreground).child(
                                    if query.is_empty() {
                                        "No files found in workspace"
                                    } else {
                                        "No matching files found"
                                    },
                                )),
                        );
                    } else {
                        for (index, item) in matches.iter().enumerate() {
                            let is_selected = index == selected_index;
                            let file_name = item
                                .path
                                .rsplit('/')
                                .next()
                                .unwrap_or(&item.path)
                                .to_string();
                            let parent_dir = if let Some((dir, _)) = item.path.rsplit_once('/') {
                                dir.to_string()
                            } else {
                                String::new()
                            };

                            let editor_pool = editors.coder_editors.clone();
                            let selected_bg = theme.accent.opacity(0.35);
                            let hover_bg = if is_selected {
                                theme.accent.opacity(0.45)
                            } else {
                                theme.accent.opacity(0.18)
                            };
                            let mark = if is_selected {
                                theme.primary
                            } else {
                                theme.primary.opacity(0.)
                            };
                            list = list.child(
                                div()
                                    .id(SharedString::from(format!("coder-qo-item-{index}")))
                                    .w_full()
                                    .h(px(32.))
                                    .flex_none()
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .rounded(theme.radius_sm())
                                    .border_l_2()
                                    .border_color(mark)
                                    .cursor_pointer()
                                    .when(is_selected, move |el| el.bg(selected_bg))
                                    .hover(move |s| s.bg(hover_bg))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .min_w_0()
                                            .flex_1()
                                            .child(explorer_icons::file_icon(&file_name))
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_sm()
                                                    .font_weight(FontWeight::NORMAL)
                                                    .text_color(theme.foreground)
                                                    .child(file_name),
                                            ),
                                    )
                                    .when(!parent_dir.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .truncate()
                                                .max_w(px(240.))
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child(parent_dir),
                                        )
                                    })
                                    .on_click(move |_, window, cx| {
                                        let _ = Bridge::coder_quick_open_open_index(
                                            index,
                                            &editor_pool,
                                            window,
                                            cx,
                                        );
                                    }),
                            );
                        }
                    }

                    list
                })
                // Footer
                .child(
                    div()
                        .w_full()
                        .px_3()
                        .py_1p5()
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_t_1()
                        .border_color(theme.border)
                        .bg(theme.muted.opacity(0.1))
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child("↑↓ navigate")
                                .child("•")
                                .child("↵ open")
                                .child("•")
                                .child("esc close"),
                        )
                        .child(div().child(format!("{} files", matches.len()))),
                ),
        )
}

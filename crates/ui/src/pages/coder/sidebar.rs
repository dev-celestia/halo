//! Sidebar host — file explorer only.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{BridgeState, CoderSidebar};

use super::explorer::{explorer_create_parent, render_file_tree, start_new_file, start_new_folder};
use crate::pages::editors::ToolEditors;

pub(super) fn render_sidebar_panel(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    match state.coder.sidebar {
        CoderSidebar::Explorer => {
            let parent = explorer_create_parent(state);
            let inline_input = editors.coder_explorer_inline_name.clone();
            div()
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .w_full()
                        .flex_none()
                        .px_2()
                        .py_1()
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(cx.theme().muted_foreground)
                                .child("EXPLORER"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new("coder-explorer-search-files")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Icon)
                                        .leading_icon(PhosphorIcon::MagnifyingGlass)
                                        .tooltip("Go to File (⌘P)")
                                        .on_click(|_, window, cx| {
                                            window.dispatch_action(
                                                Box::new(crate::actions::ToggleCoderQuickOpen),
                                                cx,
                                            );
                                        }),
                                )
                                .child({
                                    let parent = parent.clone();
                                    let inline_input = inline_input.clone();
                                    Button::new("coder-explorer-new-file")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Icon)
                                        .leading_icon(PhosphorIcon::FilePlus)
                                        .tooltip("New File")
                                        .on_click(move |_, window, cx| {
                                            start_new_file(&parent, &inline_input, window, cx);
                                        })
                                })
                                .child({
                                    let parent = parent.clone();
                                    let inline_input = inline_input.clone();
                                    Button::new("coder-explorer-new-folder")
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Icon)
                                        .leading_icon(PhosphorIcon::FolderPlus)
                                        .tooltip("New Folder")
                                        .on_click(move |_, window, cx| {
                                            start_new_folder(&parent, &inline_input, window, cx);
                                        })
                                }),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(render_file_tree(state, editors, cx)),
                )
                .into_any_element()
        }
    }
}

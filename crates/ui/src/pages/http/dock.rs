//! Bottom dock for the HTTP page — the request detail under a section bar.
//!
//! The dock is sized by the page split in [`super::render_http`], not by a fixed
//! sheet height, so the traffic list gets the whole page back the moment the
//! detail is closed. The header carries the target request and the close
//! control; everything below it is the inspector.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use super::detail_view::{DetailEditors, render_detail_view};

pub fn render_http_dock(
    state: &BridgeState,
    editors: &DetailEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let target = state
        .selected_detail
        .as_ref()
        .map(|detail| format!("{} {}", detail.method, detail.url));

    let close = Button::icon("http-detail-close", PhosphorIcon::X)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconSm)
        .accessibility_label("Close detail")
        .tooltip("Close detail")
        .on_click(|_, _, cx| {
            let _ = Bridge::select_history_row(None, cx);
        });

    div()
        .size_full()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        // The table no longer draws a border of its own inside the page frame,
        // so the dock supplies the rule that separates it from the traffic list.
        .border_t_1()
        .border_color(theme.border)
        .child(section_bar("Detail", target, close, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(render_detail_view(state, editors, cx)),
        )
}

/// Header for the dock: label, optional meta, trailing actions.
fn section_bar(
    label: &'static str,
    meta: Option<String>,
    trailing: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    // The meta slot stretches when it exists; otherwise a spacer keeps the
    // trailing actions pinned to the right edge.
    let has_meta = meta.is_some();
    div()
        .w_full()
        .h(px(28.))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px_2()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.3))
        .child(
            div()
                .flex_none()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .when_some(meta, |this, meta| {
            this.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .font_family("monospace")
                    .text_color(theme.muted_foreground)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(meta),
            )
        })
        .when(!has_meta, |this| this.child(div().flex_1().min_w(px(4.))))
        .child(div().flex().flex_none().items_center().child(trailing))
}

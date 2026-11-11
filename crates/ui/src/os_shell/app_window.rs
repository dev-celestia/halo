//! In-app page chrome (non-desktop windows).

use celestia_ui::components::composite::window::WINDOW_RADIUS;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::BridgeState;

use crate::app::HaloApp;
use crate::nav::Page;
use crate::pages::{ToolEditors, render_page};

/// Tool page framed as a card: inset from the window, rounded border, optional
/// shadow. Every app uses this shell so the work area reads as one surface.
///
/// Pages that own their own pane splits skip the inner content pad so splits
/// sit flush against the card edge (Coder, Source Control, Agent, …).
pub(crate) fn render_app_window(
    page: Page,
    state: &BridgeState,
    editors: &ToolEditors,
    _weak: &WeakEntity<HaloApp>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    // Framed pages that own their own pane splits must not get the default
    // content inset — that pad reads as a gap beside the split.
    let flush_frame_content = matches!(
        page,
        Page::Agent | Page::Repeater | Page::Coder | Page::SourceControl | Page::HttpHistory
    );

    div()
        .id("app-window")
        .size_full()
        .p(px(6.))
        .bg(theme.background)
        .child(
            div()
                .id("app-window-frame")
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .overflow_hidden()
                .rounded(WINDOW_RADIUS)
                .border_1()
                .border_color(theme.window_border)
                .bg(theme.background)
                .when(theme.shadow, |this| {
                    this.shadow(vec![BoxShadow {
                        offset: point(px(0.), px(12.)),
                        blur_radius: px(32.),
                        spread_radius: px(0.),
                        inset: false,
                        color: rgba(0x00000022).into(),
                    }])
                })
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .when(!flush_frame_content, |this| {
                            // Tight in-card pad for floated tools without their
                            // own full-bleed splits (settings, scanners, …).
                            this.px(px(4.)).py(px(3.))
                        })
                        .child(render_page(page, state, editors, window, cx)),
                ),
        )
}

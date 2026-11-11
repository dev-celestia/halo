//! Intercept page — real-time pause, in-flight traffic inspection, wire modification, and managed browser integration.

mod paused_list;
mod paused_row;
mod request_panel;
mod toolbar;

pub use paused_list::render_paused_list;
pub use request_panel::render_request_panel;
pub use toolbar::render_intercept_toolbar;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

pub fn render_intercept(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .id("intercept-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .children(if !state.proxy.running {
            vec![
                div()
                    .w_full()
                    .p_3()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.warning)
                    .bg(theme.background)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(theme.warning)
                                    .child(PhosphorIcon::Warning),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(theme.foreground)
                                    .child("Proxy is stopped. The Intercept engine requires the proxy listener to capture and pause in-flight traffic."),
                            ),
                    )
                    .child(
                        Button::new("intercept-start-proxy-btn")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Small)
                            .label("Start Proxy")
                            .on_click({
                                let port = state.proxy.default_port;
                                move |_, _, cx| {
                                    let _ = Bridge::start_proxy(port, port + 1, cx);
                                }
                            }),
                    )
                    .into_any_element(),
            ]
        } else {
            vec![]
        })
        .child(render_intercept_toolbar(state, editors, cx))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(
                    ResizablePanelGroup::horizontal("intercept-layout")
                        .with_handle(true)
                        .child(
                            resizable_panel()
                                .size(px(380.))
                                .size_range(px(240.)..px(560.))
                                .child(
                                    div()
                                        .size_full()
                                        .overflow_hidden()
                                        .child(render_paused_list(state, editors, cx)),
                                ),
                        )
                        .child(
                            resizable_panel().child(
                                div()
                                    .size_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(render_request_panel(state, editors, cx)),
                            ),
                        ),
                ),
        )
}

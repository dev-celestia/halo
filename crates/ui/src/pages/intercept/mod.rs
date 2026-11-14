//! Intercept page — real-time pause, in-flight traffic inspection, wire modification, and managed browser integration.

mod paused_list;
mod paused_row;
mod request_panel;
mod toolbar;

pub use paused_list::render_paused_list;
pub use request_panel::render_request_panel;
pub use toolbar::render_intercept_toolbar;

use celestia_ui::components::composite::banner::{Banner, BannerTone};
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

pub fn render_intercept(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .id("intercept-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .when(!state.proxy.running, |el| {
            el.child(
                Banner::new("intercept-proxy-stopped", "Proxy is stopped")
                    .tone(BannerTone::Warning)
                    .body(
                        "The Intercept engine needs the proxy listener to capture and pause in-flight traffic.",
                    )
                    .action("Start Proxy", |_, _, cx| {
                        let http = Bridge::state(cx).read(cx).preferred_http_port;
                        let tls = Bridge::state(cx).read(cx).preferred_tls_port;
                        let _ = Bridge::start_proxy(http, tls, cx);
                    }),
            )
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
                                .child(render_paused_list(state, editors, cx)),
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

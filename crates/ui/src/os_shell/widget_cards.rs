//! Individual desktop widget card bodies.

use celestia_ui::components::{StatCard, StatCardSize, StatDeltaTone, Tooltip};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::BridgeState;

use crate::app::HaloApp;
use crate::components::{ProxyActionState, ProxyButtonCopy, proxy_toggle_button};
use crate::nav::Page;

use super::helpers::open_app;

pub(super) fn render_proxy_widget(
    state: &BridgeState,
    value: String,
    delta: String,
    tone: StatDeltaTone,
    weak: &WeakEntity<HaloApp>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let open_settings = open_app(weak, Page::Settings);
    let connections = format!("{} connections", state.proxy.connections);
    let action = proxy_toggle_button(
        "widget-proxy",
        ProxyActionState::from_bridge(state),
        ProxyButtonCopy::Compact,
    );

    StatCard::new("widget-proxy", "Proxy", value)
        .size(StatCardSize::Small)
        .delta(delta, tone)
        .w_full()
        .flex_none()
        .footer(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .id("widget-proxy-settings")
                        .role(Role::Button)
                        .aria_label("Open proxy settings")
                        .tab_index(0)
                        .flex_1()
                        .min_w_0()
                        .cursor_pointer()
                        .tooltip(move |window, cx| {
                            Tooltip::new("Open proxy settings").build(window, cx)
                        })
                        .on_click(move |_, _, cx| open_settings(cx))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(connections),
                        ),
                )
                .child(action),
        )
}

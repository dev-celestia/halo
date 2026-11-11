use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::{StatusTone, status_pill};
use crate::pages::editors::ToolEditors;

pub fn render_override_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let host_input = editors.override_host.clone();
    let rule_count = state.mock.override_routes().len();

    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(status_pill(
            format!("{rule_count} rules"),
            if rule_count > 0 {
                StatusTone::Active
            } else {
                StatusTone::Idle
            },
            cx,
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Target host:"),
                )
                .child(div().w(px(220.)).child(host_input.clone()))
                .child(
                    Button::new("override-add-host-btn")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Plus)
                        .label("Add Host")
                        .on_click({
                            let host_input = host_input.clone();
                            move |_, _, cx| {
                                let host = host_input.read(cx).value().to_string();
                                if !host.trim().is_empty() {
                                    let _ = Bridge::mock_add_domain(&host, cx);
                                }
                            }
                        }),
                ),
        )
        .child(
            Button::new("override-new-btn")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Plus)
                .label("New Override")
                .disabled(state.mock.override_domains().is_empty())
                .on_click(|_, _, cx| {
                    Bridge::mock_select_route(None, cx);
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Add hosts from HTTP history traffic, then define override rules."),
        )
}

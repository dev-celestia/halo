use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::{StatusTone, status_pill};
use crate::pages::editors::ToolEditors;

pub fn render_server_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let running = state.mock.server.running;
    let port_input = editors.mock_port.clone();
    let port_value = editors.mock_port.read(cx).value().to_string();
    let port = port_value.parse::<u16>().unwrap_or(4000);

    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(if running {
            status_pill(
                format!(
                    "RUNNING · {}",
                    state.mock.server.url.clone().unwrap_or_default()
                ),
                StatusTone::Active,
                cx,
            )
            .into_any_element()
        } else {
            status_pill("STOPPED", StatusTone::Idle, cx).into_any_element()
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Port:"),
                )
                .child(div().w(px(72.)).child(port_input.clone())),
        )
        .child(if running {
            Button::new("mock-stop-btn")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Stop)
                .label("Stop")
                .on_click(|_, _, cx| {
                    let _ = Bridge::mock_stop_server(cx);
                })
                .into_any_element()
        } else {
            Button::new("mock-start-btn")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Play)
                .label("Start")
                .on_click(move |_, _, cx| {
                    let cors = halo_bridge::Bridge::state(cx).read(cx).mock.cors;
                    let _ = Bridge::mock_start_server(port, cors, cx);
                })
                .into_any_element()
        })
        .child(
            Button::new("mock-refresh-btn")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .label("Refresh")
                .on_click(|_, _, cx| {
                    let _ = Bridge::load_mock_forge(cx);
                }),
        )
        .child(
            Button::new("mock-cors-toggle")
                .variant(if state.mock.cors {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .size(ButtonSize::Small)
                .label(if state.mock.cors {
                    "CORS: On"
                } else {
                    "CORS: Off"
                })
                .on_click(|_, _, cx| {
                    let next = !Bridge::state(cx).read(cx).mock.cors;
                    Bridge::mock_set_cors(next, cx);
                }),
        )
        .child(
            Button::new("mock-new-btn")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Plus)
                .label("New Endpoint")
                .on_click(|_, _, cx| {
                    Bridge::mock_select_route(None, cx);
                }),
        )
}

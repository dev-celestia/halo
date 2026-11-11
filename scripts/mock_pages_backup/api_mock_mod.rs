//! API Mock page — local mock HTTP server (127.0.0.1) with editable endpoints.
//!
//! Mirrors `references/hexbuffer/src/pages/api-mock`: server toolbar (status,
//! port, start/stop), endpoint list on the left, a method/path/status/body
//! editor on the right, and a served-request log strip.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::{PAGE_GAP, StatusTone, empty_state, panel, status_pill};
use crate::pages::colors::{method_color, status_color_u16};
use crate::pages::editors::ToolEditors;

pub fn render_api_mock(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let _ = cx.theme();

    div()
        .id("api-mock-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_server_toolbar(state, editors, cx))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(
                    ResizablePanelGroup::horizontal("api-mock-layout")
                        .with_handle(true)
                        .child(
                            resizable_panel()
                                .size(px(300.))
                                .size_range(px(200.)..px(480.))
                                .child(render_endpoint_list(state, editors, cx)),
                        )
                        .child(
                            resizable_panel().child(
                                div()
                                    .size_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(render_route_editor(state, editors, cx)),
                            ),
                        ),
                ),
        )
        .child(render_logs_strip(state, cx))
}

fn render_server_toolbar(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
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
                    let _ = Bridge::mock_start_server(port, true, cx);
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

fn render_endpoint_list(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let filter = editors.mock_filter.read(cx).value().to_lowercase();
    let selected_id = state.mock.selected_route_id.clone();

    let routes: Vec<_> = state
        .mock
        .local_routes()
        .into_iter()
        .filter(|r| {
            filter.is_empty()
                || r.path.to_lowercase().contains(&filter)
                || r.method.to_lowercase().contains(&filter)
        })
        .collect();

    panel(
        format!("Endpoints ({})", routes.len()),
        None,
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().w_full().child(editors.mock_filter.clone()))
            .child(
                div()
                    .id("mock-endpoint-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .children(if routes.is_empty() {
                        vec![empty_state(
                            "No endpoints yet",
                            "Create one to start serving canned responses.",
                            cx,
                        )
                        .into_any_element()]
                    } else {
                        routes
                            .iter()
                            .map(|route| {
                                let is_selected =
                                    selected_id.as_deref() == Some(route.id.as_str());
                                let id = route.id.clone();
                                let path = route.path.clone();
                                div()
                                    .id(SharedString::from(format!("mock-row-{}", route.id)))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .py_1p5()
                                    .rounded(theme.radius_sm())
                                    .border_1()
                                    .border_color(if is_selected {
                                        theme.primary
                                    } else {
                                        theme.border
                                    })
                                    .bg(if is_selected {
                                        theme.accent
                                    } else {
                                        theme.background
                                    })
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme.accent))
                                    .on_click(move |_, _, cx| {
                                        Bridge::mock_select_route(Some(id.clone()), cx);
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(method_color(&route.method))
                                            .child(route.method.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .truncate()
                                            .child(path.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(status_color_u16(route.status_code))
                                            .child(route.status_code.to_string()),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "mock-del-{}",
                                            route.id
                                        )))
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .leading_icon(PhosphorIcon::Trash)
                                        .on_click({
                                            let id = route.id.clone();
                                            move |_, _, cx| {
                                                let _ = Bridge::mock_delete_route(&id, cx);
                                            }
                                        }),
                                    )
                                    .into_any_element()
                            })
                            .collect()
                    }),
            ),
        cx,
    )
}

fn render_route_editor(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let selected = state.mock.selected_route();

    let (title, is_new) = match selected {
        Some(route) => (
            format!("Edit Endpoint — {} {}", route.method, route.path),
            false,
        ),
        None => ("New Endpoint".to_string(), true),
    };

    panel(
        title,
        Some(
            "Responses support {{param}} template tags for dynamic path segments.".into(),
        ),
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(88.)).child(editors.mock_method.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .child(editors.mock_path.clone()),
                    )
                    .child(div().w(px(72.)).child(editors.mock_status.clone())),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(editors.mock_body.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .children(if is_new {
                        vec![
                            Button::new("mock-create-btn")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .label("Create Endpoint")
                                .on_click({
                                    let editors_method = editors.mock_method.clone();
                                    let editors_path = editors.mock_path.clone();
                                    let editors_status = editors.mock_status.clone();
                                    let editors_body = editors.mock_body.clone();
                                    move |_, _, cx| {
                                        let method =
                                            editors_method.read(cx).value().to_string();
                                        let path = editors_path.read(cx).value().to_string();
                                        let status = editors_status
                                            .read(cx)
                                            .value()
                                            .parse::<u16>()
                                            .unwrap_or(200);
                                        let body = editors_body.read(cx).text(cx);
                                        let _ = Bridge::mock_add_local_route(
                                            &method, &path, status, &body, cx,
                                        );
                                    }
                                })
                                .into_any_element(),
                        ]
                    } else {
                        vec![
                            Button::new("mock-save-btn")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::FloppyDisk)
                                .label("Save")
                                .on_click({
                                    let editors_method = editors.mock_method.clone();
                                    let editors_path = editors.mock_path.clone();
                                    let editors_status = editors.mock_status.clone();
                                    let editors_body = editors.mock_body.clone();
                                    move |_, _, cx| {
                                        let method =
                                            editors_method.read(cx).value().to_string();
                                        let path = editors_path.read(cx).value().to_string();
                                        let status = editors_status
                                            .read(cx)
                                            .value()
                                            .parse::<u16>()
                                            .unwrap_or(200);
                                        let body = editors_body.read(cx).text(cx);
                                        let _ = Bridge::mock_update_selected_route(
                                            &method, &path, status, &body, cx,
                                        );
                                    }
                                })
                                .into_any_element(),
                            Button::new("mock-delete-btn")
                                .variant(ButtonVariant::Destructive)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Trash)
                                .label("Delete")
                                .on_click(|_, _, cx| {
                                    let id = Bridge::state(cx)
                                        .read(cx)
                                        .mock
                                        .selected_route_id
                                        .clone();
                                    if let Some(id) = id {
                                        let _ = Bridge::mock_delete_route(&id, cx);
                                    }
                                })
                                .into_any_element(),
                        ]
                    }),
            ),
        cx,
    )
}

fn render_logs_strip(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let logs: Vec<_> = state.mock.logs.iter().take(8).cloned().collect();

    panel(
        "Recent Requests",
        None,
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children(if logs.is_empty() {
                vec![div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("No requests logged yet. Start the server and hit an endpoint.")
                    .into_any_element()]
            } else {
                logs.iter()
                    .map(|log| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(theme.radius_sm())
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(method_color(&log.method))
                                    .child(log.method.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .font_family("Menlo")
                                    .truncate()
                                    .child(log.path.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(status_color_u16(log.status_code))
                                    .child(log.status_code.to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{}ms", log.latency_ms)),
                            )
                            .into_any_element()
                    })
                    .collect()
            })
            .child(
                div().flex().justify_end().child(
                    Button::new("mock-clear-logs-btn")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Clear logs")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::mock_clear_logs(cx);
                        }),
                ),
            ),
        cx,
    )
}

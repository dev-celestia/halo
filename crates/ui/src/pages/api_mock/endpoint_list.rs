use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_delete;
use crate::pages::chrome::{empty_state, panel};
use crate::pages::colors::{method_color, status_color_u16};
use crate::pages::editors::ToolEditors;

pub fn render_endpoint_list(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
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
                        vec![
                            empty_state(
                                "No endpoints yet",
                                "Create one to start serving canned responses.",
                                cx,
                            )
                            .into_any_element(),
                        ]
                    } else {
                        routes
                            .iter()
                            .map(|route| {
                                let is_selected = selected_id.as_deref() == Some(route.id.as_str());
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
                                            .font_weight(FontWeight::NORMAL)
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
                                        Button::icon(
                                            SharedString::from(format!("mock-del-{}", route.id)),
                                            PhosphorIcon::Trash,
                                        )
                                        .variant(ButtonVariant::Ghost)
                                        .size(ButtonSize::Small)
                                        .accessibility_label("Delete endpoint")
                                        .tooltip("Delete endpoint")
                                        .on_click({
                                            let id = route.id.clone();
                                            let method = route.method.clone();
                                            let path = route.path.clone();
                                            move |_, window, cx| {
                                                let id = id.clone();
                                                confirm_delete(
                                                    window,
                                                    cx,
                                                    "Delete endpoint?",
                                                    format!(
                                                        "{method} {path} will be removed from this mock server. This cannot be undone."
                                                    ),
                                                    move |_, cx| {
                                                        let _ = Bridge::mock_delete_route(&id, cx);
                                                    },
                                                );
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

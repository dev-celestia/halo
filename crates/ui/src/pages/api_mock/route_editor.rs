use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use super::chaos::{render_chaos_section, render_simulate_bar};
use crate::components::confirm_delete;
use crate::pages::chrome::panel;
use crate::pages::editors::ToolEditors;

pub fn render_route_editor(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
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
        Some("Responses support {{param}} template tags for dynamic path segments.".into()),
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
            .child(render_simulate_bar(state, editors, cx))
            .child(render_chaos_section(state, editors, cx))
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
                                        let method = editors_method.read(cx).value().to_string();
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
                                        let method = editors_method.read(cx).value().to_string();
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
                            Button::icon("mock-delete-btn", PhosphorIcon::Trash)
                                .variant(ButtonVariant::Destructive)
                                .size(ButtonSize::Small)
                                .accessibility_label("Delete endpoint")
                                .tooltip("Delete endpoint")
                                .on_click(|_, window, cx| {
                                    let target = {
                                        let state = Bridge::state(cx);
                                        let state = state.read(cx);
                                        state.mock.selected_route().map(|route| {
                                            (
                                                route.id.clone(),
                                                route.method.clone(),
                                                route.path.clone(),
                                            )
                                        })
                                    };
                                    let Some((id, method, path)) = target else {
                                        return;
                                    };
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
                                })
                                .into_any_element(),
                        ]
                    }),
            ),
        cx,
    )
}

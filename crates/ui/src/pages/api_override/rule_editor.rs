use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_delete;
use crate::pages::chrome::panel;
use crate::pages::editors::ToolEditors;

pub fn render_rule_editor(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let selected = state.mock.selected_route();

    let (title, is_new) = match selected {
        Some(rule) => (format!("Edit Rule — {} {}", rule.method, rule.path), false),
        None => ("New Override Rule".to_string(), true),
    };

    panel(
        title,
        Some(
            "Paths may be relative (/v1/charges/:id) or absolute URLs. {{param}} tags are supported."
                .into(),
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
                    .child(div().w(px(88.)).child(editors.override_method.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .child(editors.override_path.clone()),
                    )
                    .child(div().w(px(72.)).child(editors.override_status.clone())),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(editors.override_body.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .children(if is_new {
                        vec![
                            Button::new("override-create-btn")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .label("Create Override Rule")
                                .disabled(state.mock.override_domains().is_empty())
                                .on_click({
                                    let method = editors.override_method.clone();
                                    let path = editors.override_path.clone();
                                    let status = editors.override_status.clone();
                                    let body = editors.override_body.clone();
                                    move |_, _, cx| {
                                        let domain_id = Bridge::state(cx)
                                            .read(cx)
                                            .mock
                                            .override_domains()
                                            .first()
                                            .map(|d| d.id.clone())
                                            .unwrap_or_default();
                                        let method = method.read(cx).value().to_string();
                                        let path = path.read(cx).value().to_string();
                                        let status = status
                                            .read(cx)
                                            .value()
                                            .parse::<u16>()
                                            .unwrap_or(200);
                                        let body = body.read(cx).text(cx);
                                        let _ = Bridge::mock_add_override_route(
                                            &domain_id, &method, &path, status, &body, cx,
                                        );
                                    }
                                })
                                .into_any_element(),
                        ]
                    } else {
                        vec![
                            Button::new("override-save-btn")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::FloppyDisk)
                                .label("Save")
                                .on_click({
                                    let method = editors.override_method.clone();
                                    let path = editors.override_path.clone();
                                    let status = editors.override_status.clone();
                                    let body = editors.override_body.clone();
                                    move |_, _, cx| {
                                        let method = method.read(cx).value().to_string();
                                        let path = path.read(cx).value().to_string();
                                        let status = status
                                            .read(cx)
                                            .value()
                                            .parse::<u16>()
                                            .unwrap_or(200);
                                        let body = body.read(cx).text(cx);
                                        let _ = Bridge::mock_update_selected_route(
                                            &method, &path, status, &body, cx,
                                        );
                                    }
                                })
                                .into_any_element(),
                            Button::icon("override-delete-btn", PhosphorIcon::Trash)
                                .variant(ButtonVariant::Destructive)
                                .size(ButtonSize::Small)
                                .accessibility_label("Delete override")
                                .tooltip("Delete override")
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
                                        "Delete override?",
                                        format!(
                                            "{method} {path} will no longer be overridden. This cannot be undone."
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

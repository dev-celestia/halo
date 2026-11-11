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

pub fn render_rules_tree(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let filter = editors.override_filter.read(cx).value().to_lowercase();
    let selected_id = state.mock.selected_route_id.clone();

    let domains: Vec<_> = state
        .mock
        .override_domains()
        .into_iter()
        .filter(|d| {
            filter.is_empty()
                || d.hostname.to_lowercase().contains(&filter)
                || state
                    .mock
                    .override_routes()
                    .iter()
                    .any(|r| r.domain_id == d.id && r.path.to_lowercase().contains(&filter))
        })
        .collect();

    let mut sections: Vec<AnyElement> = Vec::new();

    for domain in &domains {
        let rules: Vec<_> = state
            .mock
            .override_routes()
            .into_iter()
            .filter(|r| r.domain_id == domain.id)
            .collect();

        sections.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .rounded(theme.radius_sm())
                .bg(theme.muted)
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child(domain.hostname.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("{}/{}", rules.len(), domain.status)),
                        )
                        .child(
                            Button::icon(
                                SharedString::from(format!(
                                    "override-del-host-{}",
                                    domain.id
                                )),
                                PhosphorIcon::Trash,
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .accessibility_label("Delete host")
                            .tooltip("Delete host and its override rules")
                            .on_click({
                                let id = domain.id.clone();
                                let hostname = domain.hostname.clone();
                                move |_, window, cx| {
                                    let id = id.clone();
                                    confirm_delete(
                                        window,
                                        cx,
                                        "Delete host?",
                                        format!(
                                            "{hostname} and all of its override rules will be removed. This cannot be undone."
                                        ),
                                        move |_, cx| {
                                            let _ = Bridge::mock_delete_domain(&id, cx);
                                        },
                                    );
                                }
                            }),
                        ),
                )
                .into_any_element(),
        );

        if rules.is_empty() {
            sections.push(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("No override rules yet for this host")
                    .into_any_element(),
            );
        }

        for rule in rules {
            let is_selected = selected_id.as_deref() == Some(rule.id.as_str());
            let rule_id_toggle = rule.id.clone();
            sections.push(
                div()
                    .id(SharedString::from(format!("override-row-{}", rule.id)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl_6()
                    .pr_2()
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
                    .opacity(if rule.enabled { 1.0 } else { 0.45 })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.accent))
                    .on_click({
                        let id = rule.id.clone();
                        move |_, _, cx| {
                            Bridge::mock_select_route(Some(id.clone()), cx);
                        }
                    })
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(method_color(&rule.method))
                            .child(rule.method.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .child(rule.path.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(status_color_u16(rule.status_code))
                            .child(rule.status_code.to_string()),
                    )
                    .child(if rule.enabled {
                        Button::new(SharedString::from(format!(
                            "override-off-{}",
                            rule_id_toggle
                        )))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Disable")
                        .on_click({
                            let id = rule_id_toggle.clone();
                            move |_, _, cx| {
                                let _ = Bridge::mock_set_route_enabled(&id, false, cx);
                            }
                        })
                        .into_any_element()
                    } else {
                        Button::new(SharedString::from(format!(
                            "override-on-{}",
                            rule_id_toggle
                        )))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Enable")
                        .on_click({
                            let id = rule_id_toggle.clone();
                            move |_, _, cx| {
                                let _ = Bridge::mock_set_route_enabled(&id, true, cx);
                            }
                        })
                        .into_any_element()
                    })
                    .into_any_element(),
            );
        }
    }

    if domains.is_empty() {
        sections.push(
            empty_state(
                "No target hosts yet",
                "Add a host above, or capture one from HTTP history traffic.",
                cx,
            )
            .into_any_element(),
        );
    }

    panel(
        "Target Hosts & Rules",
        None,
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().w_full().child(editors.override_filter.clone()))
            .child(
                div()
                    .id("override-rules-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(sections),
            ),
        cx,
    )
}

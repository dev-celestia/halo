//! API Override page — replace live proxied responses per captured host.
//!
//! Mirrors `references/hexbuffer/src/pages/api-override`: hosts are captured
//! from HTTP history ("Send to API Override" flows land here through the
//! backend's shared MockForge route table), rules group under each host, and
//! the right pane edits the override response.

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

pub fn render_api_override(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    div()
        .id("api-override-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_override_toolbar(state, editors, cx))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(
                    ResizablePanelGroup::horizontal("api-override-layout")
                        .with_handle(true)
                        .child(
                            resizable_panel()
                                .size(px(320.))
                                .size_range(px(220.)..px(520.))
                                .child(render_rules_tree(state, editors, cx)),
                        )
                        .child(
                            resizable_panel().child(
                                div()
                                    .size_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(render_rule_editor(state, editors, cx)),
                            ),
                        ),
                ),
        )
}

fn render_override_toolbar(
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
                            let input = host_input.clone();
                            move |_, window, cx| {
                                let host = input.read(cx).value().trim().to_string();
                                if !host.is_empty() {
                                    let _ = Bridge::mock_add_domain(&host, cx);
                                    input.update(cx, |s, cx| {
                                        s.set_value("", window, cx);
                                    });
                                }
                            }
                        }),
                ),
        )
        .child(
            Button::new("override-refresh-btn")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .label("Refresh")
                .on_click(|_, _, cx| {
                    let _ = Bridge::load_mock_forge(cx);
                }),
        )
        .child(
            Button::new("override-new-btn")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Plus)
                .label("New Rule")
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

fn render_rules_tree(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
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
                        .font_weight(FontWeight::SEMIBOLD)
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
                            Button::new(SharedString::from(format!(
                                "override-del-host-{}",
                                domain.id
                            )))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::Trash)
                            .on_click({
                                let id = domain.id.clone();
                                move |_, _, cx| {
                                    let _ = Bridge::mock_delete_domain(&id, cx);
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
                            .font_weight(FontWeight::SEMIBOLD)
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

fn render_rule_editor(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
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
                            Button::new("override-delete-btn")
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

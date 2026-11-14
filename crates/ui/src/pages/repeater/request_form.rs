//! Forge workspace bar, request bar, and request canvas (headers/body/scripts).

use std::collections::HashMap;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::select::Select;
use celestia_ui::components::primitive::tabs::{Tabs, TabsList, TabsTrigger};
use celestia_ui::components::{VariableInput, VariableOption};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{Sizable as _, Size};
use halo_bridge::{
    Bridge, BridgeState, ForgeRequestTab, RepeaterRequestDto, parse_context_variables,
};

use crate::pages::chrome::{code_surface, flush_panel};
use crate::pages::editors::ToolEditors;
use crate::pages::repeater::EnvVarsForm;
use super::env_modal::open_rename_context_dialog;
use super::folder_tree::{open_new_collection_dialog, open_postman_import};

/// Slim workbench chrome: collection controls (leading), active request title, env (trailing).
pub fn render_workspace_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let request_title = active_request_title(state);

    div()
        .id("repeater-workspace-bar")
        .w_full()
        .flex_none()
        .px_2()
        .py_1()
        .flex()
        .items_center()
        .gap_2()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .flex_none()
                .child(render_collection_picker(editors))
                .child(render_collection_add())
                .child(render_collection_import()),
        )
        .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.foreground)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(request_title),
        )
        .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .flex_none()
                .child(render_env_picker(editors))
                .child(render_env_rename(state))
                .child(render_env_settings(editors)),
        )
}

/// Request transport bar spanning Forge + Response panes.
///
/// Single row: method + URL + Save/Send. Matches sidebar header padding
/// (`px_2` / `py_1p5`) so the split chrome shares one horizontal edge.
pub fn render_request_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    _cx: &App,
) -> impl IntoElement {
    let method = editors.repeater_method.clone();
    let url = editors.repeater_url.clone();
    let headers = editors.repeater_headers.clone();
    let body = editors.repeater_request.clone();
    let scripts = editors.repeater_scripts.clone();
    let loading = state.active_request.is_loading;

    div()
        .w_full()
        .flex_none()
        .px_2()
        .py_1p5()
        .flex()
        .items_center()
        .gap_2()
        .child(render_method_picker(editors))
        .child(
            div().flex_1().min_w_0().child(
                VariableInput::new(&editors.repeater_url)
                    .variables(active_variable_options(state))
                    .on_manage({
                        let env_vars = editors.repeater_env_vars.clone();
                        move |window, cx| open_environment_modal(&env_vars, window, cx)
                    }),
            ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .flex_none()
                .child(
                    Button::new("forge-save")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::FloppyDisk)
                        .label("Save")
                        .on_click({
                            let method = method.clone();
                            let url = url.clone();
                            let headers = headers.clone();
                            let body = body.clone();
                            let scripts = scripts.clone();
                            move |_, _, cx| {
                                let method = method
                                    .read(cx)
                                    .selected_value()
                                    .cloned()
                                    .unwrap_or_else(|| "GET".to_string());
                                let url = url.read(cx).value().to_string();
                                let headers_map = headers.read(cx).to_map(cx);
                                let headers_json = serde_json::to_string(&headers_map)
                                    .unwrap_or_else(|_| "{}".into());
                                let body = body.read(cx).text(cx);
                                let script_text = scripts.read(cx).text(cx);
                                let (pre, test) = split_scripts(&script_text);
                                let _ = Bridge::save_active_endpoint(
                                    &method,
                                    &url,
                                    &headers_json,
                                    &body,
                                    "raw",
                                    &pre,
                                    &test,
                                    cx,
                                );
                            }
                        }),
                )
                .child(
                    Button::new("repeater-send")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::PaperPlaneTilt)
                        .label(if loading { "Sending…" } else { "Send" })
                        .on_click({
                            let method = method.clone();
                            let url = url.clone();
                            let headers = headers.clone();
                            let body = body.clone();
                            move |_, _, cx| {
                                let request = RepeaterRequestDto {
                                    method: method
                                        .read(cx)
                                        .selected_value()
                                        .cloned()
                                        .unwrap_or_else(|| "GET".to_string()),
                                    url: url.read(cx).value().to_string(),
                                    headers: headers.read(cx).to_map(cx),
                                    body: body.read(cx).text(cx),
                                };
                                let _ = Bridge::send_craft_request(request, cx);
                            }
                        }),
                ),
        )
}

/// Forge pane: request tabs + request canvas.
pub fn render_request_pane(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    // Flush to the pane edge so the editor border fills the split — no inset gap.
    flush_panel(
        "repeater-request",
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .flex_none()
                    .px_1()
                    .pt_1()
                    .pb_0p5()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_none().child(render_request_tabs(state)))
                    .child(div().flex_1())
                    .when(
                        matches!(state.forge_request_tab, ForgeRequestTab::Body),
                        |this| this.child(render_request_insert(state, editors)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .px_1()
                    .pb_1()
                    .child(render_request_canvas(state, editors, cx)),
            ),
        cx,
    )
}

fn active_request_title(state: &BridgeState) -> SharedString {
    let name = state.active_request.name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("Untitled") {
        if state.active_request.endpoint_id.is_some() {
            "Untitled request".into()
        } else {
            "No request selected".into()
        }
    } else {
        name.to_string().into()
    }
}

fn render_collection_picker(editors: &ToolEditors) -> impl IntoElement {
    div().w(px(180.)).flex_none().min_w_0().child(
        Select::new(&editors.repeater_collection)
            .id("repeater-collection-select")
            .with_size(Size::Small)
            .placeholder("Select collection")
            .accessibility_label("Active collection")
            .text_xs()
            .font_weight(FontWeight::NORMAL),
    )
}

fn render_collection_add() -> impl IntoElement {
    Button::icon(
        "collection-add",
        Phosphor::new(PhosphorIcon::Plus).size(px(14.)),
    )
    .variant(ButtonVariant::Ghost)
    .size(ButtonSize::IconSm)
    .accessibility_label("New collection")
    .tooltip("New collection")
    .on_click(|_, window, cx| {
        open_new_collection_dialog(window, cx);
    })
}

fn render_collection_import() -> impl IntoElement {
    Button::icon(
        "collection-import",
        Phosphor::new(PhosphorIcon::UploadSimple).size(px(14.)),
    )
    .variant(ButtonVariant::Ghost)
    .size(ButtonSize::IconSm)
    .accessibility_label("Import Postman")
    .tooltip("Import Postman")
    .on_click(|_, window, cx| {
        open_postman_import(window, cx);
    })
}

fn render_env_picker(editors: &ToolEditors) -> impl IntoElement {
    div().w(px(148.)).flex_none().min_w_0().child(
        Select::new(&editors.repeater_env)
            .id("forge-env-select")
            .with_size(Size::Small)
            .accessibility_label("Active environment")
            .text_xs()
            .font_weight(FontWeight::NORMAL),
    )
}

fn render_env_rename(state: &BridgeState) -> impl IntoElement {
    let active_id = state.active_context_id.clone();
    let active_name = state
        .contexts
        .iter()
        .find(|c| Some(&c.id) == active_id.as_ref())
        .map(|c| c.name.clone());

    div().when_some(active_id.zip(active_name), |this, (id, name)| {
        this.child(
            Button::icon(
                "ctx-rename-env",
                Phosphor::new(PhosphorIcon::PencilSimple).size(px(14.)),
            )
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .accessibility_label("Rename environment")
            .tooltip("Rename environment")
            .on_click(move |_, window, cx| {
                open_rename_context_dialog(&id, &name, window, cx);
            }),
        )
    })
}

fn render_env_settings(editors: &ToolEditors) -> impl IntoElement {
    Button::icon(
        "ctx-setup-env",
        Phosphor::new(PhosphorIcon::Gear).size(px(14.)),
    )
    .variant(ButtonVariant::Ghost)
    .size(ButtonSize::IconSm)
    .accessibility_label("Manage environments")
    .tooltip("Manage environments")
    .on_click({
        let env_vars = editors.repeater_env_vars.clone();
        move |_, window, cx| open_environment_modal(&env_vars, window, cx)
    })
}

fn open_environment_modal(env_vars: &Entity<EnvVarsForm>, window: &mut Window, cx: &mut App) {
    let state = Bridge::state(cx).read(cx);
    if let Some(active_id) = &state.active_context_id {
        if let Some(context) = state
            .contexts
            .iter()
            .find(|context| &context.id == active_id)
        {
            let variables = context.variables.clone();
            env_vars.update(cx, |form, cx| {
                form.set_from_json(&variables, window, cx);
            });
        }
    }
    Bridge::set_env_modal_open(true, cx);
}

fn active_variable_options(state: &BridgeState) -> Vec<VariableOption> {
    let Some(active_id) = state.active_context_id.as_ref() else {
        return Vec::new();
    };
    let Some(context) = state
        .contexts
        .iter()
        .find(|context| &context.id == active_id)
    else {
        return Vec::new();
    };
    let mut options: Vec<_> = parse_context_variables(&context.variables)
        .into_iter()
        .map(|(name, value)| VariableOption::new(name).value(value))
        .collect();
    options.sort_by(|left, right| left.name.cmp(&right.name));
    options
}

fn render_request_insert(state: &BridgeState, editors: &ToolEditors) -> impl IntoElement {
    let request = editors.repeater_request.clone();
    let env_vars = editors.repeater_env_vars.clone();
    VariableInput::insert_button(
        "forge-request-insert-var",
        active_variable_options(state),
        move |name, window, cx| {
            let placeholder = VariableInput::placeholder(name.as_ref());
            request.update(cx, |editor, cx| {
                editor.editor_state().update(cx, |state, cx| {
                    state.insert(placeholder, window, cx);
                });
            });
        },
    )
    .on_manage(move |window, cx| open_environment_modal(&env_vars, window, cx))
}

fn render_method_picker(editors: &ToolEditors) -> impl IntoElement {
    div().w(px(92.)).flex_none().child(
        Select::new(&editors.repeater_method)
            .id("forge-method-select")
            .with_size(Size::Small)
            .accessibility_label("HTTP method")
            .text_xs()
            .font_weight(FontWeight::NORMAL),
    )
}

fn render_request_tabs(state: &BridgeState) -> impl IntoElement {
    let tab = state.forge_request_tab;
    Tabs::new("forge-request-tabs").child(
        TabsList::new("forge-request-tab-list")
            .child(
                TabsTrigger::new("forge-request-headers")
                    .label("Headers")
                    .selected(matches!(tab, ForgeRequestTab::Headers))
                    .on_click(|_, _, cx| {
                        Bridge::set_forge_request_tab(ForgeRequestTab::Headers, cx);
                    }),
            )
            .child(
                TabsTrigger::new("forge-request-body")
                    .label("Body")
                    .selected(matches!(tab, ForgeRequestTab::Body))
                    .on_click(|_, _, cx| {
                        Bridge::set_forge_request_tab(ForgeRequestTab::Body, cx);
                    }),
            )
            .child(
                TabsTrigger::new("forge-request-scripts")
                    .label("Scripts")
                    .selected(matches!(tab, ForgeRequestTab::Scripts))
                    .on_click(|_, _, cx| {
                        Bridge::set_forge_request_tab(ForgeRequestTab::Scripts, cx);
                    }),
            ),
    )
}

fn render_request_canvas(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    match state.forge_request_tab {
        ForgeRequestTab::Headers => {
            let theme = cx.theme();
            div()
                .size_full()
                .min_h_0()
                .min_w_0()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .overflow_hidden()
                .bg(theme.background)
                .child(editors.repeater_headers.clone())
                .into_any_element()
        }
        ForgeRequestTab::Body => {
            code_surface(editors.repeater_request.clone(), cx).into_any_element()
        }
        ForgeRequestTab::Scripts => {
            code_surface(editors.repeater_scripts.clone(), cx).into_any_element()
        }
    }
}

/// Split `Header: value` lines and an optional body separated by a blank line.
pub fn split_raw_http(text: &str) -> (HashMap<String, String>, String) {
    let normalized = text.replace("\r\n", "\n");
    let (header_block, body) = match normalized.split_once("\n\n") {
        Some((headers, body)) => (headers, body.to_string()),
        None => (normalized.as_str(), String::new()),
    };

    let mut headers = HashMap::new();
    for line in header_block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_string(), value.trim().to_string());
        }
    }
    (headers, body)
}

pub fn split_scripts(text: &str) -> (String, String) {
    if let Some((pre, test)) = text.split_once("---tests---") {
        (pre.trim().to_string(), test.trim().to_string())
    } else {
        (text.to_string(), String::new())
    }
}

pub fn join_scripts(pre: &str, test: &str) -> String {
    format!("{}\n\n---tests---\n\n{}", pre.trim(), test.trim())
}

pub fn headers_map_to_raw(headers: &HashMap<String, String>, body: &str) -> String {
    let mut lines: Vec<String> = headers.iter().map(|(k, v)| format!("{k}: {v}")).collect();
    lines.sort();
    let mut out = format!("{}\n\n{}", lines.join("\n"), body);
    // End on its own line so the `http` highlight grammar keeps the final
    // body line inside the body node (see `http_format::finish`).
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

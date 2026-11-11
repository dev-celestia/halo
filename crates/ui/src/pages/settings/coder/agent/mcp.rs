//! MCP servers editor and parsing helpers.

use super::*;

pub(super) fn mcp_servers_setup(
    settings: &CoderSettings,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let border = theme.border;
    let surface = theme.popover;
    let radius = theme.radius;
    let servers = settings.mcp_servers.clone();
    let transport = editors.mcp_transport.clone();
    let selected_transport = transport.get();

    let mut list = div().w_full().flex().flex_col().gap_2();
    if servers.is_empty() {
        list = list.child(
            div()
                .text_xs()
                .text_color(muted)
                .child("No MCP servers configured yet."),
        );
    }
    for server in &servers {
        let name = server.name.clone();
        let enabled = server.enabled;
        let configured = server.is_configured();
        let summary = server.summary();
        let transport_label = server.transport.label();
        let transport_icon = match server.transport {
            McpTransport::Stdio => PhosphorIcon::TerminalWindow,
            McpTransport::Remote => PhosphorIcon::Globe,
        };
        let summary_text = if configured {
            summary
        } else {
            format!("{summary} — incomplete")
        };
        let summary_color = if configured { muted } else { theme.warning };

        let toggle_all = servers.clone();
        let toggle_name = name.clone();
        let remove_all = servers.clone();
        let remove_name = name.clone();

        list = list.child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_2p5()
                .rounded(radius)
                .border_1()
                .border_color(border)
                .bg(surface)
                .child(
                    Phosphor::new(transport_icon)
                        .size(px(14.))
                        .color(if configured {
                            foreground
                        } else {
                            summary_color
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(foreground)
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded(theme.radius_sm())
                                        .bg(theme.muted)
                                        .text_color(muted)
                                        .child(transport_label),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(summary_color)
                                .child(summary_text),
                        ),
                )
                .child(
                    Switch::new(SharedString::from(format!(
                        "coder-mcp-enabled-{toggle_name}"
                    )))
                    .checked(enabled)
                    .on_change(move |next, _, cx| {
                        let mut rows = toggle_all.clone();
                        if let Some(row) = rows.iter_mut().find(|row| row.name == toggle_name) {
                            row.enabled = *next;
                        }
                        Bridge::set_mcp_servers(rows, cx);
                    }),
                )
                .child(
                    Button::icon(
                        SharedString::from(format!("coder-mcp-remove-{remove_name}")),
                        PhosphorIcon::Trash,
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .accessibility_label("Remove MCP server")
                    .on_click(move |_, _, cx| {
                        let rows = remove_all
                            .iter()
                            .filter(|row| row.name != remove_name)
                            .cloned()
                            .collect();
                        Bridge::set_mcp_servers(rows, cx);
                    }),
                ),
        );
    }

    let mut transport_chips = div().flex().gap_2();
    for transport_kind in [McpTransport::Stdio, McpTransport::Remote] {
        let transport = transport.clone();
        transport_chips = transport_chips.child(
            Button::new(SharedString::from(format!(
                "coder-mcp-transport-{}",
                transport_kind.id()
            )))
            .variant(if transport_kind == selected_transport {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Outline
            })
            .size(ButtonSize::Small)
            .label(transport_kind.label())
            .on_click(move |_, window, cx| {
                transport.set(transport_kind);
                cx.notify(window.current_view());
            }),
        );
    }

    let name_input = editors.coder_settings_mcp_name.clone();
    let address_input = editors.coder_settings_mcp_command.clone();
    let extras_input = editors.coder_settings_mcp_args.clone();
    let add_transport = transport.clone();
    let add = {
        let name_input = name_input.clone();
        let address_input = address_input.clone();
        let extras_input = extras_input.clone();
        Button::new("coder-mcp-add")
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Small)
            .leading_icon(PhosphorIcon::Plus)
            .label("Add server")
            .on_click(move |_, window, cx| {
                let name = name_input.read(cx).value().trim().to_string();
                let address = address_input.read(cx).value().trim().to_string();
                let extras = extras_input.read(cx).value().to_string();
                let transport = add_transport.get();
                let mut row = McpServerConfig {
                    name,
                    transport,
                    enabled: true,
                    ..McpServerConfig::default()
                };
                match transport {
                    McpTransport::Stdio => {
                        row.command = address;
                        row.args = parse_mcp_args(&extras);
                    }
                    McpTransport::Remote => {
                        row.url = address;
                        row.headers = parse_mcp_headers(&extras);
                    }
                }
                if !row.is_configured() {
                    return;
                }
                let mut rows = Bridge::state(cx)
                    .read(cx)
                    .coder_settings
                    .mcp_servers
                    .clone();
                rows.retain(|existing| existing.name != row.name);
                rows.push(row);
                Bridge::set_mcp_servers(rows, cx);
                for input in [&name_input, &address_input, &extras_input] {
                    input.update(cx, |state, cx| state.set_value("", window, cx));
                }
            })
    };

    div().w_full().flex().flex_col().gap_3().child(list).child(
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .pt_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(muted)
                    .child("Add server"),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(200.))
                            .flex_none()
                            .child(Input::new(&editors.coder_settings_mcp_name)),
                    )
                    .child(transport_chips),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&editors.coder_settings_mcp_command)),
                    )
                    .child(add),
            )
            .child(Input::new(&editors.coder_settings_mcp_args).cleanable(true)),
    )
}

pub(super) fn parse_mcp_args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(ToString::to_string).collect()
}

pub(super) fn parse_mcp_headers(raw: &str) -> Vec<McpKeyValue> {
    raw.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some(McpKeyValue {
                key: key.to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect()
}

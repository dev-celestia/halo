//! OpenCode harness setup panel.

use super::*;

pub(super) fn opencode_setup(
    settings: &CoderSettings,
    editors: &ToolEditors,
    installing: Option<&str>,
    needs_upgrade: bool,
    cx: &App,
) -> impl IntoElement {
    let installing_opencode = installing == Some("opencode");
    let install_busy = installing.is_some();
    let path_summary = settings
        .opencode_executable_path
        .as_deref()
        .unwrap_or("Default ($PATH / system binary)");

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section(
            "OpenCode Executable Path",
            "Custom path to the OpenCode binary if not on PATH.",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Active: {path_summary}")),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(Input::new(&editors.coder_settings_opencode_path)),
                        )
                        .child(action("coder-opencode-path-apply", "Apply", {
                            let input = editors.coder_settings_opencode_path.clone();
                            move |cx| {
                                let val = input.read(cx).value().trim().to_string();
                                let path = if val.is_empty() { None } else { Some(val) };
                                Bridge::update_coder_settings(cx, move |s| {
                                    s.opencode_executable_path = path.clone();
                                });
                                let _ = Bridge::halo_agent_refresh_catalogs(cx);
                            }
                        }))
                        .child(
                            Button::new("coder-opencode-install")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .label(if installing_opencode {
                                    if needs_upgrade {
                                        "Upgrading…"
                                    } else {
                                        "Installing…"
                                    }
                                } else if needs_upgrade {
                                    "Upgrade"
                                } else {
                                    "Install"
                                })
                                // `disabled` cancels loading styling — only grey
                                // out when another harness install is in flight.
                                .disabled(install_busy && !installing_opencode)
                                .loading(installing_opencode)
                                .loading_style(ButtonLoadingStyle::GridShift)
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::halo_agent_install_opencode(cx);
                                }),
                        )
                        .when(settings.opencode_executable_path.is_some(), |el| {
                            el.child(action("coder-opencode-path-clear", "Reset", |cx| {
                                Bridge::update_coder_settings(cx, |s| {
                                    s.opencode_executable_path = None;
                                });
                                let _ = Bridge::halo_agent_refresh_catalogs(cx);
                            }))
                        }),
                ),
            cx,
        ))
        .child(section(
            "MCP servers",
            "Local servers run commands on your machine; remote servers receive the headers you configure. Only enable servers you trust.",
            mcp_servers_setup(settings, editors, cx),
            cx,
        ))
}

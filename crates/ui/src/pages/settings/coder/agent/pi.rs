//! Pi harness setup panel.

use super::*;

pub(super) fn pi_setup(
    settings: &CoderSettings,
    editors: &ToolEditors,
    current_model: Option<&str>,
    installing: Option<&str>,
    needs_upgrade: bool,
    cx: &App,
) -> impl IntoElement {
    let installing_pi = installing == Some("pi");
    let install_busy = installing.is_some();
    let path_summary = settings
        .pi_executable_path
        .as_deref()
        .unwrap_or("Default ($PATH / system binary)");

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section(
            "Pi Executable Path",
            "Custom path to the Pi binary if not in your default system PATH.",
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
                                .child(Input::new(&editors.coder_settings_pi_path)),
                        )
                        .child(action("coder-pi-path-apply", "Apply", {
                            let input = editors.coder_settings_pi_path.clone();
                            move |cx| {
                                let val = input.read(cx).value().trim().to_string();
                                let path = if val.is_empty() { None } else { Some(val) };
                                Bridge::update_coder_settings(cx, move |s| {
                                    s.pi_executable_path = path.clone();
                                });
                            }
                        }))
                        .child(
                            Button::new("coder-pi-install")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .label(if installing_pi {
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
                                .disabled(install_busy && !installing_pi)
                                .loading(installing_pi)
                                .loading_style(ButtonLoadingStyle::GridShift)
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::halo_agent_install_pi(cx);
                                }),
                        )
                        .when(settings.pi_executable_path.is_some(), |el| {
                            el.child(action("coder-pi-path-clear", "Reset", |cx| {
                                Bridge::update_coder_settings(cx, |s| {
                                    s.pi_executable_path = None;
                                });
                            }))
                        }),
                ),
            cx,
        ))
        .child(deepseek_setup(settings, editors, current_model, cx))
}

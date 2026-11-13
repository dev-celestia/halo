//! Unified, data-driven first-run setup and readiness card for Pi, OpenCode,
//! Antigravity, and Cursor harnesses.

use celestia_ui::components::primitive::button::ButtonLoadingStyle;
use celestia_ui::components::primitive::input::Input;
use halo_bridge::{HarnessReadiness, SettingsTab};

use super::model_picker::{harness_mark, render_mark};
use super::*;

struct HarnessSetupSpec {
    id: &'static str,
    badge: &'static str,
    title: &'static str,
    setup_hint: String,
    architecture_note: &'static str,
    primary_label: &'static str,
    working: bool,
    install_busy: bool,
    state_blocked: bool,
    needs_cursor_key: bool,
    show_custom_path: bool,
    install_error: Option<String>,
    install_stage: Option<String>,
}

fn build_spec(state: &BridgeState) -> HarnessSetupSpec {
    let harness = state.coder.agent_harness.as_str();
    let is_installing_this = state.coder.agent_installing.as_deref() == Some(harness);
    let install_stage = if is_installing_this {
        state.coder.agent_install_stage.clone()
    } else {
        None
    };

    match harness {
        "opencode" => {
            let repairing = state.doctor.is_installing("system:agent-state");
            let working = is_installing_this || repairing;
            let install_busy = state.coder.agent_installing.is_some() || repairing;
            let needs_upgrade = state.coder.agent_pi_needs_upgrade;
            let state_blocked = state.coder.agent_state_blocked;
            let setup_hint = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
                "OpenCode is not installed. Click Install below, or set a custom binary path in Settings."
                    .into()
            });
            let install_error = state
                .coder
                .agent_install_error
                .clone()
                .filter(|_| state.coder.agent_install_error_harness.as_deref() == Some("opencode"))
                .filter(|err| !err.trim().is_empty() && !setup_hint.contains(err.trim()))
                .filter(|_| !state_blocked)
                .or_else(|| {
                    if !state_blocked {
                        return None;
                    }
                    state
                        .doctor
                        .last_log_for("system:agent-state")
                        .filter(|entry| entry.is_error)
                        .map(|entry| entry.message.clone())
                });
            let badge = if working {
                "WORKING"
            } else if state_blocked {
                "BLOCKED"
            } else if needs_upgrade {
                "UPGRADE"
            } else {
                "SETUP"
            };
            let title = if state_blocked {
                "OpenCode can't start"
            } else if needs_upgrade {
                "Upgrade OpenCode"
            } else {
                "Set up OpenCode"
            };
            let primary_label = if repairing {
                "Fixing ownership…"
            } else if is_installing_this {
                if needs_upgrade {
                    "Upgrading OpenCode…"
                } else {
                    "Installing OpenCode…"
                }
            } else if state_blocked {
                "Fix ownership"
            } else if needs_upgrade {
                "Upgrade OpenCode"
            } else {
                "Install OpenCode"
            };
            HarnessSetupSpec {
                id: "opencode",
                badge,
                title,
                setup_hint,
                architecture_note: "Runs a local OpenCode server with MCP support. Sign in to model providers with `opencode auth login`.",
                primary_label,
                working,
                install_busy,
                state_blocked,
                needs_cursor_key: false,
                show_custom_path: true,
                install_error,
                install_stage,
            }
        }
        "cursor" => {
            let doctor_installing = state.doctor.is_installing("harness:cursor");
            let working = is_installing_this || doctor_installing;
            let install_busy = state.coder.agent_installing.is_some() || state.doctor.busy();
            let needs_cursor_key = state
                .coder
                .agent_pi_setup_hint
                .as_deref()
                .is_some_and(|hint| hint.contains("API key") || hint.contains("CURSOR_API_KEY"));
            let setup_hint = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
                "Install the Cursor SDK runtime and connect your Cursor API key below.".into()
            });
            let install_error = state
                .coder
                .agent_install_error
                .clone()
                .filter(|_| state.coder.agent_install_error_harness.as_deref() == Some("cursor"))
                .or_else(|| {
                    state
                        .doctor
                        .last_log_for("harness:cursor")
                        .filter(|entry| entry.is_error)
                        .map(|entry| entry.message.clone())
                });
            let badge = if working {
                "WORKING"
            } else if needs_cursor_key {
                "AUTH REQUIRED"
            } else {
                "SETUP"
            };
            let title = if needs_cursor_key {
                "Connect Cursor API key"
            } else {
                "Set up Cursor SDK"
            };
            let primary_label = if working {
                "Installing Cursor SDK…"
            } else {
                "Install Cursor SDK"
            };
            HarnessSetupSpec {
                id: "cursor",
                badge,
                title,
                setup_hint,
                architecture_note: "Runs the local Cursor SDK runtime using your Cursor account API key.",
                primary_label,
                working,
                install_busy,
                state_blocked: false,
                needs_cursor_key,
                show_custom_path: false,
                install_error,
                install_stage,
            }
        }
        "antigravity" => {
            let working = is_installing_this;
            let install_busy = state.coder.agent_installing.is_some();
            let setup_hint = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
                "Install the Antigravity ACP server to use Gemini and Antigravity models.".into()
            });
            let install_error = state.coder.agent_install_error.clone().filter(|_| {
                state.coder.agent_install_error_harness.as_deref() == Some("antigravity")
            });
            let primary_label = if working {
                "Installing ACP server…"
            } else {
                "Install ACP server"
            };
            HarnessSetupSpec {
                id: "antigravity",
                badge: if working { "WORKING" } else { "SETUP" },
                title: "Set up Antigravity",
                setup_hint,
                architecture_note: "Connects to Google Antigravity over the Agent Client Protocol (ACP).",
                primary_label,
                working,
                install_busy,
                state_blocked: false,
                needs_cursor_key: false,
                show_custom_path: true,
                install_error,
                install_stage,
            }
        }
        _ => {
            let working = is_installing_this;
            let install_busy = state.coder.agent_installing.is_some();
            let needs_upgrade = state.coder.agent_pi_needs_upgrade;
            let setup_hint = state
                .coder
                .agent_pi_setup_hint
                .clone()
                .unwrap_or_else(|| "Install Pi 1.4.0 to run coding sessions in Halo.".into());
            let install_error = state
                .coder
                .agent_install_error
                .clone()
                .filter(|_| state.coder.agent_install_error_harness.as_deref() == Some("pi"));
            let badge = if working {
                "WORKING"
            } else if needs_upgrade {
                "UPGRADE"
            } else {
                "SETUP"
            };
            let title = if needs_upgrade {
                "Upgrade Pi"
            } else {
                "Set up Pi"
            };
            let primary_label = if working {
                if needs_upgrade {
                    "Upgrading to Pi 1.4.0…"
                } else {
                    "Installing Pi 1.4.0…"
                }
            } else if needs_upgrade {
                "Upgrade to Pi 1.4.0"
            } else {
                "Install Pi 1.4.0"
            };
            HarnessSetupSpec {
                id: "pi",
                badge,
                title,
                setup_hint,
                architecture_note: "Runs the Pi coding agent with your Anthropic, OpenAI, Gemini, DeepSeek, or Ollama keys. Requires Node.js 22.19+.",
                primary_label,
                working,
                install_busy,
                state_blocked: false,
                needs_cursor_key: false,
                show_custom_path: true,
                install_error,
                install_stage,
            }
        }
    }
}

pub(super) fn render_harness_setup_card(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let spec = build_spec(state);
    let theme = cx.theme();
    let warning = theme.warning;
    let muted = theme.muted_foreground;
    let border = theme.border;
    let surface = theme.popover;
    let chip_bg = warning.opacity(0.14);
    let chip_radius = px(6.);
    let (mark, tint) = harness_mark(spec.id);

    // Other harnesses that are already installed and ready to use right now.
    let ready_alternatives: Vec<(String, String)> = state
        .coder
        .agent_harness_readiness
        .iter()
        .filter(|row| row.id != spec.id && row.is_usable())
        .map(|row| {
            (
                row.id.clone(),
                HarnessReadiness::display_name(&row.id).to_string(),
            )
        })
        .collect();

    let harness_id = spec.id;
    let state_blocked = spec.state_blocked;

    div()
        .id(SharedString::from(format!("{}-setup-card", spec.id)))
        .w_full()
        .p(px(16.))
        .bg(surface)
        .border_1()
        .border_color(border)
        .rounded(theme.radius_lg)
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex_none()
                                .size(px(30.))
                                .rounded(chip_radius)
                                .bg(chip_bg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(render_mark(
                                    mark,
                                    tint,
                                    px(16.),
                                    PhosphorIcon::Cpu,
                                    warning,
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(3.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.))
                                        .child(
                                            div()
                                                .px(px(7.))
                                                .py(px(2.))
                                                .rounded(chip_radius)
                                                .bg(chip_bg)
                                                .text_xs()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(warning)
                                                .child(spec.badge),
                                        )
                                        .when_some(spec.install_stage.clone(), |el, stage| {
                                            el.child(div().text_xs().text_color(muted).child(stage))
                                        }),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .line_height(CARD_TITLE_LINE)
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(spec.title),
                                ),
                        ),
                )
                .child(
                    Button::new("setup-card-switch-harness")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::ArrowsLeftRight)
                        .label("Switch harness")
                        .disabled(spec.install_busy)
                        .on_click(|_, _, cx| {
                            Bridge::halo_agent_open_provider_sheet(cx);
                        }),
                ),
        )
        .child(
            div()
                .text_sm()
                .line_height(CARD_TITLE_LINE)
                .text_color(theme.foreground)
                .child(spec.setup_hint),
        )
        .child(
            div()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted)
                .child(spec.architecture_note),
        )
        .when(spec.needs_cursor_key, |el| {
            let input_entity = editors.agent_cursor_api_key.clone();
            el.child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .pt(px(2.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(220.))
                            .child(Input::new(&editors.agent_cursor_api_key)),
                    )
                    .child(
                        Button::new("cursor-inline-save-key")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::Key)
                            .label("Save API Key")
                            .on_click(move |_, window, cx| {
                                let val = input_entity.read(cx).value().trim().to_string();
                                if !val.is_empty() {
                                    let _ =
                                        Bridge::halo_agent_save_inline_api_key("cursor", &val, cx);
                                    input_entity.update(cx, |state, cx| {
                                        state.set_value("", window, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("cursor-inline-open-keys")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Small)
                            .label("Get Key ↗")
                            .on_click(|_, _, cx| {
                                cx.open_url("https://www.cursor.com/settings");
                            }),
                    ),
            )
        })
        .when_some(spec.install_error, |el, err| {
            let err_for_copy = err.clone();
            el.child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(8.))
                    .p(px(10.))
                    .rounded(chip_radius)
                    .bg(theme.danger.opacity(0.08))
                    .border_1()
                    .border_color(theme.danger.opacity(0.25))
                    .child(
                        div()
                            .flex_none()
                            .px(px(6.))
                            .py(px(2.))
                            .rounded(px(4.))
                            .bg(theme.danger.opacity(0.16))
                            .text_xs()
                            .text_color(theme.danger)
                            .child("FAILED"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .text_color(theme.foreground)
                            .child(err),
                    )
                    .child(
                        Button::new("setup-card-copy-error")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::Copy)
                            .label("Copy")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    err_for_copy.clone(),
                                ));
                            }),
                    ),
            )
        })
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .pt_1()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .when(!spec.needs_cursor_key, |el| {
                            el.child(
                                Button::new(SharedString::from(format!(
                                    "{}-setup-install",
                                    spec.id
                                )))
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .label(spec.primary_label)
                                .disabled(spec.install_busy && !spec.working)
                                .loading(spec.working)
                                .loading_style(ButtonLoadingStyle::GridShift)
                                .on_click(
                                    move |_, window, cx| {
                                        if state_blocked {
                                            crate::pages::doctor::request_fix(
                                                window,
                                                cx,
                                                "system:agent-state",
                                            );
                                        } else {
                                            let _ =
                                                Bridge::halo_agent_install_harness(harness_id, cx);
                                        }
                                    },
                                ),
                            )
                        })
                        .child(
                            Button::new(SharedString::from(format!("{}-setup-refresh", spec.id)))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::ArrowsClockwise)
                                .label("Check again")
                                .disabled(spec.install_busy)
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::halo_agent_refresh_catalogs(cx);
                                }),
                        )
                        .when(spec.show_custom_path, |el| {
                            el.child(
                                Button::new(SharedString::from(format!(
                                    "{}-setup-settings",
                                    spec.id
                                )))
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Gear)
                                .label("Custom path…")
                                .disabled(spec.install_busy)
                                .on_click(|_, _, cx| {
                                    crate::windows::open_settings_window(SettingsTab::Agent, cx);
                                }),
                            )
                        }),
                )
                .when(!ready_alternatives.is_empty(), |el| {
                    let mut row = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .child(div().text_xs().text_color(muted).child("Ready now:"));
                    for (alt_id, alt_name) in ready_alternatives {
                        let switch_id = alt_id.clone();
                        row = row.child(
                            Button::new(SharedString::from(format!("setup-quick-switch-{alt_id}")))
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .label(format!("Use {alt_name}"))
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::halo_agent_set_harness(&switch_id, cx);
                                }),
                        );
                    }
                    el.child(row)
                }),
        )
}

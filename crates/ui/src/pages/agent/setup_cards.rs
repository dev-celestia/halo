//! First-run setup cards for the OpenCode, Cursor, Antigravity, and Pi harnesses.

use super::*;

pub(super) fn opencode_setup_card(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let warning = theme.warning;
    let muted = theme.muted_foreground;
    let border = theme.border;
    let surface = theme.popover;
    let chip_bg = warning.opacity(0.14);
    let chip_radius = px(6.);
    let is_installing = state.coder.agent_installing.as_deref() == Some("opencode");
    // The ownership repair runs through Doctor (`system:agent-state`), so the
    // button has to watch Doctor's in-flight list, not the harness installer.
    let repairing = state.doctor.is_installing("system:agent-state");
    let working = is_installing || repairing;
    let install_busy = state.coder.agent_installing.is_some() || repairing;
    let needs_upgrade = state.coder.agent_pi_needs_upgrade;
    // The binary works but cannot write its own state directory: reinstalling
    // is a no-op, the ownership repair is the fix.
    let state_blocked = state.coder.agent_state_blocked;
    let setup_hint = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
        "Install OpenCode ≥ 2.2.0 (https://opencode.ai) or set a custom binary path in Coder Settings."
            .into()
    });
    // A failed run usually repeats the setup hint verbatim; showing it twice
    // just doubles the wall of text. When the state directory is blocked the
    // hint *is* the diagnosis, so the raw probe line is never worth a second row.
    let install_error = state
        .coder
        .agent_install_error
        .clone()
        .filter(|err| !err.trim().is_empty() && !setup_hint.contains(err.trim()))
        .filter(|_| !state_blocked)
        .or_else(|| {
            // A repair that failed (usually: needs sudo) has to be visible right
            // here — its log line carries the hand-off command.
            if !state_blocked {
                return None;
            }
            state
                .doctor
                .last_log_for("system:agent-state")
                .filter(|entry| entry.is_error)
                .map(|entry| entry.message.clone())
        });
    let title = if state_blocked {
        "OpenCode can't start"
    } else if needs_upgrade {
        "Upgrade OpenCode"
    } else {
        "Set up OpenCode"
    };
    let primary_label = if repairing {
        "Fixing ownership…"
    } else if is_installing {
        if needs_upgrade {
            "Upgrading OpenCode…"
        } else {
            "Installing OpenCode…"
        }
    } else if state_blocked {
        "Fix Ownership"
    } else if needs_upgrade {
        "Upgrade OpenCode"
    } else {
        "Install OpenCode"
    };

    div()
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
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(28.))
                        .rounded(chip_radius)
                        .bg(chip_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::Cpu)
                                .size(px(14.))
                                .color(warning),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .px(px(7.))
                                .py(px(2.))
                                .rounded(chip_radius)
                                .bg(chip_bg)
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(warning)
                                .child(if is_installing {
                                    "WORKING"
                                } else if state_blocked {
                                    "BLOCKED"
                                } else if needs_upgrade {
                                    "UPGRADE"
                                } else {
                                    "SETUP"
                                }),
                        )
                        .child(
                            div()
                                .text_sm()
                                .line_height(CARD_TITLE_LINE)
                                .font_weight(FontWeight::NORMAL)
                                .child(title),
                        ),
                ),
        )
        .child(
            div()
                .text_sm()
                .line_height(CARD_TITLE_LINE)
                .text_color(theme.foreground)
                .child(setup_hint),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(
                    "Halo drives OpenCode over its native HTTP server (`opencode serve`), not ACP. Authenticate providers with `opencode auth login` in a terminal.",
                ),
        )
        .when_some(install_error, |el, err| {
            el.child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    // An error is multi-line; centre-aligning it against the chip
                    // floats the badge into the middle of the paragraph.
                    .items_start()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_none()
                            .px(px(7.))
                            .py(px(2.))
                            .rounded(chip_radius)
                            .bg(theme.danger.opacity(0.14))
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.danger)
                            .child("FAILED"),
                    )
                    .child(
                        // `min_w_0` is what lets a long probe message wrap instead
                        // of running past the card's right edge.
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(err),
                    ),
            )
        })
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .pt_1()
                .child(
                    Button::new("opencode-setup-install")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .label(primary_label)
                        // `disabled` cancels loading styling — only grey out when
                        // another harness install is in flight.
                        .disabled(install_busy && !working)
                        .loading(working)
                        .loading_style(ButtonLoadingStyle::GridShift)
                        .on_click(move |_, window, cx| {
                            if state_blocked {
                                // Doctor owns the ownership repair, its log, its
                                // verification pass — and the confirmation gate.
                                crate::pages::doctor::request_fix(
                                    window,
                                    cx,
                                    "system:agent-state",
                                );
                            } else {
                                let _ = Bridge::halo_agent_install_opencode(cx);
                            }
                        }),
                )
                .child(
                    Button::new("opencode-setup-refresh")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .label("Check again")
                        .disabled(install_busy)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_refresh_catalogs(cx);
                        }),
                )
                .child(
                    Button::new("opencode-setup-settings")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Gear)
                        .label("Custom path…")
                        .disabled(install_busy)
                        .on_click(|_, _, cx| {
                            crate::windows::open_settings_window(SettingsTab::Agent, cx);
                        }),
                ),
        )
}

pub(super) fn cursor_setup_card(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let warning = theme.warning;
    let border = theme.border;
    let surface = theme.popover;
    let chip_bg = warning.opacity(0.14);
    let chip_radius = px(6.);
    // Doctor owns Node + pinned SDK install for `harness:cursor`.
    let installing = state.doctor.is_installing("harness:cursor");
    let doctor_busy = state.doctor.busy();
    let install_error = state
        .doctor
        .last_log_for("harness:cursor")
        .filter(|entry| entry.is_error)
        .map(|entry| entry.message.clone());
    let needs_key = state
        .coder
        .agent_pi_setup_hint
        .as_deref()
        .is_some_and(|hint| hint.contains("API key"));
    let primary_label = if installing {
        "Installing Cursor SDK…"
    } else {
        "Install Node + SDK"
    };
    let body = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
        "Install Node.js (if needed) and the pinned @cursor/sdk shim. Then set CURSOR_API_KEY or paste a user key into Halo cursor-state/auth.json.".into()
    });

    div()
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
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(28.))
                        .rounded(chip_radius)
                        .bg(chip_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::Cpu)
                                .size(px(14.))
                                .color(warning),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .px(px(7.))
                                .py(px(2.))
                                .rounded(chip_radius)
                                .bg(chip_bg)
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(warning)
                                .child(if installing { "WORKING" } else { "SETUP" }),
                        )
                        .child(
                            div()
                                .text_sm()
                                .line_height(CARD_TITLE_LINE)
                                .font_weight(FontWeight::NORMAL)
                                .child("Set up Cursor"),
                        ),
                ),
        )
        .child(
            div()
                .text_sm()
                .line_height(CARD_TITLE_LINE)
                .text_color(theme.foreground)
                .child(body),
        )
        .when_some(install_error, |el, err| {
            el.child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_none()
                            .px(px(7.))
                            .py(px(2.))
                            .rounded(chip_radius)
                            .bg(theme.danger.opacity(0.14))
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.danger)
                            .child("FAILED"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(err),
                    ),
            )
        })
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .pt_1()
                .when(!needs_key, |el| {
                    el.child(
                        Button::new("cursor-setup-install")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Small)
                            .label(primary_label)
                            .disabled(doctor_busy && !installing)
                            .loading(installing)
                            .loading_style(ButtonLoadingStyle::GridShift)
                            .on_click(move |_, window, cx| {
                                crate::pages::doctor::request_fix(window, cx, "harness:cursor");
                            }),
                    )
                })
                .child(
                    Button::new("cursor-setup-refresh")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .label("Check again")
                        .disabled(doctor_busy)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_refresh_catalogs(cx);
                            Bridge::doctor_refresh(cx);
                        }),
                ),
        )
}

pub(super) fn antigravity_setup_card(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let warning = theme.warning;
    let muted = theme.muted_foreground;
    let border = theme.border;
    let surface = theme.popover;
    let chip_bg = warning.opacity(0.14);
    let chip_radius = px(6.);
    let is_installing = state.coder.agent_installing.as_deref() == Some("antigravity");
    let install_busy = state.coder.agent_installing.is_some();
    let install_error = state.coder.agent_install_error.clone();
    let setup_hint = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
        "Install Google's Antigravity ACP server (agy_acp_server) from the ACP registry.".into()
    });
    let primary_label = if is_installing {
        "Installing ACP server…"
    } else {
        "Install ACP server"
    };

    div()
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
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(28.))
                        .rounded(chip_radius)
                        .bg(chip_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::Cpu)
                                .size(px(14.))
                                .color(warning),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .px(px(7.))
                                .py(px(2.))
                                .rounded(chip_radius)
                                .bg(chip_bg)
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(warning)
                                .child(if is_installing { "WORKING" } else { "SETUP" }),
                        )
                        .child(
                            div()
                                .text_sm()
                                .line_height(CARD_TITLE_LINE)
                                .font_weight(FontWeight::NORMAL)
                                .child("Set up Antigravity ACP"),
                        ),
                ),
        )
        .child(
            div()
                .text_sm()
                .line_height(CARD_TITLE_LINE)
                .text_color(theme.foreground)
                .child(setup_hint),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Halo drives Google's official `agy_acp_server` over ACP (JSON-RPC stdio)."),
        )
        .when_some(install_error, |el, err| {
            el.child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    // An error is multi-line; centre-aligning it against the chip
                    // floats the badge into the middle of the paragraph.
                    .items_start()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_none()
                            .px(px(7.))
                            .py(px(2.))
                            .rounded(chip_radius)
                            .bg(theme.danger.opacity(0.14))
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.danger)
                            .child("FAILED"),
                    )
                    .child(
                        // `min_w_0` is what lets a long probe message wrap instead
                        // of running past the card's right edge.
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(err),
                    ),
            )
        })
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .pt_1()
                .child(
                    Button::new("antigravity-setup-install")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .label(primary_label)
                        // `disabled` cancels loading styling — only grey out when
                        // another harness install is in flight.
                        .disabled(install_busy && !is_installing)
                        .loading(is_installing)
                        .loading_style(ButtonLoadingStyle::GridShift)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_install_antigravity(cx);
                        }),
                )
                .child(
                    Button::new("antigravity-setup-refresh")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .label("Check again")
                        .disabled(install_busy)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_refresh_catalogs(cx);
                        }),
                )
                .child(
                    Button::new("antigravity-setup-settings")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Gear)
                        .label("Custom path…")
                        .disabled(install_busy)
                        .on_click(|_, _, cx| {
                            crate::windows::open_settings_window(SettingsTab::Agent, cx);
                        }),
                ),
        )
}

pub(super) fn pi_setup_card(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let warning = theme.warning;
    let muted = theme.muted_foreground;
    let border = theme.border;
    let surface = theme.popover;
    let chip_bg = warning.opacity(0.14);
    let chip_radius = px(6.);
    let is_installing = state.coder.agent_installing.as_deref() == Some("pi");
    let install_busy = state.coder.agent_installing.is_some();
    let needs_upgrade = state.coder.agent_pi_needs_upgrade;
    let install_error = state.coder.agent_install_error.clone();
    let setup_hint = state.coder.agent_pi_setup_hint.clone().unwrap_or_else(|| {
        "Install @earendil-works/pi-coding-agent 1.4.0 to use Halo Agent.".into()
    });
    let title = if needs_upgrade {
        "Upgrade Pi Coding Agent"
    } else {
        "Set up Pi Coding Agent"
    };
    let primary_label = if is_installing {
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

    div()
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
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(28.))
                        .rounded(chip_radius)
                        .bg(chip_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::Cpu)
                                .size(px(14.))
                                .color(warning),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .px(px(7.))
                                .py(px(2.))
                                .rounded(chip_radius)
                                .bg(chip_bg)
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(warning)
                                .child(if is_installing {
                                    "WORKING"
                                } else if needs_upgrade {
                                    "UPGRADE"
                                } else {
                                    "SETUP"
                                }),
                        )
                        .child(
                            div()
                                .text_sm()
                                .line_height(CARD_TITLE_LINE)
                                .font_weight(FontWeight::NORMAL)
                                .child(title),
                        ),
                ),
        )
        .child(
            div()
                .text_sm()
                .line_height(CARD_TITLE_LINE)
                .text_color(theme.foreground)
                .child(setup_hint),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(
                    "Halo drives Pi over native JSON-L RPC (`pi --mode rpc`). This one-time setup needs Node.js ≥ 22.19 and npm.",
                ),
        )
        .when_some(install_error, |el, err| {
            el.child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    // An error is multi-line; centre-aligning it against the chip
                    // floats the badge into the middle of the paragraph.
                    .items_start()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_none()
                            .px(px(7.))
                            .py(px(2.))
                            .rounded(chip_radius)
                            .bg(theme.danger.opacity(0.14))
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.danger)
                            .child("FAILED"),
                    )
                    .child(
                        // `min_w_0` is what lets a long probe message wrap instead
                        // of running past the card's right edge.
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(err),
                    ),
            )
        })
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .pt_1()
                .child(
                    Button::new("install-pi-agent")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .label(primary_label)
                        // `disabled` cancels loading styling — only grey out when
                        // another harness install is in flight.
                        .disabled(install_busy && !is_installing)
                        .loading(is_installing)
                        .loading_style(ButtonLoadingStyle::GridShift)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_install_pi(cx);
                        }),
                )
                .child(
                    Button::new("pi-setup-refresh")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .label("Check again")
                        .disabled(install_busy)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::halo_agent_refresh_catalogs(cx);
                        }),
                )
                .child(
                    Button::new("pi-setup-settings")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Gear)
                        .label("Custom path…")
                        .disabled(install_busy)
                        .on_click(|_, _, cx| {
                            crate::windows::open_settings_window(SettingsTab::Agent, cx);
                        }),
                ),
        )
}

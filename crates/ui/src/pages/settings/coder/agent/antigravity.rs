//! Antigravity harness setup panel and ACP auth row.

use super::*;

pub(super) fn antigravity_setup(
    settings: &CoderSettings,
    editors: &ToolEditors,
    installing: Option<&str>,
    cx: &App,
) -> impl IntoElement {
    let installing_antigravity = installing == Some("antigravity");
    let install_busy = installing.is_some();
    let path_summary = settings
        .antigravity_executable_path
        .as_deref()
        .unwrap_or("Default (managed install / $PATH)");
    let acp_login = Bridge::antigravity_acp_login_status(cx);
    let acp_signed_in = Bridge::antigravity_acp_signed_in(cx);

    section(
        "Antigravity ACP server",
        "Managed installation or custom binary path for Google's Antigravity ACP server (`agy_acp_server`).",
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
                            .child(Input::new(&editors.coder_settings_antigravity_path)),
                    )
                    .child(action("coder-antigravity-path-apply", "Apply", {
                        let input = editors.coder_settings_antigravity_path.clone();
                        move |cx| {
                            let val = input.read(cx).value().trim().to_string();
                            let path = if val.is_empty() { None } else { Some(val) };
                            Bridge::update_coder_settings(cx, move |s| {
                                s.antigravity_executable_path = path.clone();
                            });
                            let _ = Bridge::halo_agent_refresh_catalogs(cx);
                        }
                    }))
                    .child(
                        Button::new("coder-antigravity-install")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Small)
                            .label(if installing_antigravity {
                                "Installing…"
                            } else {
                                "Install"
                            })
                            // `disabled` cancels loading styling — only grey
                            // out when another harness install is in flight.
                            .disabled(install_busy && !installing_antigravity)
                            .loading(installing_antigravity)
                            .loading_style(ButtonLoadingStyle::GridShift)
                            .on_click(|_, _, cx| {
                                let _ = Bridge::halo_agent_install_antigravity(cx);
                            }),
                    )
                    .when(settings.antigravity_executable_path.is_some(), |el| {
                        el.child(action("coder-antigravity-path-clear", "Reset", |cx| {
                            Bridge::update_coder_settings(cx, |s| {
                                s.antigravity_executable_path = None;
                            });
                            let _ = Bridge::halo_agent_refresh_catalogs(cx);
                        }))
                    }),
            )
            .child(antigravity_acp_auth_row(&acp_login, acp_signed_in, cx)),
        cx,
    )
}

pub(super) fn antigravity_acp_auth_row(
    login: &AntigravityLoginStatus,
    signed_in: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let waiting = login.is_waiting();
    let fail = match login {
        AntigravityLoginStatus::Failed { message } => Some(message.as_str()),
        _ => None,
    };
    // Keep the sign-in page reachable when the browser did not come up.
    let auth_url = match login {
        AntigravityLoginStatus::Waiting { auth_url } => auth_url.clone(),
        _ => None,
    };
    let status = if waiting {
        "Complete Google sign-in in your browser to continue…".to_string()
    } else if let Some(message) = fail {
        message.to_string()
    } else if signed_in {
        "Signed in with Google".to_string()
    } else {
        "Not signed in — sign in with your Google account to use Antigravity models.".to_string()
    };
    let status_color = if fail.is_some() {
        theme.danger
    } else if signed_in && !waiting {
        theme.success
    } else {
        muted
    };

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
                .text_xs()
                .text_color(status_color)
                .child(status),
        )
        .when_some(auth_url, |el, url| {
            el.child(action(
                "coder-antigravity-acp-open-url",
                "Open sign-in page",
                move |cx| {
                    cx.open_url(&url);
                },
            ))
        })
        .child(if signed_in && !waiting {
            Button::new("coder-antigravity-acp-sign-out")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label("Sign out")
                .on_click(|_, _, cx| {
                    let _ = Bridge::coder_antigravity_acp_logout(cx);
                })
                .into_any_element()
        } else {
            Button::new("coder-antigravity-acp-sign-in")
                .variant(if waiting {
                    ButtonVariant::Ghost
                } else {
                    ButtonVariant::Primary
                })
                .size(ButtonSize::Small)
                .leading_icon(if waiting {
                    PhosphorIcon::X
                } else {
                    PhosphorIcon::GoogleLogo
                })
                .label(if waiting {
                    "Cancel"
                } else if fail.is_some() {
                    "Try again"
                } else {
                    "Sign in with Google"
                })
                .on_click(|_, _, cx| {
                    if Bridge::antigravity_acp_login_status(cx).is_waiting() {
                        Bridge::coder_antigravity_acp_cancel_login(cx);
                        return;
                    }
                    Bridge::coder_antigravity_acp_clear_login_error(cx);
                    let _ = Bridge::coder_antigravity_acp_begin_login(cx);
                })
                .into_any_element()
        })
}

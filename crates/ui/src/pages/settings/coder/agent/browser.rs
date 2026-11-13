//! Browser / computer-use tooling panels.

use super::*;

pub(super) fn browser_setup(
    status: &halo_bridge::BrowserStatus,
    loading: bool,
    cx: &App,
) -> impl IntoElement {
    let title = if loading {
        "Checking…"
    } else if status.available {
        "Ready"
    } else {
        "Not installed"
    };

    let detail = if loading {
        "Checking for agent-browser…".to_string()
    } else if status.available {
        let mut line = status
            .version
            .clone()
            .unwrap_or_else(|| "agent-browser".to_string());
        line.push_str(" · one session per chat");
        match status.allowed_domains.len() {
            0 => line.push_str(" · any domain"),
            1 => line.push_str(" · 1 allowed domain"),
            count => line.push_str(&format!(" · {count} allowed domains")),
        }
        if let Some(cdp) = status.cdp.as_deref() {
            line.push_str(&format!(" · attached via CDP {cdp}"));
        }
        line
    } else {
        status.detail.clone()
    };

    let available = status.available;

    rows()
        .child(row(
            title,
            detail,
            div()
                .flex()
                .gap_2()
                .when(available, |el| {
                    el.child(action("coder-browser-close", "Close all", |cx| {
                        let _ = Bridge::halo_agent_browser_close_all(cx);
                    }))
                })
                .child(action("coder-browser-refresh", "Refresh", |cx| {
                    let _ = Bridge::halo_agent_refresh_browser(cx);
                })),
            cx,
        ))
        .when(!available && !loading, |el| {
            el.child(row(
                "Install",
                "Run `npm i -g agent-browser && agent-browser install`, or set HALO_BROWSER_BIN to an existing binary.",
                div(),
                cx,
            ))
        })
}

pub(super) fn computer_use_setup(state: &BridgeState, cx: &App) -> impl IntoElement {
    let settings = &state.coder_settings;
    let status = &state.coder.agent_desktop;
    let loading = state.coder.agent_desktop_loading;
    let theme = cx.theme();

    let engine_detail = if loading {
        "Checking for agent-desktop…".to_string()
    } else {
        status.detail.clone()
    };
    let access = grant_label(status.accessibility);
    let screen = grant_label(status.screen_recording);
    let show_access_guide = status.installed && status.accessibility == Some(false);
    let show_screen_guide = status.installed && status.screen_recording == Some(false);

    rows()
        .child(row(
            "Computer use",
            "The agent may read and operate desktop apps.",
            switch("coder-computer-use", settings.computer_use, |next, cx| {
                Bridge::update_coder_settings(cx, move |s| {
                    s.computer_use = next;
                });
            }),
            cx,
        ))
        .child(row(
            "Engine",
            engine_detail,
            action("coder-desktop-refresh", "Refresh", |cx| {
                let _ = Bridge::halo_agent_refresh_desktop(cx);
            }),
            cx,
        ))
        .child(row(
            "Accessibility",
            access,
            div().when(show_access_guide, |el| {
                el.child(action(
                    "coder-desktop-access-guide",
                    "Open permission guide",
                    |cx| {
                        let _ = Bridge::halo_agent_open_desktop_permissions(cx);
                    },
                ))
            }),
            cx,
        ))
        .child(row(
            "Screen Recording",
            format!("{screen} · Screenshots only"),
            div().when(show_screen_guide, |el| {
                el.child(action(
                    "coder-desktop-screen-guide",
                    "Open permission guide",
                    |cx| {
                        let _ = Bridge::halo_agent_open_desktop_permissions(cx);
                    },
                ))
            }),
            cx,
        ))
        .child(headed_row(settings.computer_use_headed, cx))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Blocked: Passwords, System Settings, Terminal, and Halo."),
        )
}

pub(super) fn grant_label(state: Option<bool>) -> &'static str {
    match state {
        Some(true) => "Granted",
        Some(false) => "Denied",
        None => "Unknown",
    }
}

/// Same rhythm as [`row`], with a danger label while physical input is on.
pub(super) fn headed_row(on: bool, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let label_color = if on { theme.danger } else { theme.foreground };
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(label_color)
                        .child("Physical input"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("This run can move the pointer and type."),
                ),
        )
        .child(
            div()
                .flex_none()
                .child(switch("coder-computer-use-headed", on, |next, cx| {
                    Bridge::update_coder_settings(cx, move |s| {
                        s.computer_use_headed = next;
                    });
                })),
        )
}

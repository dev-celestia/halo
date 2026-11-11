//! Proxy controls for Settings: status, start/stop actions, and feedback.

use celestia_ui::components::HorizontalAlignment;
use celestia_ui::components::VStack;
use celestia_ui::components::primitive::kbd::Kbd;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::ProxyStatus;

use crate::actions::primary;
use crate::components::{ProxyActionState, ProxyButtonCopy, proxy_start_button, proxy_stop_button};
use crate::pages::chrome::{StatusTone, section, status_pill};

pub fn render_proxy_controls(
    proxy: &ProxyStatus,
    proxy_busy: bool,
    last_message: Option<&str>,
    last_error: Option<&str>,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let starting = proxy_busy && !proxy.running;
    let stopping = proxy_busy && proxy.running;
    let action = ProxyActionState {
        running: proxy.running,
        busy: proxy_busy,
    };

    let status_label: SharedString = if starting {
        "Starting…".into()
    } else if stopping {
        "Stopping…".into()
    } else if proxy.running {
        match (proxy.port, proxy.tls_port) {
            (Some(http), Some(tls)) => format!("Running — HTTP {http} / TLS {tls}").into(),
            (Some(http), None) => format!("Running — HTTP {http}").into(),
            _ => "Running".into(),
        }
    } else {
        "Stopped".into()
    };

    let tone = if proxy_busy {
        StatusTone::Busy
    } else if proxy.running {
        StatusTone::Active
    } else {
        StatusTone::Idle
    };

    let mut controls = VStack::new(HorizontalAlignment::Leading)
        .spacing(px(12.))
        .child(status_pill(status_label, tone, cx))
        .child(
            div()
                .flex()
                .gap_2()
                .child(proxy_start_button(
                    "proxy-start",
                    action,
                    ProxyButtonCopy::Full,
                ))
                .child(proxy_stop_button(
                    "proxy-stop",
                    action,
                    ProxyButtonCopy::Full,
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .text_color(muted)
                .child("Toggle with")
                .child(Kbd::new(
                    Keystroke::parse(&primary("shift-p")).expect("valid keystroke"),
                )),
        );

    if let Some(msg) = last_message {
        controls = controls.child(
            div()
                .id("proxy-status-msg")
                .role(Role::Status)
                .text_xs()
                .text_color(muted)
                .child(msg.to_string()),
        );
    }
    if let Some(err) = last_error {
        controls = controls.child(
            div()
                .id("proxy-status-err")
                .role(Role::Alert)
                .text_xs()
                .text_color(cx.theme().danger)
                .child(format!(
                    "Unable to complete: {err}. Check the ports and try again."
                )),
        );
    }

    section(
        "Proxy",
        Some("Start or stop the MITM listener. Configure ports in the section below.".into()),
        controls,
        cx,
    )
}

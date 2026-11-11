//! CA certificate management — install, export, regenerate, and guides.

use celestia_ui::components::HorizontalAlignment;
use celestia_ui::components::VStack;
use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::section;

use super::constants::{HOW_IT_WORKS, INSTALLATION_GUIDES, TROUBLESHOOTING};

pub fn render_ca_tab(state: &BridgeState, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let busy = state.ca_busy;

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .child(section(
            "Important Security Notice",
            Some(
                "Managed Chromium (Open Browser) uses an isolated profile. Install the system CA only when intercepting external browsers or apps."
                    .into(),
            ),
            div()
                .text_xs()
                .text_color(muted)
                .child("Never share your Halo CA private key. Regenerate if it may be compromised."),
            cx,
        ))
        .child(section(
            "Certificate Actions",
            Some("Install, download, or regenerate the MITM root CA.".into()),
            VStack::new(HorizontalAlignment::Leading)
                .spacing(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("ca-trust")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Key)
                                .label(if busy {
                                    "Working…"
                                } else if cfg!(target_os = "macos") {
                                    "Install to macOS Keychain"
                                } else {
                                    "Install to Chrome NSS"
                                })
                                .disabled(busy)
                                .loading(busy)
                                .loading_style(ButtonLoadingStyle::GridShift)
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::trust_ca(cx);
                                }),
                        )
                        .child(
                            Button::new("ca-export")
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::DownloadSimple)
                                .label(if busy { "Saving…" } else { "Download CA Certificate" })
                                .disabled(busy)
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::export_ca_cert(cx);
                                }),
                        )
                        .child(
                            Button::new("ca-regen")
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::ArrowClockwise)
                                .label(if busy {
                                    "Regenerating…"
                                } else {
                                    "Regenerate CA"
                                })
                                .disabled(busy)
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::regenerate_ca_cert(cx);
                                }),
                        ),
                )
                .child(feedback_line(state, cx)),
            cx,
        ))
        .child(render_guides(cx))
        .child(render_how_it_works(cx))
        .child(render_troubleshooting(cx))
}

fn feedback_line(state: &BridgeState, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    if let Some(err) = state.last_error.as_ref() {
        div()
            .id("ca-err")
            .role(Role::Alert)
            .text_xs()
            .text_color(cx.theme().danger)
            .child(err.clone())
    } else if let Some(msg) = state.last_message.as_ref() {
        div()
            .id("ca-msg")
            .role(Role::Status)
            .text_xs()
            .text_color(muted)
            .child(msg.clone())
    } else {
        div()
            .id("ca-hint")
            .text_xs()
            .text_color(muted)
            .child("Download opens a save dialog (defaults to Downloads/halo-ca.pem).")
    }
}

fn render_guides(cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let mut stack = VStack::new(HorizontalAlignment::Leading).spacing(px(14.));

    for guide in INSTALLATION_GUIDES {
        let mut steps = div().flex().flex_col().gap_1();
        for (i, step) in guide.steps.iter().enumerate() {
            steps = steps.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{}. {step}", i + 1)),
            );
        }
        if let Some(note) = guide.note {
            steps = steps.child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(cx.theme().warning)
                    .child(note.to_string()),
            );
        }
        stack = stack
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::NORMAL)
                    .child(guide.title),
            )
            .child(steps);
    }

    section(
        "Installation Guides",
        Some("Step-by-step import for external browsers.".into()),
        stack,
        cx,
    )
}

fn render_how_it_works(cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let mut stack = VStack::new(HorizontalAlignment::Leading).spacing(px(10.));
    for (title, body) in HOW_IT_WORKS {
        stack = stack.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!("{title}: {body}")),
        );
    }
    section(
        "How It Works",
        Some("Certificate-based MITM lifecycle.".into()),
        stack,
        cx,
    )
}

fn render_troubleshooting(cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let mut stack = VStack::new(HorizontalAlignment::Leading).spacing(px(10.));
    for (title, body) in TROUBLESHOOTING {
        stack = stack
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::NORMAL)
                    .child(*title),
            )
            .child(div().text_xs().text_color(muted).child(*body));
    }
    section(
        "Troubleshooting",
        Some("Common trust and pinning issues.".into()),
        stack,
        cx,
    )
}

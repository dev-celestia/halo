//! Language Servers section — detect and install LSP binaries for Coder.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::settings::widgets::{action, section};

pub(in crate::pages::settings) fn render(state: &BridgeState, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let installing = state.coder_lsp_installing_id.clone();
    let message = state.coder_lsp_install_message.clone();
    let servers = state.coder_lsp_servers.clone();

    let mut list = div().w_full().flex().flex_col().gap_3();

    if servers.is_empty() {
        list = list.child(
            div()
                .text_xs()
                .text_color(muted)
                .child("No servers loaded yet. Click Refresh to scan PATH."),
        );
    }

    for server in servers {
        let id = server.id.clone();
        let installing_this = installing.as_deref() == Some(id.as_str());
        let busy = installing.is_some();
        let installed = server.installed;
        let can_install = server.can_auto_install && !installed;

        let status = if installing_this {
            "Installing…".to_string()
        } else if installed {
            "Installed".to_string()
        } else {
            "Not on PATH".to_string()
        };

        let mut controls = div().flex().items_center().gap_2();
        controls = controls.child(
            div()
                .text_xs()
                .text_color(if installed {
                    cx.theme().foreground
                } else {
                    muted
                })
                .child(status),
        );

        if can_install {
            let install_id = id.clone();
            let btn_id = SharedString::from(format!("coder-lsp-install-{id}"));
            controls = controls.child(
                Button::new(ElementId::Name(btn_id))
                    .variant(ButtonVariant::Primary)
                    .size(ButtonSize::Small)
                    .label(if installing_this {
                        "Installing…"
                    } else {
                        "Install"
                    })
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        Bridge::coder_lsp_install_server(&install_id, cx);
                    }),
            );
        } else if !installed {
            controls = controls.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .max_w(px(220.))
                    .child(server.install_hint.clone()),
            );
        }

        list = list.child(
            div()
                .w_full()
                .flex()
                .items_start()
                .justify_between()
                .gap_4()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border.opacity(0.5))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::NORMAL)
                                .child(server.display_name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!("{} · `{}`", server.languages, server.command)),
                        ),
                )
                .child(div().flex_none().child(controls)),
        );
    }

    let footer_msg = message.unwrap_or_else(|| {
        "Install puts the binary on PATH via rustup / npm / go / pip when available.".into()
    });

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .child(section(
            "Language servers",
            "Detect and install LSP servers used by Coder for diagnostics, completion, and go-to-definition.",
            list,
            cx,
        ))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .child(footer_msg),
                )
                .child(action("coder-lsp-refresh", "Refresh", |cx| {
                    Bridge::coder_lsp_refresh_servers(cx);
                })),
        )
}

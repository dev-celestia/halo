use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, JwtAlgorithm};

use super::helpers::{colorize_token, copy_button};
use crate::pages::chrome::panel;
use crate::pages::editors::ToolEditors;

pub fn render_generator(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div().w_full().flex_1().min_h_0().overflow_hidden().child(
        ResizablePanelGroup::horizontal("jwt-generate-layout")
            .with_handle(true)
            .child(
                resizable_panel()
                    .size(px(420.))
                    .size_range(px(280.)..px(680.))
                    .child(render_generator_config(state, editors, cx)),
            )
            .child(
                resizable_panel().child(
                    div()
                        .size_full()
                        .min_w_0()
                        .overflow_hidden()
                        .child(render_generated_token(state, cx)),
                ),
            ),
    )
}

pub fn render_generator_config(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let alg = state.jwt.gen_algorithm;
    let secret_input = editors.jwt_gen_secret.clone();
    let secret_text = secret_input.read(cx).value().to_string();
    let secret_text_for_copy = secret_text.clone();
    let is_none = alg == JwtAlgorithm::None;

    panel(
        "Configuration",
        None,
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div().w_full().h(px(140.)).overflow_hidden().child(
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.muted_foreground)
                                .child("HEADER JSON"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .child(editors.jwt_gen_header.clone()),
                        ),
                ),
            )
            .child(
                div().w_full().h(px(160.)).overflow_hidden().child(
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.muted_foreground)
                                .child("PAYLOAD JSON"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .child(editors.jwt_gen_payload.clone()),
                        ),
                ),
            )
            .child(algorithm_picker(alg, cx))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(theme.muted_foreground)
                                    .child(if is_none {
                                        "KEY (unused for none)"
                                    } else if alg == JwtAlgorithm::ES512 {
                                        "PRIVATE KEY (PEM) — ES512 not supported in this build"
                                    } else if alg.is_asymmetric() {
                                        "PRIVATE KEY (PEM)"
                                    } else {
                                        "SECRET KEY"
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(copy_button(
                                        "jwt-copy-secret",
                                        secret_text_for_copy,
                                        "Copied secret",
                                    ))
                                    .child(if alg.is_hmac() {
                                        Button::new("jwt-gen-secret")
                                            .variant(ButtonVariant::Secondary)
                                            .size(ButtonSize::Small)
                                            .leading_icon(PhosphorIcon::Key)
                                            .label("Generate Secret")
                                            .on_click(|_, _, cx| {
                                                Bridge::jwt_generate_secret(cx);
                                            })
                                            .into_any_element()
                                    } else if alg.is_asymmetric() && alg != JwtAlgorithm::ES512 {
                                        Button::new("jwt-gen-keypair")
                                            .variant(ButtonVariant::Secondary)
                                            .size(ButtonSize::Small)
                                            .leading_icon(PhosphorIcon::Key)
                                            .label("Generate Key Pair")
                                            .on_click(|_, _, cx| {
                                                Bridge::jwt_generate_keypair(cx);
                                            })
                                            .into_any_element()
                                    } else {
                                        div().into_any_element()
                                    }),
                            ),
                    )
                    .child(div().w_full().child(secret_input.clone())),
            )
            .children(state.jwt.gen_error.clone().map(|error| {
                div()
                    .w_full()
                    .p_2p5()
                    .rounded(theme.radius_sm())
                    .bg(theme.danger.opacity(0.08))
                    .text_color(theme.danger)
                    .text_xs()
                    .font_family("Menlo")
                    .child(error)
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt_1()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("jwt-clear-generate")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::Trash)
                            .label("Reset")
                            .on_click({
                                let header = editors.jwt_gen_header.clone();
                                let payload = editors.jwt_gen_payload.clone();
                                let secret = editors.jwt_gen_secret.clone();
                                move |_, window, cx| {
                                    Bridge::jwt_clear_generate(cx);
                                    header.update(cx, |e, cx| {
                                        e.set_text(halo_bridge::JWT_DEFAULT_HEADER, window, cx)
                                    });
                                    payload.update(cx, |e, cx| {
                                        e.set_text(halo_bridge::JWT_DEFAULT_PAYLOAD, window, cx)
                                    });
                                    secret.update(cx, |s, cx| s.set_value("", window, cx));
                                }
                            }),
                    )
                    .child(
                        Button::new("jwt-generate-btn")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Small)
                            .leading_icon(PhosphorIcon::Key)
                            .label("Generate JWT")
                            .disabled(!is_none && secret_text.trim().is_empty())
                            .on_click({
                                let header = editors.jwt_gen_header.clone();
                                let payload = editors.jwt_gen_payload.clone();
                                let secret = editors.jwt_gen_secret.clone();
                                move |_, _, cx| {
                                    let header_text = header.read(cx).text(cx);
                                    let payload_text = payload.read(cx).text(cx);
                                    let secret_value = secret.read(cx).value().to_string();
                                    Bridge::jwt_generate(
                                        &header_text,
                                        &payload_text,
                                        &secret_value,
                                        cx,
                                    );
                                }
                            }),
                    ),
            ),
        cx,
    )
}

pub fn algorithm_picker(selected: JwtAlgorithm, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mut wrap = div().w_full().flex().flex_col().gap_1();
    for category in ["HMAC", "RSA", "ECDSA", "RSA-PSS", "None"] {
        let mut row = div().flex().flex_wrap().items_center().gap_1();
        row = row.child(
            div()
                .w(px(58.))
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(category),
        );
        for alg in JwtAlgorithm::ALL {
            if alg.category() != category {
                continue;
            }
            let is_selected = alg == selected;
            row = row.child(
                Button::new(SharedString::from(format!("jwt-alg-{}", alg.label())))
                    .variant(if is_selected {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    })
                    .size(ButtonSize::Small)
                    .label(alg.label())
                    .on_click(move |_, _, cx| Bridge::jwt_set_algorithm(alg, cx)),
            );
        }
        wrap = wrap.child(row);
    }
    wrap.into_any_element()
}

pub fn render_generated_token(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let token = state.jwt.generated_token.clone();

    panel(
        "Generated Token",
        None,
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .children(if token.is_empty() {
                        vec![]
                    } else {
                        vec![
                            copy_button("jwt-copy-token", token.clone(), "Copied token")
                                .into_any_element(),
                        ]
                    }),
            )
            .child(
                div()
                    .id("jwt-generated")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .p_3()
                    .rounded(theme.radius_sm())
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .font_family("Menlo")
                    .whitespace_normal()
                    .children(if token.is_empty() {
                        vec![
                            div()
                                .text_color(theme.muted_foreground)
                                .child("Generated JWT token will appear here...")
                                .into_any_element(),
                        ]
                    } else {
                        colorize_token(&token)
                    }),
            ),
        cx,
    )
}

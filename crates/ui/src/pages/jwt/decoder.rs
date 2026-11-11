use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, JwtClaim, JwtDecoded};

use super::findings::render_findings;
use super::helpers::{
    SIGNATURE_COLOR, claim_value_color, copy_button, empty_hint, format_copy_text,
};
use crate::pages::chrome::panel;
use crate::pages::editors::ToolEditors;

pub fn render_decoder(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div().flex_1().min_h_0().overflow_hidden().child(
                ResizablePanelGroup::horizontal("jwt-decode-layout")
                    .with_handle(true)
                    .child(
                        resizable_panel()
                            .size(px(360.))
                            .size_range(px(220.)..px(640.))
                            .child(render_token_input(state, editors, cx)),
                    )
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .min_w_0()
                                .overflow_hidden()
                                .child(render_decoded(state, cx)),
                        ),
                    ),
            ),
        )
        .child(render_findings(state, cx))
}

pub fn render_token_input(
    _state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let has_token = !editors.jwt_token.read(cx).text(cx).trim().is_empty();

    panel(
        "JWT Token",
        Some("Paste a token — decoding is live and fully local.".into()),
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div().flex().items_center().justify_end().child(
                    Button::new("jwt-clear-token")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Trash)
                        .disabled(!has_token)
                        .on_click({
                            let editor = editors.jwt_token.clone();
                            move |_, window, cx| {
                                Bridge::jwt_clear_decode(cx);
                                editor.update(cx, |e, cx| {
                                    e.set_text("", window, cx);
                                });
                            }
                        }),
                ),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(editors.jwt_token.clone()),
            ),
        cx,
    )
}

pub fn render_decoded(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    panel(
        "Decoded Breakdown",
        None,
        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().flex().items_center().justify_between().children(
                if let Some(decoded) = &state.jwt.decoded {
                    vec![
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded(theme.radius_sm())
                            .bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .text_xs()
                            .font_family("Menlo")
                            .child(decoded.algorithm.clone())
                            .into_any_element(),
                        copy_button("jwt-copy-decoded", format_copy_text(decoded), "Copy claims")
                            .into_any_element(),
                    ]
                } else {
                    vec![]
                },
            ))
            .child(div().w_full().flex_1().min_h_0().children(
                if let Some(error) = &state.jwt.decode_error {
                    vec![
                        div()
                            .w_full()
                            .p_2p5()
                            .rounded(theme.radius_sm())
                            .bg(theme.danger.opacity(0.08))
                            .text_color(theme.danger)
                            .text_xs()
                            .font_family("Menlo")
                            .child(error.clone())
                            .into_any_element(),
                    ]
                } else if let Some(decoded) = &state.jwt.decoded {
                    vec![
                        div()
                            .id("jwt-claims")
                            .w_full()
                            .h_full()
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(claims_section("Header", &decoded.header, cx))
                            .child(claims_section("Payload", &decoded.payload, cx))
                            .child(signature_block(decoded, cx))
                            .into_any_element(),
                    ]
                } else {
                    vec![empty_hint("Paste a JWT token to decode.", cx)]
                },
            )),
        cx,
    )
}

pub fn claims_section(title: &str, claims: &[JwtClaim], cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child(format!("{} CLAIMS", title.to_uppercase())),
        )
        .children(claims.iter().map(|claim| {
            div()
                .flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_family("Menlo")
                        .text_color(hsla(215. / 360., 0.6, 0.55, 1.))
                        .child(format!("{}:", claim.key)),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_xs()
                                .font_family("Menlo")
                                .text_color(claim_value_color(&claim.value_kind))
                                .child(claim.value_json.clone()),
                        )
                        .children(claim.timestamp.clone().map(|ts| {
                            div().text_xs().text_color(theme.muted_foreground).child(ts)
                        })),
                )
                .into_any_element()
        }))
        .into_any_element()
}

pub fn signature_block(decoded: &JwtDecoded, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child("SIGNATURE"),
        )
        .child(
            div()
                .text_xs()
                .font_family("Menlo")
                .text_color(SIGNATURE_COLOR)
                .truncate()
                .child(decoded.signature_b64.clone()),
        )
        .into_any_element()
}

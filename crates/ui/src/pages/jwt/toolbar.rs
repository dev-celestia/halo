use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use gpui::*;
use halo_bridge::{Bridge, BridgeState, JwtMode};

pub fn render_jwt_toolbar(state: &BridgeState, _cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(if state.jwt.mode == JwtMode::Decode {
            Button::new("jwt-tab-decode")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .label(format!("Decoder ({})", state.jwt.vulnerabilities.len()))
                .on_click(|_, _, cx| Bridge::jwt_set_mode(JwtMode::Decode, cx))
                .into_any_element()
        } else {
            Button::new("jwt-tab-decode")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .label(format!("Decoder ({})", state.jwt.vulnerabilities.len()))
                .on_click(|_, _, cx| Bridge::jwt_set_mode(JwtMode::Decode, cx))
                .into_any_element()
        })
        .child(if state.jwt.mode == JwtMode::Generate {
            Button::new("jwt-tab-generate")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .label("Generator")
                .on_click(|_, _, cx| Bridge::jwt_set_mode(JwtMode::Generate, cx))
                .into_any_element()
        } else {
            Button::new("jwt-tab-generate")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .label("Generator")
                .on_click(|_, _, cx| Bridge::jwt_set_mode(JwtMode::Generate, cx))
                .into_any_element()
        })
}

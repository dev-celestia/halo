//! JWT page — decode, vulnerability audit, and token forging.

pub mod decoder;
pub mod findings;
pub mod generator;
pub mod helpers;
pub mod toolbar;

use gpui::*;
use halo_bridge::{BridgeState, JwtMode};

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

pub use decoder::render_decoder;
pub use generator::render_generator;
pub use toolbar::render_jwt_toolbar;

pub fn render_jwt(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .id("jwt-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_jwt_toolbar(state, cx))
        .child(match state.jwt.mode {
            JwtMode::Decode => render_decoder(state, editors, cx).into_any_element(),
            JwtMode::Generate => render_generator(state, editors, cx).into_any_element(),
        })
}

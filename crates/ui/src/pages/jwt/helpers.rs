use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;

pub const HEADER_COLOR: Hsla = hsla(0. / 360., 0.75, 0.52, 1.);
pub const PAYLOAD_COLOR: Hsla = hsla(285. / 360., 0.6, 0.65, 1.);
pub const SIGNATURE_COLOR: Hsla = hsla(190. / 360., 0.75, 0.48, 1.);

/// Tri-color token rendering: header red, payload purple, signature cyan.
pub fn colorize_token(token: &str) -> Vec<AnyElement> {
    let parts: Vec<&str> = token.split('.').collect();
    let colors = [HEADER_COLOR, PAYLOAD_COLOR, SIGNATURE_COLOR];
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        out.push(
            div()
                .flex_none()
                .text_color(colors[i.min(2)])
                .child(part.to_string())
                .into_any_element(),
        );
        if i + 1 < parts.len() {
            out.push(
                div()
                    .flex_none()
                    .text_color(hsla(0., 0., 0.5, 1.))
                    .child(".")
                    .into_any_element(),
            );
        }
    }
    out
}

pub fn claim_value_color(kind: &str) -> Hsla {
    match kind {
        "string" => hsla(145. / 360., 0.6, 0.4, 1.),
        "number" => hsla(38. / 360., 0.9, 0.45, 1.),
        "boolean" => hsla(280. / 360., 0.55, 0.55, 1.),
        "null" => hsla(0., 0., 0.5, 1.),
        _ => hsla(190. / 360., 0.75, 0.42, 1.),
    }
}

pub fn format_copy_text(decoded: &halo_bridge::JwtDecoded) -> String {
    let header: Vec<String> = decoded
        .header
        .iter()
        .map(|c| format!("  {}: {}", c.key, c.value_json))
        .collect();
    let payload: Vec<String> = decoded
        .payload
        .iter()
        .map(|c| format!("  {}: {}", c.key, c.value_json))
        .collect();
    format!(
        "Header:\n{{\n{}\n}}\n\nPayload:\n{{\n{}\n}}",
        header.join(",\n"),
        payload.join(",\n")
    )
}

pub fn copy_button(id: &str, text: String, toast: &str) -> Button {
    let text = text.clone();
    let toast = toast.to_string();
    Button::new(SharedString::from(id.to_string()))
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::Copy)
        .label("Copy")
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            let _ = toast.clone();
        })
}

pub fn empty_hint(text: &str, cx: &App) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
        .into_any_element()
}

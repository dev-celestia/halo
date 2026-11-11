//! Shared semantic colors for HTTP method / status — one meaning per hue.

use gpui::{Hsla, hsla};

/// Method hue: interactive identity in traffic tables (not a theme accent).
/// Tuned toward Insomnia-style request trees (GET purple, PATCH olive).
pub fn method_color(method: &str) -> Hsla {
    match method.to_ascii_uppercase().as_str() {
        "GET" => hsla(268. / 360., 0.62, 0.62, 1.),
        "POST" => hsla(145. / 360., 0.58, 0.45, 1.),
        "PUT" => hsla(28. / 360., 0.88, 0.55, 1.),
        "DELETE" => hsla(0. / 360., 0.72, 0.58, 1.),
        "PATCH" => hsla(72. / 360., 0.45, 0.48, 1.),
        "OPTIONS" | "HEAD" => hsla(200. / 360., 0.15, 0.55, 1.),
        _ => hsla(0., 0., 0.55, 1.),
    }
}

/// Compact method label for dense trees (Insomnia: PTCH / DEL).
pub fn method_short(method: &str) -> &'static str {
    match method.to_ascii_uppercase().as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "DELETE" => "DEL",
        "PATCH" => "PTCH",
        "OPTIONS" => "OPT",
        "HEAD" => "HEAD",
        _ => "HTTP",
    }
}

/// Status-class hue: success / redirect / client error / server error.
pub fn status_color(status: Option<u16>) -> Hsla {
    match status {
        Some(s) if (200..300).contains(&s) => hsla(150. / 360., 0.65, 0.40, 1.),
        Some(s) if (300..400).contains(&s) => hsla(200. / 360., 0.70, 0.50, 1.),
        Some(s) if (400..500).contains(&s) => hsla(38. / 360., 0.90, 0.50, 1.),
        Some(s) if (500..600).contains(&s) => hsla(350. / 360., 0.70, 0.55, 1.),
        _ => hsla(0., 0., 0.50, 1.),
    }
}

pub fn status_color_u16(status: u16) -> Hsla {
    status_color(Some(status))
}

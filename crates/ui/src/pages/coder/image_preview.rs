//! Read-only image preview for Coder tabs (png/jpg/gif/svg/…).

use std::path::PathBuf;

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::CoderFileKind;

pub(super) fn is_image_tab(kind: CoderFileKind) -> bool {
    kind == CoderFileKind::Image
}

/// Image surface only — zoom chrome lives in the shared pane floating bar.
pub(super) fn render_image_preview(path: &str, zoom_pct: u16, cx: &App) -> impl IntoElement {
    let path_buf = PathBuf::from(path);
    let muted = cx.theme().muted_foreground;
    let scale = (zoom_pct as f32 / 100.0).max(0.25);

    div()
        .id(SharedString::from(format!("coder-image-{path}")))
        .size_full()
        .min_h_0()
        .overflow_scroll()
        .flex()
        .items_center()
        .justify_center()
        .p_3()
        .child({
            // Neutral edge so the bitmap does not float into varied chrome —
            // pure black/white at 10%, never a tinted slate.
            let outline = if cx.theme().is_dark() {
                hsla(0., 0., 1., 0.10)
            } else {
                hsla(0., 0., 0., 0.10)
            };
            img(path_buf)
                .id(SharedString::from(format!("coder-image-src-{path}")))
                .w(relative(scale))
                .h(relative(scale))
                .object_fit(ObjectFit::Contain)
                .border_1()
                .border_color(outline)
                .with_fallback({
                    let muted = muted;
                    move || {
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Could not load image")
                            .into_any_element()
                    }
                })
        })
}

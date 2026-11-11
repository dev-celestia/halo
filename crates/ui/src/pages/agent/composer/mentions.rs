//! Mention chips and image attachment pill renderers for prompt composer.

use std::path::PathBuf;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::InlineTokenContext;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::Bridge;

use super::super::completion::{FILE_TOKEN_PREFIX, SKILL_TOKEN_PREFIX};
use crate::explorer_icons;

/// Max size of the hover thumbnail shown over an attachment chip.
const PREVIEW_W: Pixels = px(160.);
const PREVIEW_H: Pixels = px(120.);

/// Hover view for an attached image — mounted through gpui's native `.tooltip`.
struct AttachmentHoverPreview {
    path: SharedString,
    name: SharedString,
}

impl Render for AttachmentHoverPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let outline = if cx.theme().is_dark() {
            hsla(0., 0., 1., 0.10)
        } else {
            hsla(0., 0., 0., 0.10)
        };
        let path = self.path.clone();
        let name = self.name.clone();
        let img_id = SharedString::from(format!("coder-agent-attach-preview-{path}"));

        div()
            .m_3()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(6.))
            .font_family(cx.theme().font_family.clone())
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_md()
            .rounded(cx.theme().radius)
            .child(
                img(PathBuf::from(path.as_ref()))
                    .id(img_id)
                    .w(PREVIEW_W)
                    .h(PREVIEW_H)
                    .object_fit(ObjectFit::Contain)
                    .rounded(px(4.))
                    .border_1()
                    .border_color(outline)
                    .with_fallback({
                        let muted = muted;
                        move || {
                            div()
                                .w(PREVIEW_W)
                                .h(PREVIEW_H)
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(muted)
                                .child("Could not load image")
                                .into_any_element()
                        }
                    }),
            )
            .child(
                div()
                    .max_w(PREVIEW_W)
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(muted)
                    .child(name),
            )
    }
}

/// Build the hover preview view for an attachment path (composer + transcript).
pub(crate) fn attachment_preview_tooltip(
    path: impl Into<SharedString>,
    name: impl Into<SharedString>,
    cx: &mut App,
) -> AnyView {
    let path = path.into();
    let name = name.into();
    cx.new(|_| AttachmentHoverPreview { path, name }).into()
}

/// Cursor-style mention chip: file = blue + type icon, skill = warm orange.
pub(super) fn render_mention_token(
    ctx: &InlineTokenContext,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = ctx.token().id().as_ref();
    let label = ctx.token().label().clone();
    let is_file = id.starts_with(FILE_TOKEN_PREFIX);
    let (file_color, skill_color) = mention_colors(cx);
    let color = if id.starts_with(SKILL_TOKEN_PREFIX) {
        skill_color
    } else {
        file_color
    };
    let selected = ctx.is_selected();
    let accent = cx.theme().accent;

    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .h(ctx.line_height())
        .max_w(ctx.available_width())
        .when(selected, |el| el.rounded(px(4.)).bg(accent))
        .when(ctx.is_disabled(), |el| el.opacity(0.5))
        .when(is_file, |el| {
            el.child(explorer_icons::file_icon(label.as_ref()).into_any_element())
        })
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(color)
                .child(label),
        )
        .into_any_element()
}

/// Soft blue for `@` files, warm orange for `/` skills — readable on both themes.
fn mention_colors(cx: &App) -> (Hsla, Hsla) {
    if cx.theme().is_dark() {
        (
            hsla(205. / 360., 0.28, 0.72, 1.),
            hsla(28. / 360., 0.58, 0.66, 1.),
        )
    } else {
        (
            hsla(205. / 360., 0.55, 0.42, 1.),
            hsla(24. / 360., 0.70, 0.42, 1.),
        )
    }
}

/// Chip label for an attachment. Clipboard images are persisted as
/// `paste-<uuid>.<ext>` temp files — show a friendly name instead of the UUID;
/// the real filename stays available in the hover tooltip.
pub(crate) fn attachment_label(path: &str) -> String {
    let Some(name) = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return path.to_string();
    };
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return name.to_string();
    };
    let pasted = stem.strip_prefix("paste-").is_some_and(|id| {
        id.len() == 36
            && id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    });
    if pasted {
        format!("Pasted image · {}", ext.to_ascii_uppercase())
    } else {
        name.to_string()
    }
}

pub(super) fn render_attachment_chips(paths: &[String], cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let surface = theme.secondary;
    let text = theme.secondary_foreground;
    let muted = theme.muted_foreground;
    let radius = theme.radius_sm();
    div()
        .w_full()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.))
        .px(px(2.))
        .children(paths.iter().cloned().enumerate().map(|(i, path)| {
            let name = attachment_label(&path);
            let remove_path = path.clone();
            let preview_path = path.clone();
            let tip_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&path)
                .to_string();
            div()
                .id(("coder-agent-attach-chip", i))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.))
                .px(px(8.))
                .py(px(3.))
                .rounded(radius)
                .bg(surface)
                .tooltip(move |_, cx| {
                    attachment_preview_tooltip(preview_path.clone(), tip_name.clone(), cx)
                })
                .child(
                    Phosphor::new(PhosphorIcon::Image)
                        .size(px(12.))
                        .color(muted),
                )
                .child(
                    div()
                        .max_w(px(180.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_xs()
                        .line_height(px(16.))
                        .text_color(text)
                        .child(name),
                )
                .child(
                    Button::icon(
                        SharedString::from(format!("coder-agent-attach-rm-{i}")),
                        PhosphorIcon::X,
                    )
                    .variant(ButtonVariant::Quiet)
                    .size(ButtonSize::IconSm)
                    .accessibility_label("Remove attachment")
                    .tooltip("Remove")
                    .on_click(move |_, _, cx| {
                        let _ = Bridge::halo_agent_remove_attachment(&remove_path, cx);
                    }),
                )
        }))
}

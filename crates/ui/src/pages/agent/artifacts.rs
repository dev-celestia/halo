//! Agent artifact rows — Write/Edit file changes listed in the Context rail.

use std::path::Path;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, HaloAgentFileChange, HaloAgentFileChangeKind};

use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};

pub(super) fn artifact_row(
    change: &HaloAgentFileChange,
    editors: &[Entity<celestia_ui::components::CodeEditor>],
    foreground: Hsla,
    muted: Hsla,
    accent: Hsla,
    success: Hsla,
    warning: Hsla,
    row_radius: Pixels,
) -> impl IntoElement {
    let path = change.path.clone();
    let path_ref = Path::new(&change.path);
    let name = path_ref
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(change.path.as_str())
        .to_string();
    let dir = path_ref
        .parent()
        .and_then(|p| {
            let s = p.to_string_lossy();
            if s.is_empty() || s == "." {
                None
            } else {
                Some(s.to_string())
            }
        })
        .unwrap_or_default();
    let (badge, badge_color) = match change.kind {
        HaloAgentFileChangeKind::Write => ("A", success),
        HaloAgentFileChangeKind::Edit => ("M", warning),
    };
    let editors = editors.to_vec();

    div()
        .id(SharedString::from(format!("coder-agent-artifact-{path}")))
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(4.))
        .py(px(5.))
        .rounded(row_radius)
        .cursor_pointer()
        .hover(move |s| s.bg(accent))
        .on_click(move |_, window, cx| {
            let _ = Bridge::halo_agent_open_artifact(&path, &editors, window, cx);
        })
        .child(
            div()
                .flex_none()
                .w(px(14.))
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .font_weight(FontWeight::NORMAL)
                .text_color(badge_color)
                .child(badge),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .w_full()
                        .truncate()
                        .text_sm()
                        .line_height(CARD_TITLE_LINE)
                        .text_color(foreground)
                        .child(name),
                )
                .when(!dir.is_empty(), |el| {
                    el.child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .text_color(muted)
                            .child(dir),
                    )
                }),
        )
}

//! Read-only commit transcript for the terminal dock's Git Log tab.
//!
//! One row per line rather than a single text node: the graph gutter is made
//! of spaces, and a shared text run would let them collapse and scramble the
//! history alignment.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, CoderTerminalKind};

/// Whether the active terminal tab is the Git Log transcript.
pub(crate) fn git_log_open(state: &BridgeState) -> bool {
    state
        .coder
        .active_terminal()
        .is_some_and(|t| t.kind == CoderTerminalKind::GitLog)
}

/// Toolbar button that opens (or focuses) the Git Log tab.
pub(crate) fn git_log_tab_button() -> Button {
    Button::new("coder-term-git-log")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconSm)
        .leading_icon(PhosphorIcon::GitCommit)
        .tooltip("Git log (read-only)")
        .on_click(|_, _, cx| {
            let _ = Bridge::coder_terminal_open_git_log(cx);
        })
}

/// Reload action shown while the Git Log tab is active.
pub(crate) fn git_log_refresh_button() -> Button {
    Button::new("coder-term-git-log-refresh")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconSm)
        .leading_icon(PhosphorIcon::ArrowsClockwise)
        .tooltip("Reload git log")
        .on_click(|_, _, cx| {
            let _ = Bridge::git_log_refresh(cx);
        })
}

pub(crate) fn render_git_log_pane(
    state: &BridgeState,
    fg: Hsla,
    font_size: u16,
    mono: SharedString,
) -> impl IntoElement {
    let font_size = font_size.max(8) as f32;
    let text = state
        .git_log
        .as_deref()
        .filter(|text| !text.is_empty())
        .unwrap_or("Loading git log…");
    let mut lines = div().w_full().flex().flex_col();
    for line in text.lines() {
        // A space keeps blank lines from collapsing to zero height.
        let line = if line.is_empty() { " " } else { line };
        lines = lines.child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .child(line.to_string()),
        );
    }
    div()
        .id("sc-log")
        .size_full()
        .min_h_0()
        .overflow_scroll()
        .px_3()
        .py_2()
        .font_family(mono)
        .text_size(px(font_size))
        .line_height(px(font_size * 1.5))
        .text_color(fg.opacity(0.92))
        .child(lines)
}

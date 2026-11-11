//! Trace blocks — re-exported and adapted from [`celestia_ui::components`].
//!
//! Replaces the hand-rolled traces with Celestia UI's composite [`Reasoning`]
//! and [`Tool`] components.

use gpui::*;

pub(super) use celestia_ui::components::{Reasoning, Tool, TraceState};

/// A reasoning part: muted header (shimmers while streaming), collapsed body
/// by default. Click the header to reveal the thought text.
#[allow(dead_code)]
pub(super) fn reasoning(
    id: SharedString,
    body: String,
    streaming: bool,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    _cx: &App,
) -> AnyElement {
    Reasoning::new(id)
        .body(body)
        .streaming(streaming)
        .open(expanded)
        .on_toggle(on_toggle)
        .into_any_element()
}

/// A tool call: one flat row — status glyph, label, caret — with the call's
/// detail (command, diff, output) under it once expanded.
#[allow(dead_code)]
pub(super) fn tool(
    id: SharedString,
    label: String,
    detail: Option<String>,
    state: TraceState,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    _cx: &App,
) -> AnyElement {
    let mut t = Tool::new(id, label)
        .state(state)
        .open(expanded)
        .on_toggle(on_toggle);
    if let Some(detail) = detail {
        t = t.detail(detail);
    }
    t.into_any_element()
}

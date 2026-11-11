//! Shared page chrome — consistent spacing, status dots, and empty states.

use celestia_ui::components::composite::empty::{Empty, EmptyDescription, EmptyHeader, EmptyTitle};
use celestia_ui::components::composite::tab_bar::TabBar;
use celestia_ui::theme::{ActiveTheme as _, Theme};
use gpui::*;
use halo_debugger::{DebugRegionMeta, region_bounds_sensor};

/// Vertical rhythm shared by every tool page (intra-group 8 → inter-section 16).
pub const PAGE_GAP: f32 = 16.;
pub const SECTION_GAP: f32 = 12.;

/// Height of a content-tab chip. The component default is 30px; tool panes
/// trade that 6px for content, since these tabs are pure in-pane navigation.
pub const CONTENT_TAB_CHIP_HEIGHT: Pixels = px(24.);
/// Inset above the chips (the component default is 4px).
pub const CONTENT_TAB_INSET: Pixels = px(2.);

/// Content tab strip — a [`TabBar`] with compact chips.
///
/// Use for navigation *inside* a pane (Raw / Headers / Body / Params / Cookies,
/// Request / Attack / Payloads, …). Document tabs keep the component default so
/// they stay aligned with the window's title strip.
pub fn content_tabs(id: impl Into<SharedString>) -> TabBar {
    TabBar::new(id)
        .tab_height(CONTENT_TAB_CHIP_HEIGHT)
        .pt(CONTENT_TAB_INSET)
}

fn panel_region_meta() -> DebugRegionMeta {
    DebugRegionMeta {
        padding: Some("px_1 py_1".into()),
        gap: Some("gap_2".into()),
        notes: Some("chrome::panel".into()),
    }
}

/// Register a named panel region for the in-app debugger Elements tab.
pub fn debug_panel(id: &str, title: &str) -> impl IntoElement {
    region_bounds_sensor(id.to_string(), title.to_string(), panel_region_meta())
}

/// Concentric corner radius: `outer ≈ inner + padding` when surfaces nest closely.
///
/// Ladder (base `theme.radius` = md, typically 8px):
/// - **md** [`Theme::radius`]: panels, cards, standalone controls
/// - **sm** [`Theme::radius_sm`]: children of md wrappers, chips, compact rows
/// - **xs** [`radius_xs`]: tightly nested inside sm (tab icons, micro-pills)
pub fn radius_xs(theme: &Theme) -> Pixels {
    theme.radius_xs()
}

fn panel_header(
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    muted: Hsla,
) -> impl IntoElement {
    let mut header = div().flex().flex_col().gap_0p5().flex_none().child(
        div()
            .text_sm()
            .font_weight(FontWeight::NORMAL)
            .child(title.into()),
    );
    if let Some(desc) = description {
        header = header.child(div().text_xs().text_color(muted).child(desc));
    }
    header
}

/// Flat panel for resizable layouts — title + optional description, no card chrome.
pub fn panel(
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    child: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let title = title.into();
    let region_id = format!("panel-{}", title);
    let muted = cx.theme().muted_foreground;
    div()
        .relative()
        .size_full()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_2()
        .px_1()
        .py_1()
        .overflow_hidden()
        .child(panel_header(title.clone(), description, muted))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(child),
        )
        .child(debug_panel(&region_id, title.as_ref()))
}

/// Flat panel without a visible title — for split panes whose content speaks
/// for itself. Keeps [`panel`]'s insets and debugger region so spacing and
/// element inspection stay consistent.
pub fn bare_panel(region_id: &str, child: impl IntoElement, cx: &App) -> Div {
    flush_panel(region_id, child, cx).px_1().py_1()
}

/// [`bare_panel`] with no inset — the child spans the pane edge to edge.
///
/// Use when the first child draws its own full-bleed rule (a tab strip): an
/// 8px inset leaves that rule floating short of both edges while the section
/// bar directly above it runs the full width.
pub fn flush_panel(region_id: &str, child: impl IntoElement, _cx: &App) -> Div {
    div()
        .relative()
        .size_full()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(child),
        )
        .child(debug_panel(region_id, region_id))
}

/// Stacked settings section — same header as [`panel`], without filling the viewport.
pub fn section(
    title: impl Into<SharedString>,
    description: Option<SharedString>,
    child: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .px_1()
        .py_1()
        .child(panel_header(title, description, muted))
        .child(child)
}

/// Rounded, bordered frame for a [`CodeEditor`] running unbordered
/// (`set_bordered(false)`) inside a pane that owns the chrome.
///
/// Without this the editor renders both its card border and the inner input
/// border, which read as one doubled line.
pub fn code_surface(child: impl IntoElement, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .size_full()
        .min_h_0()
        .min_w_0()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(child)
}

/// Status indicator that never relies on color alone — always pairs a dot with a label.
pub fn status_pill(label: impl Into<SharedString>, tone: StatusTone, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (dot, text) = match tone {
        StatusTone::Idle => (theme.muted_foreground, theme.muted_foreground),
        StatusTone::Busy => (theme.warning, theme.foreground),
        StatusTone::Active => (theme.success, theme.foreground),
    };
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_0p5()
        .rounded(theme.radius_sm())
        .text_xs()
        .text_color(text)
        .child(div().size(px(7.)).rounded_full().bg(dot))
        .child(label.into())
}

#[derive(Clone, Copy)]
pub enum StatusTone {
    Idle,
    Busy,
    Active,
}

/// Centered empty-state via `composite::empty`: what this is + how to fill it.
pub fn empty_state(
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    _cx: &App,
) -> impl IntoElement {
    Empty::new().header(
        EmptyHeader::new()
            .title(EmptyTitle::new().child(title.into()))
            .description(EmptyDescription::new().child(hint.into())),
    )
}

/// Human-readable byte size (`B`, `KB`, `MB`, `GB`, `TB`).
pub(crate) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

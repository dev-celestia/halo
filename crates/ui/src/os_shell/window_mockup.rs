//! Window mockup — the macOS-style chrome drawn for design review.
//!
//! `HALO_MOCKUP=window cargo run` boots the app straight into the mockup: a
//! desktop backdrop with a mock window on it, the way the reference mockup was
//! drawn. The lights are ours (celestia's [`TrafficLights`]), not AppKit's, so
//! the mockup reads the same on every platform and the three buttons can be
//! wired to real window actions.
//!
//! The mockup replaces the app's own chrome. The host OS window keeps its
//! AppKit traffic lights, but mockup mode pushes them out of the window frame
//! (see `windows::halo_window_options`) so the review shows the single
//! cluster drawn here — the same picture as the reference screenshot.

use std::cell::Cell;

use celestia_ui::components::composite::traffic_lights::{TRAFFIC_LIGHT_DIAMETER, TrafficLights};
use celestia_ui::components::composite::window::{
    TRAFFIC_INSET, WINDOW_RADIUS, WINDOW_TITLE_BAR_HEIGHT,
};
use celestia_ui::components::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::app_icons;
use crate::nav::Page;
use crate::windows::TRAFFIC_LIGHT_Y;

/// The grip's square box in the band; the nine-dot icon centers inside it.
const GRIP_BOX: Pixels = px(24.);

/// Which mockup to boot into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mockup {
    /// The macOS-style window: traffic lights, drag grip, title band.
    Window,
}

impl Mockup {
    /// Read the mockup selection from the environment.
    ///
    /// `HALO_MOCKUP=window` picks the window mockup; anything else (or an
    /// unset variable) boots the normal app.
    pub(crate) fn from_env() -> Option<Self> {
        match std::env::var("HALO_MOCKUP")
            .ok()?
            .trim()
            .to_lowercase()
            .as_str()
        {
            "window" | "1" => Some(Self::Window),
            _ => None,
        }
    }
}

/// Render the selected mockup.
pub(crate) fn render_mockup(mockup: Mockup, window: &mut Window, cx: &mut App) -> AnyElement {
    match mockup {
        Mockup::Window => render_window_mockup(window, cx),
    }
}

/// The whole mockup surface: a desktop backdrop with the mock window on it.
fn render_window_mockup(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let is_dark = theme.is_dark();
    let background = theme.background;
    let scale = window.scale_factor();

    div()
        .id("mockup-surface")
        .size_full()
        .relative()
        .bg(background)
        // The real desktop wallpaper, so the mockup sits on the same backdrop
        // the app uses for the desktop page.
        .child(
            img(app_icons::wallpaper(is_dark))
                .id("mockup-wallpaper")
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .child(
            div()
                .id("mockup-stage")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(mock_window(window, scale, cx)),
        )
        .into_any_element()
}

/// The mock window: frame, title band, and a body that sketches the shell.
fn mock_window(window: &mut Window, scale: f32, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let border = theme.window_border;
    let background = theme.background;

    div()
        .id("mockup-window")
        .w(px(880.))
        .h(px(560.))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(WINDOW_RADIUS)
        .border_1()
        .border_color(border)
        .bg(background)
        .shadow(vec![BoxShadow {
            offset: point(px(0.), px(12.)),
            blur_radius: px(32.),
            spread_radius: px(0.),
            inset: false,
            color: rgba(0x00000022).into(),
        }])
        .child(mock_title_band(window, cx))
        .child(mock_tab_strip(scale, cx))
        .child(mock_body(cx))
        .into_any_element()
}

/// The title band: traffic lights, drag grip, centered title.
///
/// The band keeps celestia's metrics (36px tall, a 1px bottom border in the
/// default border colour, 20px of leading inset) so the mockup matches the
/// window the
/// app actually opens. The light cluster sits at Halo's own inset
/// (`windows::TRAFFIC_LIGHT_Y`), a touch above the band's optical centre, and
/// the grip's center stays on the lights' center. The grip is a real drag
/// handle: pressing it and moving the mouse moves the host window.
fn mock_title_band(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let foreground = theme.foreground;
    let muted_foreground = theme.muted_foreground;
    let border = theme.border;

    // Band is center-aligned; the cluster hangs from its own inset instead so
    // it reads a touch high, the same picture the real window's AppKit lights
    // get (`windows::TRAFFIC_LIGHT_Y`). Grip box top puts its center on the
    // lights' center.
    let light_top = TRAFFIC_LIGHT_Y;
    let grip_top = TRAFFIC_LIGHT_Y + TRAFFIC_LIGHT_DIAMETER / 2. - GRIP_BOX / 2.;

    let dragging = window.use_state(cx, |_, _| Cell::new(false));

    div()
        .id("mockup-title-band")
        .w_full()
        .h(WINDOW_TITLE_BAR_HEIGHT)
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(border)
        .pl(TRAFFIC_INSET)
        .pr(px(10.))
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down_out(window.listener_for(&dragging, |dragging, _, _, _| dragging.set(false)))
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(&dragging, |dragging, _, _, _| dragging.set(true)),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(&dragging, |dragging, _, _, _| dragging.set(false)),
        )
        .on_mouse_move(window.listener_for(&dragging, |dragging, _, window, _| {
            if dragging.replace(false) {
                window.start_window_move();
            }
        }))
        .child(
            div()
                .flex_none()
                .h_full()
                .flex()
                .items_start()
                .pt(light_top)
                .child(
                    TrafficLights::new("mockup")
                        .active(true)
                        .on_close(|window, _| window.remove_window())
                        .on_minimize(|window, _| window.minimize_window())
                        .on_zoom(|window, _| window.zoom_window()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(foreground)
                        .child("Halo — HTTP History"),
                ),
        )
        .child(
            div()
                .flex_none()
                .h_full()
                .flex()
                .items_start()
                .pt(grip_top)
                .child(
                    // Nine-dot grip — the cue that the band drags the window.
                    div()
                        .id("mockup-title-grip")
                        .flex_none()
                        .size(GRIP_BOX)
                        .flex()
                        .items_center()
                        .justify_center()
                        .aria_label("Drag to move window")
                        .child(
                            Phosphor::new(PhosphorIcon::DotsNine)
                                .size(px(14.))
                                .color(muted_foreground),
                        ),
                ),
        )
        .into_any_element()
}

/// The work-tab strip under the title band.
fn mock_tab_strip(scale: f32, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let background = theme.background;
    let border = theme.border;
    let primary = theme.primary;
    let foreground = theme.foreground;
    let muted_foreground = theme.muted_foreground;
    let radius = theme.radius_sm();

    let tabs: [(Page, &str, bool); 3] = [
        (Page::HttpHistory, "HTTP", true),
        (Page::Intercept, "Intercept", false),
        (Page::Coder, "Coder", false),
    ];

    div()
        .id("mockup-tab-strip")
        .w_full()
        .h(px(34.))
        .flex_shrink_0()
        .flex()
        .items_end()
        .gap(px(2.))
        .px(px(8.))
        .border_b_1()
        .border_color(border)
        .children(
            tabs.into_iter()
                .enumerate()
                .map(|(index, (page, label, active))| {
                    let mark = app_icons::icon_at(page, app_icons::device_px(16., scale));
                    div()
                        .id(format!("mockup-tab-{index}"))
                        .h(px(28.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded_t(radius)
                        .when(active, |this| {
                            this.bg(background).border_t_2().border_color(primary)
                        })
                        .child(
                            div()
                                .size(px(16.))
                                .flex_none()
                                .overflow_hidden()
                                .rounded(radius)
                                .child(img(mark).size_full().object_fit(ObjectFit::Cover)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(if active { foreground } else { muted_foreground })
                                .child(label),
                        )
                }),
        )
        .into_any_element()
}

/// The window body: a rail on the left, a work area on the right.
fn mock_body(cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let background = theme.background;
    let border = theme.border;
    let muted = theme.muted.opacity(0.22);
    let muted_button = theme.muted.opacity(0.5);
    let skeleton = theme.skeleton;
    let primary = theme.primary;
    let primary_foreground = theme.primary_foreground;
    let foreground = theme.foreground;
    let muted_foreground = theme.muted_foreground;
    let success = theme.success;
    let radius = theme.radius_sm();

    div()
        .id("mockup-body")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_row()
        .child(
            // Left rail.
            div()
                .id("mockup-rail")
                .w(px(180.))
                .h_full()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(9.))
                .p(px(12.))
                .border_r_1()
                .border_color(border)
                .bg(muted)
                .children((0..6).map(|i| skeleton_row(skeleton, px(120. - (i as f32) * 9.)))),
        )
        .child(
            // Work area: a toolbar row over a list of entries.
            div()
                .id("mockup-work-area")
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .gap(px(10.))
                .p(px(14.))
                .child(
                    div()
                        .id("mockup-toolbar")
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .id("mockup-filter")
                                .w(px(240.))
                                .h(px(26.))
                                .rounded(radius)
                                .border_1()
                                .border_color(border)
                                .bg(background)
                                .flex()
                                .items_center()
                                .px(px(8.))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted_foreground)
                                        .child("Filter requests"),
                                ),
                        )
                        .child(div().flex().items_center().gap(px(6.)).children(
                            [("Clear", false), ("Send", true)].map(|(label, is_primary)| {
                                div()
                                    .id(format!("mockup-button-{label}"))
                                    .h(px(26.))
                                    .px(px(10.))
                                    .rounded(radius)
                                    .flex()
                                    .items_center()
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(if is_primary {
                                        primary_foreground
                                    } else {
                                        foreground
                                    })
                                    .bg(if is_primary { primary } else { muted_button })
                                    .child(label)
                            }),
                        )),
                )
                .child(
                    div()
                        .id("mockup-list")
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .children((0..7).map(|i| {
                            div()
                                .id(format!("mockup-row-{i}"))
                                .h(px(26.))
                                .w_full()
                                .flex_none()
                                .rounded(radius)
                                .border_1()
                                .border_color(border)
                                .bg(background)
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .px(px(8.))
                                .child(div().size(px(10.)).flex_none().rounded_full().bg(
                                    match i % 3 {
                                        0 => primary.opacity(0.75),
                                        1 => success.opacity(0.75),
                                        _ => muted_foreground.opacity(0.5),
                                    },
                                ))
                                .child(skeleton_row(skeleton, px(90. + (i as f32) * 22.)))
                        })),
                ),
        )
        .into_any_element()
}

/// A muted placeholder bar.
fn skeleton_row(skeleton: Hsla, width: Pixels) -> impl IntoElement {
    div()
        .h(px(10.))
        .w(width)
        .flex_none()
        .rounded_full()
        .bg(skeleton)
}

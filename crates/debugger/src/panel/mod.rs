//! Bottom-docked DevTools panel.

mod elements;
mod logs;
mod network;
mod storage;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::BridgeState;

use crate::state::{DebuggerState, DebuggerTab};

fn tab_icon(tab: DebuggerTab) -> PhosphorIcon {
    match tab {
        DebuggerTab::Elements => PhosphorIcon::TreeStructure,
        DebuggerTab::Network => PhosphorIcon::ArrowsLeftRight,
        DebuggerTab::Storage => PhosphorIcon::HardDrives,
        DebuggerTab::Logs => PhosphorIcon::TerminalWindow,
    }
}

/// Soft upward lift so the dock reads above the page (border stays for structure).
fn panel_elevation(is_dark: bool) -> Vec<BoxShadow> {
    if is_dark {
        vec![
            BoxShadow::new(px(0.), px(0.), hsla(0., 0., 1., 0.06))
                .blur_radius(px(0.))
                .spread_radius(px(1.)),
            BoxShadow::new(px(0.), px(-8.), hsla(0., 0., 0., 0.40))
                .blur_radius(px(24.))
                .spread_radius(px(-4.)),
        ]
    } else {
        vec![
            BoxShadow::new(px(0.), px(0.), hsla(0., 0., 0., 0.06))
                .blur_radius(px(0.))
                .spread_radius(px(1.)),
            BoxShadow::new(px(0.), px(-4.), hsla(0., 0., 0., 0.08))
                .blur_radius(px(12.))
                .spread_radius(px(-2.)),
            BoxShadow::new(px(0.), px(-10.), hsla(0., 0., 0., 0.10))
                .blur_radius(px(28.))
                .spread_radius(px(-4.)),
        ]
    }
}

/// Render the docked debugger panel. Height comes from `state.height`.
pub fn render_debugger(
    state: &DebuggerState,
    bridge: &BridgeState,
    on_close: impl Fn(&mut App) + 'static + Clone,
    on_tab: impl Fn(DebuggerTab, &mut App) + 'static + Clone,
    on_inspect_toggle: impl Fn(&mut App) + 'static + Clone,
    on_select_region: impl Fn(String, &mut App) + 'static + Clone,
    on_select_network: impl Fn(Option<String>, &mut App) + 'static + Clone,
    on_select_prefs: impl Fn(Option<String>, &mut App) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let height = state.height;

    div()
        .id("debugger-panel")
        .flex_none()
        .w_full()
        .h(px(height))
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.background)
        .shadow(panel_elevation(theme.is_dark()))
        .overflow_hidden()
        .child(render_tab_bar(
            state,
            on_close,
            on_tab,
            on_inspect_toggle,
            cx,
        ))
        .child(
            div()
                .id("debugger-body")
                .flex_1()
                .w_full()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "debugger-tab-{}",
                            state.tab.id()
                        )))
                        .size_full()
                        .min_h_0()
                        .child(match state.tab {
                            DebuggerTab::Elements => {
                                elements::render(state, bridge, on_select_region, cx)
                                    .into_any_element()
                            }
                            DebuggerTab::Network => {
                                network::render(state, bridge, on_select_network, cx)
                                    .into_any_element()
                            }
                            DebuggerTab::Storage => {
                                storage::render(state, bridge, on_select_prefs, cx)
                                    .into_any_element()
                            }
                            DebuggerTab::Logs => logs::render(state, bridge, cx).into_any_element(),
                        }),
                ),
        )
}

fn render_tab_bar(
    state: &DebuggerState,
    on_close: impl Fn(&mut App) + 'static + Clone,
    on_tab: impl Fn(DebuggerTab, &mut App) + 'static + Clone,
    on_inspect_toggle: impl Fn(&mut App) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();

    let mut tabs = div().flex().items_center().gap_0p5().flex_1().min_w_0();
    for tab in DebuggerTab::all() {
        let active = state.tab == *tab;
        let on_tab = on_tab.clone();
        let t = *tab;
        tabs = tabs.child(
            Button::new(SharedString::from(format!("debugger-tab-{}", tab.id())))
                .variant(if active {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Small)
                .leading_icon(tab_icon(*tab))
                .label(tab.label())
                .on_click(move |_, _, cx| on_tab(t, cx)),
        );
    }

    div()
        .id("debugger-tab-bar")
        .w_full()
        .h(px(36.))
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_1p5()
                .px_1()
                .child(
                    Phosphor::new(PhosphorIcon::Bug)
                        .size(px(14.))
                        .color(theme.primary),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("Debugger"),
                ),
        )
        .child(div().w(px(1.)).h(px(16.)).flex_none().bg(theme.border))
        .child(tabs)
        .when(state.tab == DebuggerTab::Elements, |this| {
            let inspecting = state.inspect_mode;
            let on_inspect_toggle = on_inspect_toggle.clone();
            this.child(
                Button::new("debugger-inspect")
                    .variant(if inspecting {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::CursorClick)
                    .label(if inspecting {
                        "Inspecting…"
                    } else {
                        "Inspect"
                    })
                    .tooltip("Click a registered UI region to select it")
                    .on_click(move |_, _, cx| on_inspect_toggle(cx)),
            )
        })
        .child(kbd_chip("F12", cx))
        .child(
            Button::new("debugger-close")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::X)
                .tooltip("Close debugger")
                .on_click(move |_, _, cx| on_close(cx)),
        )
}

/// Compact keyboard hint — static cue next to the close control.
fn kbd_chip(label: &'static str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex_none()
        .px_1p5()
        .py_0p5()
        .rounded(px(4.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.35))
        .text_xs()
        .font_family("monospace")
        .text_color(theme.muted_foreground)
        .child(label)
}

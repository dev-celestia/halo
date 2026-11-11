//! Desktop wallpaper, app icons, and context menu.

use celestia_ui::components::Tooltip;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState};

use crate::app::HaloApp;
use crate::app_icons;
use crate::components::toggle_proxy;
use crate::nav::Page;
use crate::theme;

use super::helpers::{alpha_chip, open_app};
use super::widgets::{DesktopWidget, render_widgets};

/// App mark size inside a desktop tile.
const ICON_MARK: f32 = 48.;

pub(crate) fn render_desktop(
    state: &BridgeState,
    widget_order: &[DesktopWidget],
    dragging_widget: Option<DesktopWidget>,
    widget_drop_target: Option<DesktopWidget>,
    weak: &WeakEntity<HaloApp>,
    scale: f32,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let menu_weak = weak.clone();
    let proxy_running = state.proxy.running;
    let proxy_busy = state.proxy_busy;
    let is_dark = theme.is_dark();

    // Cap at 4 columns: 4 × 68px tiles + 3 × 4px gaps.
    const ICON_COLS: f32 = 4.;
    const ICON_TILE: f32 = 68.;
    const ICON_GAP: f32 = 4.;
    let icons_w = ICON_COLS * ICON_TILE + (ICON_COLS - 1.) * ICON_GAP;
    // Marks are rasterized 1:1 with the device grid.
    let mark_px = app_icons::device_px(ICON_MARK, scale);

    let mut icons = div()
        .id("desktop-icons")
        .flex_none()
        .w(px(icons_w))
        .flex()
        .flex_row()
        .flex_wrap()
        .items_start()
        .gap(px(ICON_GAP));
    let mut pinned_count = 0usize;
    for page in Page::ALL {
        if !page.ships() {
            continue;
        }
        // Only pinned apps get a desktop tile; the Start menu owns pinning.
        let pinned = page
            .work_kind()
            .is_some_and(|kind| state.pinned_apps.contains(&kind));
        if !pinned {
            continue;
        }
        pinned_count += 1;
        icons = icons.child(desktop_icon_tile(page, mark_px, weak, cx));
    }

    div()
        .id("desktop")
        .relative()
        .size_full()
        .overflow_y_scrollbar()
        .bg(theme.background)
        .context_menu(move |menu, _, _| {
            desktop_context_menu(menu, &menu_weak, proxy_running, proxy_busy, is_dark)
        })
        .child(
            img(app_icons::wallpaper(theme.is_dark()))
                .id("desktop-wallpaper")
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .child(
            div()
                .relative()
                .size_full()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap(px(24.))
                .p(px(20.))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(icons)
                        .when(pinned_count == 0, |this| {
                            // Empty desktop is a valid user choice — say how
                            // to get icons back instead of showing nothing.
                            this.child(
                                div()
                                    .w(px(icons_w))
                                    .px(px(6.))
                                    .py(px(4.))
                                    .rounded(theme.radius_sm())
                                    .bg(theme.background)
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("No apps pinned. Pin apps from the Start menu."),
                            )
                        }),
                )
                .child(render_widgets(
                    state,
                    widget_order,
                    dragging_widget,
                    widget_drop_target,
                    weak,
                    cx,
                )),
        )
}

fn desktop_context_menu(
    menu: PopupMenu,
    weak: &WeakEntity<HaloApp>,
    proxy_running: bool,
    proxy_busy: bool,
    is_dark: bool,
) -> PopupMenu {
    let theme_label = if is_dark {
        "Switch to light theme"
    } else {
        "Switch to dark theme"
    };
    let proxy_label = if proxy_running {
        "Stop proxy"
    } else {
        "Start proxy"
    };

    let open_window = |page: Page| {
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            HaloApp::open_in_new_window(page, cx);
        }
    };
    let open_tab = |page: Page| {
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            crate::windows::open_tab_in_main_shell(page, cx);
        }
    };

    let mut menu = menu
        .item(PopupMenuItem::new("New Window").on_click(open_window(Page::Desktop)))
        .item(PopupMenuItem::new("Refresh traffic").on_click(|_, _, cx| {
            let _ = Bridge::refresh_history(cx);
        }))
        .item(
            PopupMenuItem::new(proxy_label)
                .disabled(proxy_busy)
                .on_click(|_, _, cx| toggle_proxy(cx)),
        )
        .separator();

    // Only shipping apps — alpha tools stay out of the release context menu.
    for page in [
        Page::HttpHistory,
        Page::Intercept,
        Page::Repeater,
        Page::Intruder,
        Page::Crawler,
        Page::Coder,
        Page::SourceControl,
        Page::Agent,
        Page::Doctor,
        Page::Notes,
    ] {
        if page.ships() {
            menu = menu.item(PopupMenuItem::new(page.label()).on_click(open_tab(page)));
        }
    }

    menu.separator()
        .item(PopupMenuItem::new(theme_label).on_click(|_, _, cx| theme::toggle_mode(cx)))
        .item({
            let weak = weak.clone();
            PopupMenuItem::new("Reset widget order").on_click(move |_, _, cx| {
                if let Some(app) = weak.upgrade() {
                    app.update(cx, |this, cx| this.reset_widget_order(cx));
                }
            })
        })
        .item({
            let weak = weak.clone();
            PopupMenuItem::new("Open Start menu").on_click(move |_, _, cx| {
                if let Some(app) = weak.upgrade() {
                    app.update(cx, |this, cx| {
                        if !this.launcher_open {
                            this.toggle_launcher(cx);
                        }
                    });
                }
            })
        })
}

fn desktop_icon_tile(
    page: Page,
    mark_px: u32,
    weak: &WeakEntity<HaloApp>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let open = open_app(weak, page);
    let tip = page.description();
    let id: SharedString = format!("desk-{}", page.id()).into();

    div()
        .id(id.clone())
        .role(Role::Button)
        .aria_label(page.label())
        .tab_index(0)
        .w(px(68.))
        .px(px(4.))
        .py(px(4.))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .rounded(theme.radius)
        .cursor_pointer()
        .overflow_hidden()
        .hover(|s| s.bg(theme.foreground.opacity(0.08)))
        .active(|s| s.opacity(0.85))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .on_click(move |_, _, cx| open(cx))
        .context_menu(move |menu, _, _| {
            menu.item(PopupMenuItem::new("Open").on_click(move |_, _, cx| {
                crate::windows::open_tab_in_main_shell(page, cx);
            }))
            .item(
                PopupMenuItem::new("Open in New Sub Window").on_click(move |_, _, cx| {
                    HaloApp::open_sub_window(page, cx);
                }),
            )
            .item(
                PopupMenuItem::new("Open in New Window").on_click(move |_, _, cx| {
                    HaloApp::open_in_new_window(page, cx);
                }),
            )
            .separator()
            .item(
                PopupMenuItem::new("Unpin from Desktop").on_click(move |_, _, cx| {
                    if let Some(kind) = page.work_kind() {
                        Bridge::toggle_desktop_pin(kind, cx);
                    }
                }),
            )
        })
        .child(
            // Plain img, not ImageIcon: ImageIcon owns an `.id()` hitbox that
            // steals clicks from this clickable tile in GPUI.
            div()
                .size(px(ICON_MARK))
                .flex_none()
                .overflow_hidden()
                .rounded(theme.radius_sm())
                .child(
                    img(app_icons::icon_at(page, mark_px))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
        )
        .child(
            // The label hugs its text so the plate reads as a small pill
            // instead of a full-width band. Solid, not washed: a translucent
            // scrim let the wallpaper bleed through and the text never stood
            // out. Horizontal padding is kept to 1px — the tile is 68px, so
            // wider padding would truncate labels that used to fit. 11px
            // normal is the smallest step in the app — the old 13px semibold
            // ran out of room ("Split View" was 63px of 66). Alpha apps get
            // a compact ALPHA chip under the name; Release has none.
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.))
                .child(
                    div()
                        .max_w_full()
                        .px(px(1.))
                        .py(px(0.5))
                        .rounded(theme.radius_sm())
                        .bg(theme.background)
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_center()
                        .text_color(theme.foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(page.label()),
                )
                .child(alpha_chip(page, cx)),
        )
}

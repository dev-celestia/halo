//! Bottom start bar / system tray.

use std::rc::Rc;
use std::sync::Arc;

use celestia_ui::components::primitive::icon::Phosphor;
use celestia_ui::components::{PhosphorIcon, SystemTray, SystemTrayClock, SystemTrayItem, Tooltip};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};
use halo_bridge::{Bridge, BridgeState};
use halo_debugger::{DebugRegionMeta, region_bounds_sensor};

use crate::app::HaloApp;
use crate::app_icons;
use crate::components::{ProxyActionState, proxy_dock_button};
use crate::nav::Page;
use crate::theme;

use super::helpers::{badge_for, is_running, open_app, proxy_status};

const DOCK_H: f32 = 40.;
const DOCK_MARK: f32 = 20.;

pub(crate) fn render_dock(
    page: Page,
    launcher_open: bool,
    notifications_open: bool,
    notification_unread: usize,
    debugger_open: bool,
    clock_time: SharedString,
    clock_date: SharedString,
    state: &BridgeState,
    weak: &WeakEntity<HaloApp>,
    scale: f32,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let weak_start = weak.clone();
    let weak_notify = weak.clone();
    let proxy = ProxyActionState::from_bridge(state);
    // Marks are rasterized 1:1 with the device grid — 20px on a 1x display,
    // 40 device px on a 2x one.
    let mark_px = app_icons::device_px(DOCK_MARK, scale);

    // One icon per open session tab, in title-bar order. Click selects that
    // tab; it does not open a new one. Launching stays on Start, the desktop,
    // and the title-bar plus menu. The row grows into leftover space and clips
    // so a long list cannot push the tray off the bar.
    let mut apps = div()
        .id("dock-open-tabs")
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .flex_row()
        .items_center()
        .gap(px(2.));
    for tab in state.session_work_tabs() {
        let tool = Page::from_work_kind(tab.kind);
        let tab_id = tab.id.clone();
        let close_id = tab.id.clone();
        let weak_tab = weak.clone();
        let weak_close = weak.clone();
        let title: SharedString = tab.title.clone().into();
        let active =
            page != Page::Desktop && state.active_work_tab_id.as_deref() == Some(tab.id.as_str());
        apps = apps.child(dock_button(
            DockIcon {
                id: format!("dock-tab-{}", tab.id).into(),
                image: app_icons::icon_at(tool, mark_px),
                name: title.clone(),
                tooltip: title,
                active,
                running: is_running(tool, state),
                badge: badge_for(tool, state),
            },
            move |cx| {
                let _ = Bridge::select_work_tab(&tab_id, cx);
                if let Some(app) = weak_tab.upgrade() {
                    app.update(cx, |this, cx| this.follow_active_work(cx));
                }
            },
            Some(tool),
            Some(Rc::new(move |cx| {
                let _ = Bridge::close_work_tab(&close_id, cx);
                if let Some(app) = weak_close.upgrade() {
                    app.update(cx, |this, cx| this.follow_active_work(cx));
                }
            })),
            cx,
        ));
    }

    let (proxy_label, _) = proxy_status(state);

    let tray_sep = || div().w(px(1.)).h(px(18.)).flex_none().bg(theme.border);

    div()
        .id("dock")
        .relative()
        .role(Role::Navigation)
        .aria_label("Apps")
        .w_full()
        .h(px(DOCK_H))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(2.))
        .px(px(4.))
        .bg(theme.status_bar)
        .border_t_1()
        .border_color(theme.status_bar_border)
        // Start sits at the far left of the dock; the proxy status button
        // sits immediately to its right.
        .child(dock_button(
            DockIcon {
                id: "dock-start".into(),
                image: app_icons::brand_at(mark_px),
                name: "Start".into(),
                tooltip: "Start".into(),
                active: launcher_open,
                running: false,
                badge: None,
            },
            move |cx| {
                if let Some(app) = weak_start.upgrade() {
                    app.update(cx, |this, cx| this.toggle_launcher(cx));
                }
            },
            None,
            None,
            cx,
        ))
        .child(
            // Full-height slot so the button cannot stretch with the bar.
            // In a centered row, `my` is absorbed into alignment and reads as
            // no margin — pad the slot instead to keep clear space around it.
            div()
                .flex_none()
                .h(px(DOCK_H))
                .mx(px(4.))
                // XSmall is 20px; 10px inset top/bottom is the full free space
                // in the 40px dock (margin around the control, not inside it).
                .py(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .child(proxy_dock_button(proxy, proxy_label)),
        )
        .child(tray_sep())
        .child(apps)
        .child({
            // Flat against the dock — no pill background.
            // DevTools bug icon is debug-only; release / `.app` bundles
            // omit it so shipping builds do not expose the inspector.
            let tray = SystemTray::new("tray")
                .bg(theme.transparent)
                .rounded(px(0.))
                .border_0()
                .px(px(0.));
            #[cfg(debug_assertions)]
            let tray = {
                let weak_dbg = weak.clone();
                tray.item(
                    SystemTrayItem::new("debugger", PhosphorIcon::Bug)
                        .active(debugger_open)
                        .on_click(move |_, window, cx| {
                            if let Some(app) = weak_dbg.upgrade() {
                                app.update(cx, |this, cx| {
                                    this.toggle_debugger(window, cx);
                                });
                            }
                        }),
                )
            };
            #[cfg(not(debug_assertions))]
            let _ = debugger_open;
            tray.item(
                SystemTrayItem::new(
                    "theme",
                    if theme.is_dark() {
                        PhosphorIcon::Sun
                    } else {
                        PhosphorIcon::Moon
                    },
                )
                .on_click(|_, _, cx| theme::toggle_mode(cx)),
            )
        })
        // Bell / Session sit immediately left of the clock.
        .child(tray_sep())
        .child(notification_button(
            notifications_open,
            notification_unread,
            move |cx| {
                if let Some(app) = weak_notify.upgrade() {
                    app.update(cx, |this, cx| this.toggle_notifications(cx));
                }
            },
            cx,
        ))
        .child(tray_sep())
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .child(super::work_tabs::session_button(state, weak)),
        )
        .child(tray_sep())
        .child({
            let open_settings = open_app(weak, Page::Settings);
            SystemTray::new("dock-settings-tray")
                .bg(theme.transparent)
                .rounded(px(0.))
                .border_0()
                .px(px(0.))
                .item(
                    SystemTrayItem::new("settings", PhosphorIcon::Gear)
                        .active(page == Page::Settings)
                        .on_click(move |_, _, cx| open_settings(cx)),
                )
        })
        .child(tray_sep())
        .child(SystemTrayClock::new("dock-clock", clock_time).date(clock_date))
        .child(region_bounds_sensor(
            "dock",
            "Dock",
            DebugRegionMeta {
                padding: Some("dock chrome".into()),
                gap: None,
                notes: Some(format!(
                    "proxy={} tabs={}",
                    if state.proxy.running { "on" } else { "off" },
                    state.work_tabs.len()
                )),
            },
        ))
}

struct DockIcon {
    id: SharedString,
    image: Arc<Image>,
    name: SharedString,
    tooltip: SharedString,
    active: bool,
    running: bool,
    badge: Option<String>,
}

fn notification_button(
    open: bool,
    unread: usize,
    on_click: impl Fn(&mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let tip: SharedString = if unread > 0 {
        format!("{unread} new event(s) — open notification log").into()
    } else {
        "Notification log — all app events".into()
    };
    let badge = if unread > 99 {
        Some("99+".to_string())
    } else if unread > 0 {
        Some(unread.to_string())
    } else {
        None
    };

    div()
        .id("dock-notifications")
        .role(Role::Button)
        .aria_label("Notifications")
        .tab_index(0)
        .w(px(28.))
        .h(px(30.))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.))
        .rounded(theme.radius)
        .cursor_pointer()
        .when(open, |this| this.bg(theme.accent.opacity(0.16)))
        .when(!open, |this| this.hover(|s| s.bg(theme.muted)))
        .active(|s| s.opacity(0.85))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(move |_, _, cx| on_click(cx))
        .child(
            div()
                .relative()
                .size(px(DOCK_MARK))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Phosphor::new(if unread > 0 {
                        PhosphorIcon::BellRinging
                    } else {
                        PhosphorIcon::Bell
                    })
                    .size(px(14.))
                    .color(if open {
                        theme.accent
                    } else {
                        theme.muted_foreground
                    }),
                )
                .when_some(badge, |this, badge| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(-2.))
                            .right(px(-4.))
                            .min_w(px(12.))
                            .h(px(12.))
                            .px(px(2.))
                            .rounded_full()
                            .bg(theme.danger)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.danger_foreground)
                            .child(badge),
                    )
                }),
        )
        .child(
            div()
                .h(px(2.))
                .w(if open { px(10.) } else { px(2.) })
                .rounded_full()
                .when(open, |this| this.bg(theme.accent))
                .when(!open, |this| this.bg(theme.transparent)),
        )
}

fn dock_button(
    icon: DockIcon,
    on_click: impl Fn(&mut App) + 'static,
    sub_window_page: Option<Page>,
    on_close: Option<Rc<dyn Fn(&mut App)>>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let tip = icon.tooltip.clone();
    let show_pip = icon.active || icon.running;
    let pip_w = if icon.active {
        px(10.)
    } else if icon.running {
        px(4.)
    } else {
        px(2.)
    };
    let pip = if icon.active {
        theme.accent
    } else if icon.running {
        theme.muted_foreground
    } else {
        theme.transparent
    };
    let hover_group: SharedString = format!("{}-hover", icon.id).into();
    let hover_ring = theme.foreground.opacity(0.45);

    // Avoid nesting ImageIcon (which owns its own `.id()` hitbox) inside this
    // button — that steals clicks from the parent in GPUI.
    // Mark sits inside a 1px ring: accent while this tab is in front, a
    // lighter ring while the pointer is over the button.
    let mark = div()
        .size(px(DOCK_MARK))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded(theme.radius_sm())
        .bg(theme.muted)
        .child(
            img(icon.image.clone())
                .size_full()
                .object_fit(ObjectFit::Cover),
        );
    let mut icon_ring = div()
        .size(px(DOCK_MARK + 4.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radius)
        .border_1()
        .border_color(if icon.active {
            theme.accent
        } else {
            theme.transparent
        })
        .child(mark);
    if !icon.active {
        let group = hover_group.clone();
        icon_ring = icon_ring.group_hover(group, move |style| style.border_color(hover_ring));
    }

    div()
        .id(icon.id.clone())
        .group(hover_group)
        .role(Role::Button)
        .aria_label(icon.name.clone())
        .tab_index(0)
        .w(px(28.))
        .h(px(30.))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.))
        .rounded(theme.radius)
        .cursor_pointer()
        .when(icon.active, |this| this.bg(theme.accent.opacity(0.16)))
        .when(!icon.active, |this| this.hover(|s| s.bg(theme.muted)))
        .active(|s| s.opacity(0.85))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(move |_, _, cx| on_click(cx))
        .child(
            div()
                .relative()
                .child(icon_ring)
                .when_some(icon.badge, |this, badge| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(-2.))
                            .right(px(-4.))
                            .min_w(px(12.))
                            .h(px(12.))
                            .px(px(2.))
                            .rounded_full()
                            .bg(theme.danger)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.danger_foreground)
                            .child(badge),
                    )
                }),
        )
        .child(
            div()
                .h(px(2.))
                .w(pip_w)
                .rounded_full()
                .when(show_pip, |this| this.bg(pip))
                .when(!show_pip, |this| this.bg(theme.transparent)),
        )
        .context_menu(move |mut menu, _, _| {
            if let Some(close) = on_close.clone() {
                menu = menu.item(PopupMenuItem::new("Close").on_click(move |_, _, cx| close(cx)));
            }
            if let Some(page) = sub_window_page {
                if on_close.is_some() {
                    menu = menu.separator();
                }
                menu = menu.item(PopupMenuItem::new("Open in New Sub Window").on_click(
                    move |_, _, cx| {
                        HaloApp::open_sub_window(page, cx);
                    },
                ));
            }
            menu
        })
}

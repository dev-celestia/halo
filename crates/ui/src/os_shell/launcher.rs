//! Start menu overlay.

use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon, PhosphorWeight};
use celestia_ui::components::primitive::input::InputState;
use celestia_ui::components::{Input, SearchField, StartMenu, Tooltip};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};
use halo_bridge::{Bridge, BridgeState};

use crate::app::HaloApp;
use crate::app_icons;
use crate::nav::Page;

use super::helpers::{alpha_chip, open_app};

/// App mark size in the start-menu rows.
const ROW_MARK: f32 = 20.;
/// Hitbox of the trailing desktop-pin toggle on each row.
const PIN_HIT: f32 = 24.;

pub(crate) fn render_launcher(
    state: &BridgeState,
    search: &Entity<InputState>,
    weak: &WeakEntity<HaloApp>,
    scale: f32,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let weak_scrim = weak.clone();
    let query = search.read(cx).value().to_ascii_lowercase();
    let query = query.trim().to_string();
    // Definite half-window box so the flyout cannot grow with the app list.
    let menu_max_h = (window.viewport_size().height * 0.5).max(px(240.));

    let search_input = search.clone();
    let clear_search = search.clone();
    let weak_clear = weak.clone();
    let mut menu = StartMenu::new("start-menu")
        .shadow_md()
        .h_full()
        .scrollable(true)
        .search(
            SearchField::new("launcher-search")
                .input(Input::new(&search_input).appearance(false).cleanable(false))
                .on_clear(move |_, window, cx| {
                    clear_search.update(cx, |s, cx| {
                        s.set_value("", window, cx);
                    });
                    // `set_value` does not emit InputEvent::Change.
                    if let Some(app) = weak_clear.upgrade() {
                        app.update(cx, |_, cx| cx.notify());
                    }
                }),
        )
        .pins_label("Apps")
        .pins_list(true);

    let mut matched = 0usize;
    for page in Page::ALL {
        if !page.ships() {
            continue;
        }
        if !query.is_empty()
            && !page.label().to_ascii_lowercase().contains(&query)
            && !page.description().to_ascii_lowercase().contains(&query)
        {
            continue;
        }
        matched += 1;
        let pinned = page
            .work_kind()
            .is_some_and(|kind| state.pinned_apps.contains(&kind));
        menu = menu.pin(app_row("pin", page, pinned, weak, scale, cx));
    }

    if matched == 0 {
        menu = menu.pin(
            div()
                .w_full()
                .px(px(6.))
                .py(px(6.))
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("No apps match your search."),
        );
    }

    div()
        .id("launcher-overlay")
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        // Keep wheel/trackpad gestures from scrolling the workspace under the
        // flyout (same approach as context menus).
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("launcher-scrim")
                .size_full()
                .bg(theme.foreground.opacity(0.12))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    if let Some(app) = weak_scrim.upgrade() {
                        app.update(cx, |this, cx| this.close_launcher(cx));
                    }
                }),
        )
        .child(
            div()
                .id("launcher-menu")
                .absolute()
                .bottom(px(4.))
                .left(px(4.))
                .w(px(360.))
                .h(menu_max_h)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(menu),
        )
}

fn app_row(
    prefix: &str,
    page: Page,
    pinned: bool,
    weak: &WeakEntity<HaloApp>,
    scale: f32,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let open = open_app(weak, page);
    let tip = page.description();
    let id: SharedString = format!("{prefix}-{}", page.id()).into();
    let kind = page.work_kind();

    let pin_label: SharedString = if pinned {
        "Unpin from Desktop".into()
    } else {
        "Pin to Desktop".into()
    };
    let pin_tip = pin_label.clone();
    let pin_color = if pinned {
        theme.foreground
    } else {
        theme.muted_foreground
    };
    let pin_weight = if pinned {
        PhosphorWeight::Fill
    } else {
        PhosphorWeight::Regular
    };

    div()
        .id(id.clone())
        .role(Role::Button)
        .aria_label(page.label())
        .tab_index(0)
        .w_full()
        .h(px(28.))
        .px(px(4.))
        .flex_none()
        .flex()
        .flex_row()
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
            menu.item(
                PopupMenuItem::new(pin_label.clone()).on_click(move |_, _, cx| {
                    if let Some(kind) = kind {
                        Bridge::toggle_desktop_pin(kind, cx);
                    }
                }),
            )
            .item(
                PopupMenuItem::new("Open in New Sub Window").on_click(move |_, _, cx| {
                    HaloApp::open_sub_window(page, cx);
                }),
            )
        })
        .child(
            // Plain img + label (not ImageIcon): ImageIcon's own `.id()` hitbox
            // steals clicks from this clickable row in GPUI.
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .size(px(ROW_MARK))
                        .flex_none()
                        .overflow_hidden()
                        .rounded(theme.radius_sm())
                        .child(
                            img(app_icons::icon_at(
                                page,
                                app_icons::device_px(ROW_MARK, scale),
                            ))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                        ),
                )
                .child(div().text_xs().child(page.label()))
                .child(alpha_chip(page, cx)),
        )
        .child(div().flex_1())
        .child(
            // Trailing pin toggle: filled mark = on the desktop, outline =
            // not. Stops propagation so the row's open-app click never fires.
            div()
                .id(SharedString::from(format!("{prefix}-desk-{}", page.id())))
                .role(Role::Button)
                .aria_label(pin_tip.clone())
                .size(px(PIN_HIT))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radius_sm())
                .cursor_pointer()
                .hover(|s| s.bg(theme.foreground.opacity(0.10)))
                .active(|s| s.opacity(0.85))
                .tooltip(move |window, cx| Tooltip::new(pin_tip.clone()).build(window, cx))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if let Some(kind) = kind {
                        Bridge::toggle_desktop_pin(kind, cx);
                    }
                })
                .child(
                    Phosphor::new(PhosphorIcon::PushPin)
                        .weight(pin_weight)
                        .size(px(14.))
                        .color(pin_color),
                ),
        )
}

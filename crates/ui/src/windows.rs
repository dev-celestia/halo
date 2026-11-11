//! Open OS windows via `celestia_ui::components::composite::window`.

use celestia_ui::components::composite::window::{TRAFFIC_INSET, open_window_options};
use gpui::*;
use gpui_component::Root;
use halo_bridge::{Bridge, SettingsTab};

use crate::app::HaloApp;
use crate::icon;
use crate::nav::Page;

const APP_ID: &str = "com.halo.app";

/// Traffic-light inset from the top of the 36px title band.
///
/// Celestia centers the 12px lights in the band ((36 - 12) / 2 = 12px); the
/// review reads that a touch low, so Halo sits the cluster 2px higher. The
/// window mockup draws its own cluster at the same inset
/// (`os_shell::window_mockup`) so review and the real window stay one picture.
pub(crate) const TRAFFIC_LIGHT_Y: Pixels = px(10.);

/// Halo window options: Celestia native chrome plus app icon / id / size.
fn halo_window_options(
    cx: &mut App,
    bounds: Bounds<Pixels>,
    min_size: Size<Pixels>,
) -> WindowOptions {
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(min_size),
        icon: icon::window_icon(),
        app_id: Some(APP_ID.into()),
        ..open_window_options(cx)
    };

    // AppKit's lights sit at Halo's inset, not celestia's centered default.
    // Mockup mode draws its own traffic lights (celestia's `TrafficLights`),
    // so AppKit's real buttons are pushed out of the window frame — otherwise
    // the review shows two clusters, one of them AppKit's.
    let light_x = if crate::os_shell::Mockup::from_env().is_some() {
        px(-100.)
    } else {
        TRAFFIC_INSET
    };
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.traffic_light_position = Some(point(light_x, TRAFFIC_LIGHT_Y));
    }

    options
}

/// Open `page` as a work tab of the main desktop shell. Used from popup
/// menus that only carry `&mut App`; returns false when no main shell exists.
pub fn open_tab_in_main_shell(page: Page, cx: &mut App) -> bool {
    for handle in cx.windows() {
        let Some(window) = handle.downcast::<Root>() else {
            continue;
        };
        let Some(app) = window
            .read(cx)
            .ok()
            .and_then(|root| root.view().clone().downcast::<HaloApp>().ok())
        else {
            continue;
        };
        if app.read(cx).sub_window_page().is_some() {
            continue; // satellite window, not the main shell
        }
        app.update(cx, |this, cx| this.open_page(page, cx));
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        return true;
    }
    false
}

/// Focus the satellite window already showing `page`, or open one.
///
/// A satellite is a sub window: the tool by itself, beside the desktop shell.
/// Used from right-click → “Open in New Sub Window”. Left-click / dock / start
/// menu open work tabs on the main shell instead. Repeating the open brings
/// that window forward instead of stacking copies.
pub fn open_sub_window(page: Page, cx: &mut App) {
    if focus_sub_window(page, cx) {
        return;
    }
    open_page_window(page, cx);
}

fn focus_sub_window(page: Page, cx: &mut App) -> bool {
    for handle in cx.windows() {
        let Some(window) = handle.downcast::<Root>() else {
            continue;
        };
        let showing = window
            .read(cx)
            .ok()
            .and_then(|root| root.view().clone().downcast::<HaloApp>().ok());
        let is_match = showing.and_then(|app| app.read(cx).sub_window_page()) == Some(page);
        if is_match {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return true;
        }
    }
    false
}

/// Open a new halo window focused on `page`.
///
/// Desktop windows are full shells (work tabs + dock). Tool / satellite windows
/// are standalone: no tab strip, no dock, and they do not join or steal the
/// main session's work tabs. The Bridge store is still shared across windows.
pub fn open_page_window(page: Page, cx: &mut App) {
    let title = match page {
        Page::Desktop => "Halo".to_string(),
        other => format!("Halo — {}", other.label()),
    };
    let cascade = cx.windows().len() as f32;
    let mut bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
    bounds.origin.x += px(cascade * 28.);
    bounds.origin.y += px(cascade * 28.);

    let options = halo_window_options(cx, bounds, size(px(800.), px(520.)));
    let follows_work_tabs = page == Page::Desktop;
    match cx.open_window(options, move |window, cx| {
        window.set_window_title(&title);
        let view = cx.new(|cx| HaloApp::open(page, follows_work_tabs, window, cx));
        cx.new(|cx| Root::new(view, window, cx))
    }) {
        Ok(_) => cx.activate(true),
        Err(err) => eprintln!("[halo] failed to open window: {err}"),
    }
}

/// Open the Settings window on `tab`.
///
/// Coder and Agent entry points deep-link straight into their own section
/// instead of landing on General. `settings_tab` is shared state, so a Settings
/// window that is already open follows along.
pub fn open_settings_window(tab: SettingsTab, cx: &mut App) {
    Bridge::set_settings_tab(tab, cx);
    open_page_window(Page::Settings, cx);
}

/// Open the first (main) desktop window.
///
/// `take_focus` is false on hot-reload restarts so the new window is ordered
/// front without becoming key (`orderFront`, not `makeKeyAndOrderFront`).
pub fn open_main_window(cx: &mut App, take_focus: bool) -> gpui::Result<WindowHandle<Root>> {
    let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
    let mut options = halo_window_options(cx, bounds, size(px(960.), px(680.)));
    options.focus = take_focus;
    cx.open_window(options, |window, cx| {
        window.set_window_title("Halo");
        let view = cx.new(|cx| HaloApp::new(window, cx));
        let root = cx.new(|cx| Root::new(view, window, cx));
        root
    })
}

/// Ensure a main window is open and focused.
///
/// Invoked on macOS when the application icon is clicked in the Dock (`on_reopen`)
/// or when an already-running instance receives an activation signal.
/// Unminimizes and activates an existing main window; if only satellite windows
/// exist, activates the first one; if no windows are open, creates a fresh main window.
pub fn ensure_or_focus_main_window(cx: &mut App) {
    for handle in cx.windows() {
        let Some(window) = handle.downcast::<Root>() else {
            continue;
        };
        let Some(app) = window
            .read(cx)
            .ok()
            .and_then(|root| root.view().clone().downcast::<HaloApp>().ok())
        else {
            continue;
        };
        if app.read(cx).sub_window_page().is_none() {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            cx.activate(true);
            return;
        }
    }

    if let Some(first) = cx.windows().first() {
        let _ = first.update(cx, |_, window, _| window.activate_window());
        cx.activate(true);
        return;
    }

    let _ = open_main_window(cx, true);
    cx.activate(true);
}

//! Halo UI — Celestia/GPUI shell with a desktop, widgets, and start bar.
#![recursion_limit = "1024"]

mod actions;
mod app;
mod app_icons;
mod app_menu;
mod assets;
mod components;
mod explorer_icons;
mod fonts;
mod http_syntax;
mod icon;
mod instance;
mod keymap;
mod nav;
mod os_shell;
mod pages;
mod sound;
mod terminal;
mod theme;
mod toast;
mod windows;

pub use pages::agent::{
    AgentTranscript, RowAction, RowActionInput, preview_end, row_action, row_splice_parts,
};
pub use pages::editors::ToolEditors;

use assets::HaloAssets;
use celestia_ui::components::composite::window::bind_window_keys;
use gpui::App;
use halo_bridge::{Bridge, NativeBackend};
use keymap::bind_app_keys;
use nav::Page;

/// Register syntax, fonts and theme families used by the coder surface.
pub fn install_coder_services(cx: &mut App) {
    http_syntax::install();
    fonts::install(cx);
    theme::install(cx);
}

/// True on a cargo-watch restart (`make run`), false on the first launch
/// of that session and on a normal `cargo run`.
fn hot_reload_restart() -> bool {
    let hot = std::env::var_os("HALO_HOT_RELOAD").is_some_and(|v| v != "0" && !v.is_empty());
    if !hot {
        return false;
    }
    let marker = std::env::var_os("HALO_DEV_SESSION_MARKER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("halo-dev-session"));
    if marker.exists() {
        return true;
    }
    let _ = std::fs::write(&marker, b"1");
    false
}

pub fn run() {
    if !instance::ensure_single() {
        eprintln!("[halo] another instance of this build is already running — exiting");
        return;
    }

    let take_focus = !hot_reload_restart();

    let app = gpui_platform::application()
        .with_assets(HaloAssets)
        .with_quit_mode(gpui::QuitMode::LastWindowClosed);
    app.on_reopen(|cx| {
        windows::ensure_or_focus_main_window(cx);
    });
    app.run(move |cx| {
        icon::apply_macos_dock_icon();
        celestia_ui::init(cx);
        install_coder_services(cx);
        instance::listen_for_activations(cx);
        cx.on_app_quit(|_| {
            instance::cleanup();
            async {}
        })
        .detach();
        match NativeBackend::boot() {
            Ok(backend) => {
                Bridge::install_native(backend, cx);
            }
            Err(err) => {
                eprintln!("[halo] native backend boot failed: {err}; UI will lack proxy");
                Bridge::install(halo_bridge::StubBackend::degraded(err.to_string()), cx);
            }
        }
        fonts::apply_saved(cx);
        sound::install(cx);
        bind_app_keys(cx);
        bind_window_keys(cx);
        app_menu::install(Page::Desktop, &[], false, cx);
        windows::open_main_window(cx, take_focus).expect("failed to open the halo window");
        if take_focus {
            cx.activate(true);
        }
    });
}

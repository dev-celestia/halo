//! App icon — embedded from `assets/icons/icon.png`, the Halo ring mark.
//!
//! `assets/icons/` is the canonical generated set and the single source: the
//! bundler copies `assets/icons/icon.icns` into the `.app` and this file is
//! embedded for the dev-run window / Dock icon, so the two can never diverge.
//! Regenerate inside `assets/icons/`; nothing is copied out of it.

use std::sync::Arc;

use image::RgbaImage;

pub(crate) const ICON_PNG: &[u8] = include_bytes!("../assets/icons/icon.png");

/// Decoded RGBA icon for [`gpui::WindowOptions::icon`] (Linux/X11).
pub fn window_icon() -> Option<Arc<RgbaImage>> {
    let img = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    Some(Arc::new(img))
}

/// Set the macOS Dock / app switcher icon when running outside an `.app` bundle.
#[cfg(target_os = "macos")]
pub fn apply_macos_dock_icon() {
    #![allow(deprecated)]
    // `objc::msg_send!` expands `cfg(feature = "cargo-clippy")` which rustc warns about.
    #![allow(unexpected_cfgs)]
    use cocoa::appkit::{NSApp, NSApplication, NSImage};
    use cocoa::base::nil;
    use cocoa::foundation::{NSAutoreleasePool, NSData};
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let data = NSData::dataWithBytes_length_(
            nil,
            ICON_PNG.as_ptr() as *const std::os::raw::c_void,
            ICON_PNG.len() as u64,
        );
        let image = NSImage::initWithData_(NSImage::alloc(nil), data);
        if image != nil {
            NSApp().setApplicationIconImage_(image);
        }
        let _: () = msg_send![pool, drain];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn apply_macos_dock_icon() {}

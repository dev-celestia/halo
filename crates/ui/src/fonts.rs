//! Bundled monospace faces for the Coder editor font switcher, plus the
//! interface typeface applied from Settings → Appearance.
//!
//! System fonts (Menlo, SF Mono, …) stay optional. Open-source faces are
//! embedded under `assets/fonts/` and registered once at startup so they work
//! on every machine without a separate install.

use std::borrow::Cow;

use celestia_ui::theme::{DEFAULT_FONT_FAMILY, Theme as CelestiaTheme};
use gpui::{App, SharedString, px};

/// Bundled default for monospace surfaces (diffs, logs, code chrome).
pub const DEFAULT_MONO_FONT_FAMILY: &str = "Geist Mono";

macro_rules! face {
    ($file:literal) => {
        include_bytes!(concat!("../assets/fonts/", $file)).as_slice()
    };
}

/// Every face we ship. Family names must match `coder-fonts.json`.
const BUNDLED: &[&[u8]] = &[
    // JetBrains Mono
    face!("JetBrainsMono-Regular.ttf"),
    face!("JetBrainsMono-Bold.ttf"),
    face!("JetBrainsMono-Italic.ttf"),
    face!("JetBrainsMono-BoldItalic.ttf"),
    // Fira Code
    face!("FiraCode-Regular.ttf"),
    face!("FiraCode-Bold.ttf"),
    // Cascadia Mono
    face!("CascadiaMono-Regular.ttf"),
    face!("CascadiaMono-Bold.ttf"),
    face!("CascadiaMono-Italic.ttf"),
    face!("CascadiaMono-BoldItalic.ttf"),
    // Source Code Pro
    face!("SourceCodePro-Regular.otf"),
    face!("SourceCodePro-Bold.otf"),
    // IBM Plex Mono
    face!("IBMPlexMono-Regular.ttf"),
    face!("IBMPlexMono-Bold.ttf"),
    face!("IBMPlexMono-Italic.ttf"),
    face!("IBMPlexMono-BoldItalic.ttf"),
    // Geist Mono
    face!("GeistMono-Regular.ttf"),
    face!("GeistMono-Bold.ttf"),
];

/// Register embedded coder fonts with GPUI. Call once during app boot, before
/// any editor measures text with these families.
pub fn install(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = BUNDLED.iter().map(|face| Cow::Borrowed(*face)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("[halo] failed to register bundled coder fonts: {err}");
    }
}

/// Interface faces offered in Settings → Appearance → Typography, in menu
/// order.
///
/// A curated shortlist rather than every installed family: symbol and
/// decorative faces would render the chrome as garbage, and macOS reports
/// several hundred families. Entries the platform does not have are hidden by
/// [`installed_ui_faces`], so the menu can never point at a face GPUI cannot
/// resolve (an unknown family panics on the first text layout).
///
/// Monospace faces are included on purpose: a mono interface is a real
/// preference, and the bundled monos are registered by [`install`] anyway, so
/// offering them costs nothing. They are listed last, after the sans faces.
pub const UI_FACES: &[&str] = &[
    // App default (bundled)
    "Geist",
    // macOS
    "SF Pro Text",
    "SF Pro Display",
    "Helvetica Neue",
    "Avenir Next",
    // Windows
    "Segoe UI",
    "Tahoma",
    // Common Linux
    "Inter",
    "Noto Sans",
    "Cantarell",
    "Ubuntu",
    "DejaVu Sans",
    "Liberation Sans",
    // Other bundled / cross-platform open source
    "IBM Plex Sans",
    "Source Sans 3",
    "Open Sans",
    "Lato",
    "Roboto",
    "Manrope",
    "Figtree",
    // Everywhere
    "Arial",
    "Helvetica",
    "Verdana",
    // Bundled monospace, for a mono interface
    "JetBrains Mono",
];

/// Monospace faces for Settings → Appearance → Font mono.
pub const MONO_FACES: &[&str] = &[
    // App default (bundled)
    "Geist Mono",
    // Other bundled faces
    "JetBrains Mono",
    "Fira Code",
    "Cascadia Mono",
    "Source Code Pro",
    "IBM Plex Mono",
    // Platform
    "Menlo",
    "SF Mono",
    "Monaco",
    "Consolas",
    "Courier New",
    "DejaVu Sans Mono",
    "Noto Sans Mono",
    "Ubuntu Mono",
];

/// [`UI_FACES`] narrowed to the families this machine actually has.
///
/// Always includes `current` even when it is not in [`UI_FACES`] (a face that
/// was picked from an older build, or a hand-edited `shell.json`), so the
/// dropdown can still show — and re-select — the active family.
pub fn installed_ui_faces(cx: &App, current: Option<&str>) -> Vec<SharedString> {
    with_current(installed_catalog(cx, UI_FACES), current)
}

/// [`MONO_FACES`] narrowed the same way. Bundled monos are always present
/// after [`install`].
pub fn installed_mono_faces(cx: &App, current: Option<&str>) -> Vec<SharedString> {
    with_current(installed_catalog(cx, MONO_FACES), current)
}

fn with_current(mut faces: Vec<SharedString>, current: Option<&str>) -> Vec<SharedString> {
    if let Some(current) = current.filter(|name| !name.trim().is_empty()) {
        if !faces.iter().any(|face| face.as_ref() == current) {
            faces.insert(0, SharedString::from(current));
        }
    }
    faces
}

/// The filtered list, resolved once per catalog.
///
/// Enumerating system families costs a few milliseconds, and the Appearance
/// tab re-renders on every state change — cache it. Safe because [`install`]
/// registers the bundled faces at boot, before the first render.
fn installed_catalog(cx: &App, catalog: &'static [&'static str]) -> Vec<SharedString> {
    // Two small caches keyed by which list we filtered.
    if std::ptr::eq(catalog, UI_FACES) {
        static UI: std::sync::OnceLock<Vec<SharedString>> = std::sync::OnceLock::new();
        return UI.get_or_init(|| filter_catalog(cx, catalog)).clone();
    }
    static MONO: std::sync::OnceLock<Vec<SharedString>> = std::sync::OnceLock::new();
    MONO.get_or_init(|| filter_catalog(cx, catalog)).clone()
}

fn filter_catalog(cx: &App, catalog: &[&str]) -> Vec<SharedString> {
    let installed = cx.text_system().all_font_names();
    catalog
        .iter()
        .filter(|name| {
            installed
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(name))
        })
        .map(|name| SharedString::from(*name))
        .collect()
}

/// Push UI + mono typography onto both theme globals.
///
/// `celestia_ui`'s `AppFont::write` is crate-private and `AppTheme::apply`
/// would also re-assert the Celestia colours over the active family, so the
/// fields it writes are set here directly: the Celestia `Theme` every
/// component reads through `cx.theme()`, and `gpui-component`'s theme, which
/// the window root uses for the cascading family and the rem size (so
/// `text_sm()` and friends follow the size). Mode switches replace colours
/// only, so both survive a light/dark toggle.
///
/// `None` restores Geist / Geist Mono at 16px.
pub fn apply_app_font(
    family: Option<&str>,
    mono_family: Option<&str>,
    size: Option<u16>,
    cx: &mut App,
) {
    let family: SharedString = family
        .filter(|name| !name.trim().is_empty())
        .map(SharedString::from)
        .unwrap_or_else(|| DEFAULT_FONT_FAMILY.into());
    let mono: SharedString = mono_family
        .filter(|name| !name.trim().is_empty())
        .map(SharedString::from)
        .unwrap_or_else(|| DEFAULT_MONO_FONT_FAMILY.into());
    let size = px(size.map(f32::from).unwrap_or(16.));

    {
        let theme = CelestiaTheme::global_mut(cx);
        theme.font_family = family.clone();
        theme.mono_font_family = mono.clone();
        theme.font_size = size;
    }

    if cx.has_global::<gpui_component::Theme>() {
        gpui_component::Theme::update(cx, |theme| {
            theme.font_family = family;
            theme.mono_font_family = mono;
            theme.font_size = size;
        });
    }

    cx.refresh_windows();
}

/// Re-apply whatever typography the bridge has stored.
///
/// One call site for "the user just changed a font setting" and "the app is
/// booting": both read the same persisted values.
pub fn apply_saved(cx: &mut App) {
    let appearance = halo_bridge::Bridge::state(cx).read(cx).appearance.clone();
    apply_app_font(
        appearance.font_family.as_deref(),
        appearance.mono_font_family.as_deref(),
        appearance.font_size,
        cx,
    );
}

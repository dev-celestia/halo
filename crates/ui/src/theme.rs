//! Theme families for Halo — built on [`celestia_ui::theme::AppTheme`].
//!
//! [`celestia_ui::init`] seeds the `Celestia` family from `theme.json`. Extra
//! families are registered here so Settings → Appearance can switch among them
//! with [`celestia_ui::theme::select`]. Light/dark mode stays on
//! [`Theme::change`] / [`Theme::toggle`] within the active family.
//!
//! User-facing switches go through [`change_mode`] / [`toggle_mode`] /
//! [`select_family`], which paint a brief loading overlay before the sync
//! refresh so the lag does not feel like a freeze.
//!
//! Canvas background colors are kept soft to reduce ocular fatigue ("tidak membuat mata cepat lelah").

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::theme::{
    ActiveTheme as _, AppTheme, Scheme, Theme, ThemeMode, active, hex, select,
};
use gpui::{
    Anchor, App, AsyncApp, BoxShadow, FontWeight, Global, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, SharedString, Styled as _, div, hsla, point, px, relative,
    rgba,
};
use gpui_component::highlighter::HighlightTheme;

/// Yield so the switching overlay can paint before `refresh_windows` runs.
const SWITCH_YIELD_MS: u64 = 32;

/// What a in-flight theme switch is applying — used by chip loading marks.
#[derive(Clone, Default)]
pub enum SwitchingTarget {
    #[default]
    None,
    Mode(ThemeMode),
    Family(SharedString),
}

/// App-wide “theme is applying” flag — drives the fullscreen overlay.
#[derive(Default)]
struct ThemeTransition {
    busy: bool,
    target: SwitchingTarget,
}

impl Global for ThemeTransition {}

/// True while a family or light/dark switch is in flight.
pub fn is_switching(cx: &App) -> bool {
    cx.try_global::<ThemeTransition>().is_some_and(|t| t.busy)
}

/// The pending switch target, if any — for button loading marks.
pub fn switching_target(cx: &App) -> SwitchingTarget {
    cx.try_global::<ThemeTransition>()
        .map(|t| {
            if t.busy {
                t.target.clone()
            } else {
                SwitchingTarget::None
            }
        })
        .unwrap_or_default()
}

/// Switch light / dark with a loading overlay. No-op when already on `mode`
/// or a switch is already running.
pub fn change_mode(mode: ThemeMode, cx: &mut App) {
    let dark = Theme::global(cx).is_dark();
    let already = matches!(
        (mode, dark),
        (ThemeMode::Dark, true) | (ThemeMode::Light, false)
    );
    if already {
        return;
    }
    begin_switch(Pending::Mode(mode), SwitchingTarget::Mode(mode), cx);
}

/// Toggle light / dark with a loading overlay.
pub fn toggle_mode(cx: &mut App) {
    let next = if Theme::global(cx).is_dark() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    begin_switch(Pending::Toggle, SwitchingTarget::Mode(next), cx);
}

/// Activate a theme family with a loading overlay. No-op when already active
/// or a switch is already running.
pub fn select_family(name: impl Into<SharedString>, cx: &mut App) {
    let name = name.into();
    if active(cx).as_ref() == name.as_ref() {
        return;
    }
    begin_switch(
        Pending::Family(name.clone()),
        SwitchingTarget::Family(name),
        cx,
    );
}

/// Full-window scrim + GridShift mark shown while a theme switch applies.
pub fn render_switching_overlay(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    // Match celestia Scrim defaults (background wash) with an interaction lock
    // so clicks do not reach chrome mid-refresh.
    div()
        .id("theme-switching-overlay")
        .absolute()
        .inset_0()
        .size(relative(1.))
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background.opacity(0.55))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(10.))
                .px(px(18.))
                .py(px(14.))
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow(vec![BoxShadow {
                    offset: point(px(0.), px(8.)),
                    blur_radius: px(24.),
                    spread_radius: px(0.),
                    inset: false,
                    color: rgba(0x00000044).into(),
                }])
                .child(Loader::grid_shift().size(px(28.)).color(theme.primary))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.muted_foreground)
                        .child("Applying theme…"),
                ),
        )
}

enum Pending {
    Mode(ThemeMode),
    Toggle,
    Family(SharedString),
}

fn begin_switch(pending: Pending, target: SwitchingTarget, cx: &mut App) {
    {
        let transition = cx.default_global::<ThemeTransition>();
        if transition.busy {
            return;
        }
        transition.busy = true;
        transition.target = target;
    }
    // Paint the overlay on the current theme before the expensive refresh.
    cx.refresh_windows();

    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor()
            .timer(Duration::from_millis(SWITCH_YIELD_MS))
            .await;
        let _ = cx.update(|cx| {
            match &pending {
                Pending::Mode(mode) => Theme::change(*mode, cx),
                Pending::Toggle => Theme::toggle(cx),
                Pending::Family(name) => {
                    let _ = select(name, cx);
                }
            }
            if cx.has_global::<ThemeTransition>() {
                let t = cx.global_mut::<ThemeTransition>();
                t.busy = false;
                t.target = SwitchingTarget::None;
            }
            // Theme::change / select already refreshed; one more pass clears
            // the overlay with the new palette.
            cx.refresh_windows();
        });
    })
    .detach();
}

/// Named presets shown in Appearance. `Celestia` is seeded by `celestia_ui::init`.
const PRESETS: &[(&str, fn() -> AppTheme)] = &[
    ("Rose", rose),
    ("Crimson", crimson),
    ("Ocean", ocean),
    ("Terminal", terminal),
    ("Amber", amber),
    ("Graphite", graphite),
    ("Nocturne", nocturne),
];

/// Register Halo theme families and leave Celestia dark active.
pub fn install(cx: &mut App) {
    // Seed Celestia with the warm red palette (soft canvas, deepened surfaces)
    // before other families are built from the stock pair.
    celestia().apply(cx);

    // Mode switches reset editor colors to the stock grammar theme. While
    // Celestia, Rose, or Crimson is active, put that family's syntax colors back.
    cx.observe_global::<gpui_component::Theme>(|cx| {
        sync_editor_colors(cx);
    })
    .detach();

    for (name, build) in PRESETS {
        build().register(*name, cx);
    }
    let _ = select("Celestia", cx);
    Theme::change(ThemeMode::Dark, cx);
    sync_editor_colors(cx);

    // gpui-component defaults to TopRight; Halo toasts sit bottom-right
    // above the 40px start bar (default bottom margin is only 16px).
    gpui_component::Theme::update(cx, |theme| {
        theme.notification.placement = Anchor::BottomRight;
        theme.notification.margins.bottom = px(56.);
    });
}

/// Purple-night palette sampled from the reference client: lavender text on
/// an indigo canvas, iris as the accent, gold strings, pink booleans, mint
/// success, and amber warnings.
///
/// Dark values are the Rosé Pine main ramp on a deeper indigo page (`#07060c`;
/// text on that page is 15.3:1). Light values are Rosé Pine Dawn surfaces on
/// a paler indigo page (`#f9f8fc`).
/// Dawn's mid-tone accents fail 4.5:1 on the cream page, so those fills are
/// the same hue darkened until the pair clears 4.5:1 (`#7d649a` iris 4.6:1,
/// `#9d6110` gold 4.6:1, `#ab526c` love 4.6:1, `#487a43` green 4.6:1,
/// `#45767f` foam 4.6:1, `#6a6682` muted text 4.6:1 on the darkest dawn
/// surface).
fn rose() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfefefe)),
            foreground: Some(hex(0x575279)),
            surface: Some(hex(0xfffefc)),
            popover: Some(hex(0xfffefc)),
            popover_foreground: Some(hex(0x575279)),
            primary: Some(hex(0x7d649a)),
            primary_foreground: Some(hex(0xfaf4ed)),
            secondary: Some(hex(0xfcfaf8)),
            secondary_foreground: Some(hex(0x575279)),
            muted: Some(hex(0xfcfaf8)),
            muted_foreground: Some(hex(0x6a6682)),
            accent: Some(hex(0xfcfaf8)),
            accent_foreground: Some(hex(0x575279)),
            destructive: Some(hex(0xab526c)),
            destructive_foreground: Some(hex(0xfaf4ed)),
            success: Some(hex(0x487a43)),
            success_foreground: Some(hex(0xfaf4ed)),
            warning: Some(hex(0x9d6110)),
            warning_foreground: Some(hex(0xfaf4ed)),
            info: Some(hex(0x45767f)),
            info_foreground: Some(hex(0xfaf4ed)),
            border: None,
            input: None,
            ring: Some(hex(0x7d649a)),
            sidebar: Some(hex(0xfffefc)),
            sidebar_foreground: Some(hex(0x575279)),
            sidebar_border: None,
            sidebar_primary: Some(hex(0x7d649a)),
            sidebar_primary_foreground: Some(hex(0xfaf4ed)),
            sidebar_accent: Some(hex(0xfcfaf8)),
            sidebar_accent_foreground: Some(hex(0x575279)),
        },
        dark: Scheme {
            background: Some(hex(0x020203)),
            foreground: Some(hex(0xe0def4)),
            surface: Some(hex(0x08070c)),
            popover: Some(hex(0x0a090e)),
            popover_foreground: Some(hex(0xe0def4)),
            primary: Some(hex(0xc4a7e7)),
            primary_foreground: Some(hex(0x191724)),
            secondary: Some(hex(0x0a090e)),
            secondary_foreground: Some(hex(0xe0def4)),
            muted: Some(hex(0x08080c)),
            muted_foreground: Some(hex(0x908caa)),
            accent: Some(hex(0x0a090e)),
            accent_foreground: Some(hex(0xe0def4)),
            destructive: Some(hex(0xeb6f92)),
            destructive_foreground: Some(hex(0x191724)),
            success: Some(hex(0xa6e3a1)),
            success_foreground: Some(hex(0x191724)),
            warning: Some(hex(0xf6c177)),
            warning_foreground: Some(hex(0x191724)),
            info: Some(hex(0x9ccfd8)),
            info_foreground: Some(hex(0x191724)),
            border: None,
            input: None,
            ring: Some(hex(0xc4a7e7)),
            sidebar: Some(hex(0x08070c)),
            sidebar_foreground: Some(hex(0xe0def4)),
            sidebar_border: None,
            sidebar_primary: Some(hex(0xc4a7e7)),
            sidebar_primary_foreground: Some(hex(0x191724)),
            sidebar_accent: Some(hex(0x0a090e)),
            sidebar_accent_foreground: Some(hex(0xe0def4)),
        },
        ..Default::default()
    }
}

/// Rose with the accent moved from iris to crimson, and the neutrals warmed
/// to the same hue. Delete is a true red (`#dc2626` / `#f87171`), clear of the
/// coral primary so destructive controls read as danger rather than accent.
///
/// Dark text `#eee8e7` on the indigo page `#07060c` is 16.7:1. Accent `#fb7185`
/// on that page is 7.5:1, and the dark ink on the accent fill is 6.4:1. Muted `#ae989b`
/// is 7.5:1 on the page and 5.4:1 on the overlay. Light text `#764249` on
/// `#f9f8fc` is 7.5:1 (6.5:1 on the overlay). Light accent `#ad1f3b` is 6.5:1
/// on the page. Light status colors are the same hues as Rose, darkened until
/// they clear 4.5:1 on the blush overlay (`#945b0f` 4.6:1, `#44733f` 4.6:1,
/// `#416e77` 4.7:1).
fn crimson() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfefefe)),
            foreground: Some(hex(0x764249)),
            surface: Some(hex(0xfefdfc)),
            popover: Some(hex(0xfcf9f8)),
            popover_foreground: Some(hex(0x764249)),
            primary: Some(hex(0xad1f3b)),
            primary_foreground: Some(hex(0xf9f0ee)),
            secondary: Some(hex(0xfcf9f8)),
            secondary_foreground: Some(hex(0x764249)),
            muted: Some(hex(0xfcf9f8)),
            muted_foreground: Some(hex(0x7a5c60)),
            accent: Some(hex(0xfcf9f8)),
            accent_foreground: Some(hex(0x764249)),
            destructive: Some(hex(0xdc2626)),
            destructive_foreground: Some(hex(0xf9f0ee)),
            success: Some(hex(0x44733f)),
            success_foreground: Some(hex(0xf9f0ee)),
            warning: Some(hex(0x945b0f)),
            warning_foreground: Some(hex(0xf9f0ee)),
            info: Some(hex(0x416e77)),
            info_foreground: Some(hex(0xf9f0ee)),
            border: None,
            input: None,
            ring: Some(hex(0xad1f3b)),
            sidebar: Some(hex(0xfefdfc)),
            sidebar_foreground: Some(hex(0x764249)),
            sidebar_border: None,
            sidebar_primary: Some(hex(0xad1f3b)),
            sidebar_primary_foreground: Some(hex(0xf9f0ee)),
            sidebar_accent: Some(hex(0xfcf9f8)),
            sidebar_accent_foreground: Some(hex(0x764249)),
        },
        dark: Scheme {
            background: Some(hex(0x020203)),
            foreground: Some(hex(0xeee8e7)),
            surface: Some(hex(0x0c0708)),
            popover: Some(hex(0x0e090a)),
            popover_foreground: Some(hex(0xeee8e7)),
            primary: Some(hex(0xfb7185)),
            primary_foreground: Some(hex(0x241719)),
            secondary: Some(hex(0x0e090a)),
            secondary_foreground: Some(hex(0xeee8e7)),
            muted: Some(hex(0x0c0808)),
            muted_foreground: Some(hex(0xae989b)),
            accent: Some(hex(0x0e090a)),
            accent_foreground: Some(hex(0xeee8e7)),
            destructive: Some(hex(0xf87171)),
            destructive_foreground: Some(hex(0x241719)),
            success: Some(hex(0xa6e3a1)),
            success_foreground: Some(hex(0x241719)),
            warning: Some(hex(0xf6c177)),
            warning_foreground: Some(hex(0x241719)),
            info: Some(hex(0x9ccfd8)),
            info_foreground: Some(hex(0x241719)),
            border: None,
            input: None,
            ring: Some(hex(0xfb7185)),
            sidebar: Some(hex(0x0c0708)),
            sidebar_foreground: Some(hex(0xeee8e7)),
            sidebar_border: None,
            sidebar_primary: Some(hex(0xfb7185)),
            sidebar_primary_foreground: Some(hex(0x241719)),
            sidebar_accent: Some(hex(0x0e090a)),
            sidebar_accent_foreground: Some(hex(0xeee8e7)),
        },
        ..Default::default()
    }
}

/// Warm crimson accents on a flat charcoal canvas — page/surface/sidebar share
/// `#1a1a1a` so rails stay even (no mid-gray slabs). Elevated washes are a step
/// up (`#242424` popover, `#2a2a2a` muted) for row chips only. Light page
/// `#fefefe`; blush washes `#fefdfc` / `#fcf9f8`. Accent stays `#fb7185`.
fn celestia() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfefefe)),
            foreground: Some(hex(0x764249)),
            surface: Some(hex(0xfefdfc)),
            popover: Some(hex(0xfcf9f8)),
            popover_foreground: Some(hex(0x764249)),
            primary: Some(hex(0xad1f3b)),
            primary_foreground: Some(hex(0xf9f0ee)),
            secondary: Some(hex(0xfcf9f8)),
            secondary_foreground: Some(hex(0x764249)),
            muted: Some(hex(0xfcf9f8)),
            muted_foreground: Some(hex(0x7a5c60)),
            accent: Some(hex(0xfcf9f8)),
            accent_foreground: Some(hex(0x764249)),
            destructive: Some(hex(0xdc2626)),
            destructive_foreground: Some(hex(0xf9f0ee)),
            success: Some(hex(0x44733f)),
            success_foreground: Some(hex(0xf9f0ee)),
            warning: Some(hex(0x945b0f)),
            warning_foreground: Some(hex(0xf9f0ee)),
            info: Some(hex(0x416e77)),
            info_foreground: Some(hex(0xf9f0ee)),
            border: None,
            input: None,
            ring: Some(hex(0xad1f3b)),
            sidebar: Some(hex(0xfefdfc)),
            sidebar_foreground: Some(hex(0x764249)),
            sidebar_border: None,
            sidebar_primary: Some(hex(0xad1f3b)),
            sidebar_primary_foreground: Some(hex(0xf9f0ee)),
            sidebar_accent: Some(hex(0xfcf9f8)),
            sidebar_accent_foreground: Some(hex(0x764249)),
        },
        dark: Scheme {
            background: Some(hex(0x1a1a1a)),
            foreground: Some(hex(0xe8e8e8)),
            surface: Some(hex(0x1a1a1a)),
            popover: Some(hex(0x242424)),
            popover_foreground: Some(hex(0xe8e8e8)),
            primary: Some(hex(0xfb7185)),
            primary_foreground: Some(hex(0x241719)),
            secondary: Some(hex(0x242424)),
            secondary_foreground: Some(hex(0xe8e8e8)),
            muted: Some(hex(0x2a2a2a)),
            muted_foreground: Some(hex(0x9a9a9a)),
            accent: Some(hex(0x242424)),
            accent_foreground: Some(hex(0xe8e8e8)),
            destructive: Some(hex(0xf87171)),
            destructive_foreground: Some(hex(0x241719)),
            success: Some(hex(0xa6e3a1)),
            success_foreground: Some(hex(0x241719)),
            warning: Some(hex(0xf6c177)),
            warning_foreground: Some(hex(0x241719)),
            info: Some(hex(0x9ccfd8)),
            info_foreground: Some(hex(0x241719)),
            border: Some(hsla(0., 0., 1., 0.08)),
            input: Some(hsla(0., 0., 1., 0.12)),
            ring: Some(hex(0xfb7185)),
            sidebar: Some(hex(0x1a1a1a)),
            sidebar_foreground: Some(hex(0xe8e8e8)),
            sidebar_border: Some(hsla(0., 0., 1., 0.08)),
            sidebar_primary: Some(hex(0xfb7185)),
            sidebar_primary_foreground: Some(hex(0x241719)),
            sidebar_accent: Some(hex(0x242424)),
            sidebar_accent_foreground: Some(hex(0xe8e8e8)),
        },
        ..Default::default()
    }
}

fn ocean() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfcfcfe)),
            primary: Some(hex(0x2563eb)),
            ..Default::default()
        },
        dark: Scheme {
            background: Some(hex(0x050608)),
            primary: Some(hex(0x6aa5ff)),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn terminal() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfcfcfe)),
            primary: Some(hex(0x16a34a)),
            ..Default::default()
        },
        dark: Scheme {
            background: Some(hex(0x020403)),
            foreground: Some(hex(0xd1fae5)),
            surface: Some(hex(0x040605)),
            popover: Some(hex(0x030504)),
            primary: Some(hex(0x4ade80)),
            secondary: Some(hex(0x060a08)),
            muted: Some(hex(0x060a08)),
            muted_foreground: Some(hex(0x86a896)),
            accent: Some(hex(0x060a08)),
            // Match Celestia dark hairlines: white wash, not near-black solids
            // that vanish on `#020403`.
            border: Some(hsla(0., 0., 1., 0.11)),
            input: Some(hsla(0., 0., 1., 0.14)),
            sidebar: Some(hex(0x030404)),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn amber() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfcfcfe)),
            primary: Some(hex(0xd97706)),
            ..Default::default()
        },
        dark: Scheme {
            background: Some(hex(0x050608)),
            primary: Some(hex(0xfbbf24)),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn graphite() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfcfcfe)),
            primary: Some(hex(0x525252)),
            ..Default::default()
        },
        dark: Scheme {
            background: Some(hex(0x050608)),
            primary: Some(hex(0xa3a3a3)),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn nocturne() -> AppTheme {
    AppTheme {
        light: Scheme {
            background: Some(hex(0xfcfcfe)),
            primary: Some(hex(0x38bdf8)),
            ..Default::default()
        },
        dark: Scheme {
            background: Some(hex(0x030408)),
            foreground: Some(hex(0xe2e8f0)),
            surface: Some(hex(0x05070c)),
            popover: Some(hex(0x04060a)),
            primary: Some(hex(0x38bdf8)),
            secondary: Some(hex(0x080a0f)),
            muted: Some(hex(0x080a0f)),
            muted_foreground: Some(hex(0x94a3b8)),
            accent: Some(hex(0x080a0f)),
            // Same wash as Celestia dark so cards and chrome stay readable on
            // the near-black nocturne page.
            border: Some(hsla(0., 0., 1., 0.11)),
            input: Some(hsla(0., 0., 1., 0.14)),
            sidebar: Some(hex(0x03050a)),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Editor grammar colors for Celestia, Rose, and Crimson. `Theme::change`
/// installs the stock highlight theme, which paints strings green and
/// booleans gold — the reverse of these palettes — so the active family puts
/// its own theme back.
fn sync_editor_colors(cx: &mut App) {
    static BUSY: AtomicBool = AtomicBool::new(false);
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            BUSY.store(false, Ordering::SeqCst);
        }
    }
    let _clear = Clear;

    let Some((name, json)) = syntax_theme(active(cx).as_ref(), Theme::global(cx).is_dark()) else {
        return;
    };
    let already = gpui_component::Theme::global(cx).highlight_theme.name == name;
    if !already {
        let highlight = Arc::new(
            serde_json::from_str::<HighlightTheme>(json).expect("editor theme JSON must parse"),
        );
        gpui_component::Theme::update(cx, |theme| {
            theme.highlight_theme = highlight;
        });
    }
}

fn syntax_theme(family: &str, dark: bool) -> Option<(&'static str, &'static str)> {
    match (family, dark) {
        ("Celestia", true) => Some(("Celestia Dark", CELESTIA_DARK_HIGHLIGHT)),
        ("Celestia", false) => Some(("Celestia Light", CELESTIA_LIGHT_HIGHLIGHT)),
        ("Rose", true) => Some(("Rose Dark", ROSE_DARK_HIGHLIGHT)),
        ("Rose", false) => Some(("Rose Light", ROSE_LIGHT_HIGHLIGHT)),
        ("Crimson", true) => Some(("Crimson Dark", CRIMSON_DARK_HIGHLIGHT)),
        ("Crimson", false) => Some(("Crimson Light", CRIMSON_LIGHT_HIGHLIGHT)),
        _ => None,
    }
}

const CELESTIA_DARK_HIGHLIGHT: &str = r##"{
  "name": "Celestia Dark",
  "appearance": "dark",
  "style": {
    "editor.background": "#1a1a1a",
    "editor.foreground": "#e8e8e8",
    "editor.active_line.background": "#222222",
    "editor.line_number": "#9a9a9a",
    "editor.active_line_number": "#e8e8e8",
    "editor.gutter.background": "#1a1a1a",
    "syntax": {
      "attribute": { "color": "#fb7185" },
      "boolean": { "color": "#e25ab9" },
      "comment": { "color": "#9a9a9a" },
      "comment.doc": { "color": "#9a9a9a" },
      "constant": { "color": "#fb7185" },
      "constructor": { "color": "#fb7185" },
      "function": { "color": "#fb7185" },
      "keyword": { "color": "#9ccfd8" },
      "link_text": { "color": "#fb7185" },
      "link_uri": { "color": "#fb7185" },
      "number": { "color": "#f6c177" },
      "operator": { "color": "#9a9a9a" },
      "property": { "color": "#e8e8e8" },
      "punctuation": { "color": "#9a9a9a" },
      "string": { "color": "#f6c177" },
      "string.escape": { "color": "#e25ab9" },
      "string.regex": { "color": "#e25ab9" },
      "string.special": { "color": "#9ccfd8" },
      "string.special.symbol": { "color": "#9ccfd8" },
      "tag": { "color": "#fb7185" },
      "text.literal": { "color": "#f6c177" },
      "type": { "color": "#9ccfd8" },
      "variable": { "color": "#e8e8e8" },
      "variable.special": { "color": "#9ccfd8" }
    }
  }
}"##;

const CELESTIA_LIGHT_HIGHLIGHT: &str = r##"{
  "name": "Celestia Light",
  "appearance": "light",
  "style": {
    "editor.background": "#fefefe",
    "editor.foreground": "#764249",
    "editor.active_line.background": "#fcfafc",
    "editor.line_number": "#7a5c60",
    "editor.active_line_number": "#764249",
    "editor.gutter.background": "#fefefe",
    "syntax": {
      "attribute": { "color": "#ad1f3b" },
      "boolean": { "color": "#8d3572" },
      "comment": { "color": "#7a5c60" },
      "comment.doc": { "color": "#7a5c60" },
      "constant": { "color": "#ad1f3b" },
      "constructor": { "color": "#ad1f3b" },
      "function": { "color": "#ad1f3b" },
      "keyword": { "color": "#416e77" },
      "link_text": { "color": "#ad1f3b" },
      "link_uri": { "color": "#ad1f3b" },
      "number": { "color": "#945b0f" },
      "operator": { "color": "#7a5c60" },
      "property": { "color": "#764249" },
      "punctuation": { "color": "#7a5c60" },
      "string": { "color": "#945b0f" },
      "string.escape": { "color": "#8d3572" },
      "string.regex": { "color": "#8d3572" },
      "string.special": { "color": "#416e77" },
      "string.special.symbol": { "color": "#416e77" },
      "tag": { "color": "#ad1f3b" },
      "text.literal": { "color": "#945b0f" },
      "type": { "color": "#416e77" },
      "variable": { "color": "#764249" },
      "variable.special": { "color": "#416e77" }
    }
  }
}"##;

const ROSE_DARK_HIGHLIGHT: &str = r##"{
  "name": "Rose Dark",
  "appearance": "dark",
  "style": {
    "editor.background": "#020203",
    "editor.foreground": "#e0def4",
    "editor.active_line.background": "#040406",
    "editor.line_number": "#908caa",
    "editor.active_line_number": "#e0def4",
    "editor.gutter.background": "#020203",
    "syntax": {
      "attribute": { "color": "#c4a7e7" },
      "boolean": { "color": "#eb6f92" },
      "comment": { "color": "#908caa" },
      "comment.doc": { "color": "#908caa" },
      "constant": { "color": "#c4a7e7" },
      "constructor": { "color": "#c4a7e7" },
      "function": { "color": "#c4a7e7" },
      "keyword": { "color": "#9ccfd8" },
      "link_text": { "color": "#c4a7e7" },
      "link_uri": { "color": "#c4a7e7" },
      "number": { "color": "#f6c177" },
      "operator": { "color": "#908caa" },
      "property": { "color": "#e0def4" },
      "punctuation": { "color": "#908caa" },
      "string": { "color": "#f6c177" },
      "string.escape": { "color": "#eb6f92" },
      "string.regex": { "color": "#eb6f92" },
      "string.special": { "color": "#9ccfd8" },
      "string.special.symbol": { "color": "#9ccfd8" },
      "tag": { "color": "#c4a7e7" },
      "text.literal": { "color": "#f6c177" },
      "type": { "color": "#9ccfd8" },
      "variable": { "color": "#e0def4" },
      "variable.special": { "color": "#9ccfd8" }
    }
  }
}"##;

const ROSE_LIGHT_HIGHLIGHT: &str = r##"{
  "name": "Rose Light",
  "appearance": "light",
  "style": {
    "editor.background": "#fefefe",
    "editor.foreground": "#575279",
    "editor.active_line.background": "#fcfbfc",
    "editor.line_number": "#6a6682",
    "editor.active_line_number": "#575279",
    "editor.gutter.background": "#fefefe",
    "syntax": {
      "attribute": { "color": "#7d649a" },
      "boolean": { "color": "#ab526c" },
      "comment": { "color": "#6a6682" },
      "comment.doc": { "color": "#6a6682" },
      "constant": { "color": "#7d649a" },
      "constructor": { "color": "#7d649a" },
      "function": { "color": "#7d649a" },
      "keyword": { "color": "#45767f" },
      "link_text": { "color": "#7d649a" },
      "link_uri": { "color": "#7d649a" },
      "number": { "color": "#9d6110" },
      "operator": { "color": "#6a6682" },
      "property": { "color": "#575279" },
      "punctuation": { "color": "#6a6682" },
      "string": { "color": "#9d6110" },
      "string.escape": { "color": "#ab526c" },
      "string.regex": { "color": "#ab526c" },
      "string.special": { "color": "#45767f" },
      "string.special.symbol": { "color": "#45767f" },
      "tag": { "color": "#7d649a" },
      "text.literal": { "color": "#9d6110" },
      "type": { "color": "#45767f" },
      "variable": { "color": "#575279" },
      "variable.special": { "color": "#45767f" }
    }
  }
}"##;

const CRIMSON_DARK_HIGHLIGHT: &str = r##"{
  "name": "Crimson Dark",
  "appearance": "dark",
  "style": {
    "editor.background": "#020203",
    "editor.foreground": "#eee8e7",
    "editor.active_line.background": "#040406",
    "editor.line_number": "#ae989b",
    "editor.active_line_number": "#eee8e7",
    "editor.gutter.background": "#020203",
    "syntax": {
      "attribute": { "color": "#fb7185" },
      "boolean": { "color": "#e25ab9" },
      "comment": { "color": "#ae989b" },
      "comment.doc": { "color": "#ae989b" },
      "constant": { "color": "#fb7185" },
      "constructor": { "color": "#fb7185" },
      "function": { "color": "#fb7185" },
      "keyword": { "color": "#9ccfd8" },
      "link_text": { "color": "#fb7185" },
      "link_uri": { "color": "#fb7185" },
      "number": { "color": "#f6c177" },
      "operator": { "color": "#ae989b" },
      "property": { "color": "#eee8e7" },
      "punctuation": { "color": "#ae989b" },
      "string": { "color": "#f6c177" },
      "string.escape": { "color": "#e25ab9" },
      "string.regex": { "color": "#e25ab9" },
      "string.special": { "color": "#9ccfd8" },
      "string.special.symbol": { "color": "#9ccfd8" },
      "tag": { "color": "#fb7185" },
      "text.literal": { "color": "#f6c177" },
      "type": { "color": "#9ccfd8" },
      "variable": { "color": "#eee8e7" },
      "variable.special": { "color": "#9ccfd8" }
    }
  }
}"##;

const CRIMSON_LIGHT_HIGHLIGHT: &str = r##"{
  "name": "Crimson Light",
  "appearance": "light",
  "style": {
    "editor.background": "#fefefe",
    "editor.foreground": "#764249",
    "editor.active_line.background": "#fcfbfc",
    "editor.line_number": "#7a5c60",
    "editor.active_line_number": "#764249",
    "editor.gutter.background": "#fefefe",
    "syntax": {
      "attribute": { "color": "#ad1f3b" },
      "boolean": { "color": "#8d3572" },
      "comment": { "color": "#7a5c60" },
      "comment.doc": { "color": "#7a5c60" },
      "constant": { "color": "#ad1f3b" },
      "constructor": { "color": "#ad1f3b" },
      "function": { "color": "#ad1f3b" },
      "keyword": { "color": "#416e77" },
      "link_text": { "color": "#ad1f3b" },
      "link_uri": { "color": "#ad1f3b" },
      "number": { "color": "#945b0f" },
      "operator": { "color": "#7a5c60" },
      "property": { "color": "#764249" },
      "punctuation": { "color": "#7a5c60" },
      "string": { "color": "#945b0f" },
      "string.escape": { "color": "#8d3572" },
      "string.regex": { "color": "#8d3572" },
      "string.special": { "color": "#416e77" },
      "string.special.symbol": { "color": "#416e77" },
      "tag": { "color": "#ad1f3b" },
      "text.literal": { "color": "#945b0f" },
      "type": { "color": "#416e77" },
      "variable": { "color": "#764249" },
      "variable.special": { "color": "#416e77" }
    }
  }
}"##;

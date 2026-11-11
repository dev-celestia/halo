//! Appearance — theme family + light / dark mode (AppTheme), and the app-wide
//! UI + mono typefaces.

use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::number_stepper::NumberStepper;
use celestia_ui::theme::{ActiveTheme as _, DEFAULT_FONT_FAMILY, ThemeMode, active, families};
use gpui::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use halo_bridge::{Bridge, BridgeState};

use crate::fonts::{
    DEFAULT_MONO_FONT_FAMILY, apply_saved, installed_mono_faces, installed_ui_faces,
};
use crate::pages::chrome::section;
use crate::pages::settings::widgets::row;
use crate::theme::{self, SwitchingTarget};

pub fn render_appearance_tab(state: &BridgeState, cx: &App) -> impl IntoElement {
    let is_dark = cx.theme().is_dark();
    let active_family = active(cx);
    let family_names = families(cx);
    let target = theme::switching_target(cx);
    let switching = !matches!(target, SwitchingTarget::None);

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .child(section(
            "Theme family",
            Some("Light and dark palettes. See Keyboard for the theme toggle shortcut.".into()),
            family_chips(&active_family, &family_names, &target, switching),
            cx,
        ))
        .child(section(
            "Mode",
            Some("Applies to editors and chrome.".into()),
            mode_chips(is_dark, &target, switching),
            cx,
        ))
        .child(render_typography(state, cx))
}

fn family_chips(
    active_name: &SharedString,
    names: &[SharedString],
    target: &SwitchingTarget,
    switching: bool,
) -> impl IntoElement {
    let mut row = div().flex().flex_wrap().gap_2();
    for name in names {
        let name = name.clone();
        let selected = name.as_ref() == active_name.as_ref();
        let pending = matches!(target, SwitchingTarget::Family(n) if n.as_ref() == name.as_ref());
        let label = name.clone();
        row = row.child(
            Button::new(SharedString::from(format!("family-{name}")))
                .variant(if selected {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Small)
                .label(label)
                // Do not also disable while loading — that cancels the mark.
                .loading(pending)
                .loading_style(ButtonLoadingStyle::GridShift)
                .disabled(switching && !pending)
                .on_click(move |_, _, cx| {
                    theme::select_family(name.clone(), cx);
                }),
        );
    }
    row
}

fn mode_chips(is_dark: bool, target: &SwitchingTarget, switching: bool) -> impl IntoElement {
    let light_pending = matches!(target, SwitchingTarget::Mode(ThemeMode::Light));
    let dark_pending = matches!(target, SwitchingTarget::Mode(ThemeMode::Dark));
    div()
        .flex()
        .flex_wrap()
        .gap_2()
        .child(
            Button::new("theme-light")
                .variant(if !is_dark {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Sun)
                .label("Light")
                .loading(light_pending)
                .loading_style(ButtonLoadingStyle::GridShift)
                .disabled(switching && !light_pending)
                .on_click(|_, _, cx| {
                    theme::change_mode(ThemeMode::Light, cx);
                }),
        )
        .child(
            Button::new("theme-dark")
                .variant(if is_dark {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Outline
                })
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Moon)
                .label("Dark")
                .loading(dark_pending)
                .loading_style(ButtonLoadingStyle::GridShift)
                .disabled(switching && !dark_pending)
                .on_click(|_, _, cx| {
                    theme::change_mode(ThemeMode::Dark, cx);
                }),
        )
}

/// UI face, mono face, and base size for the whole app.
///
/// The size is also the window rem size, so every `text_sm()` / `gap_2()` in
/// the chrome scales with it — not just the text. Coder can still override the
/// editor face under its own settings.
fn render_typography(state: &BridgeState, cx: &App) -> impl IntoElement {
    let size = state.appearance.font_size.unwrap_or(16);
    let family = state.appearance.font_family.clone();
    let mono = state.appearance.mono_font_family.clone();
    let has_override = family.is_some() || mono.is_some() || state.appearance.font_size.is_some();

    let body = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .child(row(
            "Font",
            "UI face for menus, panels, lists, and toolbars.",
            font_picker(
                "app-font-family",
                &family,
                DEFAULT_FONT_FAMILY,
                installed_ui_faces(cx, family.as_deref()),
                |name, cx| Bridge::set_app_font_family(name, cx),
            ),
            cx,
        ))
        .child(row(
            "Font mono",
            "Monospace face for diffs, logs, terminals, and code chrome.",
            font_picker(
                "app-mono-font-family",
                &mono,
                DEFAULT_MONO_FONT_FAMILY,
                installed_mono_faces(cx, mono.as_deref()),
                |name, cx| Bridge::set_app_mono_font_family(name, cx),
            ),
            cx,
        ))
        .child(row(
            "Font size",
            format!("{size} px — the base size the rest of the chrome scales from"),
            NumberStepper::new("app-font-size", i64::from(size))
                .range(11, 20)
                .on_change(|next, _window, cx| {
                    Bridge::set_app_font_size(*next as u16, cx);
                    apply_saved(cx);
                }),
            cx,
        ))
        .child(preview(&family, &mono, size, cx))
        .child(
            div().flex().items_center().gap_2().pt_1().child(
                Button::new("app-font-reset")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::ArrowCounterClockwise)
                    .label("Reset typography")
                    .disabled(!has_override)
                    .on_click(|_, _, cx| {
                        Bridge::reset_app_font(cx);
                        apply_saved(cx);
                    }),
            ),
        );

    section(
        "Typography",
        Some(
            "Two faces for the whole app: a UI font and a monospace font. Coder can still pick a separate editor face."
                .into(),
        ),
        body,
        cx,
    )
}

fn font_picker(
    id: &'static str,
    selected: &Option<String>,
    default_name: &'static str,
    faces: Vec<SharedString>,
    set: impl Fn(Option<String>, &mut App) + Clone + 'static,
) -> Button {
    let is_default = selected.is_none()
        || selected
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(default_name));
    let label: SharedString = if is_default {
        SharedString::from(default_name)
    } else {
        SharedString::from(selected.clone().unwrap_or_default())
    };
    let current = selected.clone();

    Button::new(id)
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Small)
        .label(label)
        .trailing_icon(PhosphorIcon::CaretDown)
        .dropdown_menu({
            let set_default = set.clone();
            move |menu: PopupMenu, _window, _cx: &mut Context<PopupMenu>| {
                let mut menu = menu
                    .scrollable(true)
                    .max_h(px(320.))
                    .item(
                        PopupMenuItem::new(format!("{default_name} (Default)"))
                            .checked(is_default)
                            .on_click({
                                let set_default = set_default.clone();
                                move |_, _, cx| {
                                    set_default(None, cx);
                                    apply_saved(cx);
                                }
                            }),
                    )
                    .separator();

                for face in faces.iter().cloned() {
                    if face.eq_ignore_ascii_case(default_name) {
                        continue;
                    }
                    let checked = current.as_deref() == Some(face.as_ref());
                    let name = face.to_string();
                    let set = set.clone();
                    menu = menu.item(PopupMenuItem::new(face).checked(checked).on_click(
                        move |_, _, cx| {
                            set(Some(name.clone()), cx);
                            apply_saved(cx);
                        },
                    ));
                }
                menu
            }
        })
}

/// Live sample of both faces at the chosen size.
fn preview(
    family: &Option<String>,
    mono: &Option<String>,
    size: u16,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let face: SharedString = family
        .clone()
        .map(SharedString::from)
        .unwrap_or_else(|| DEFAULT_FONT_FAMILY.into());
    let mono_face: SharedString = mono
        .clone()
        .map(SharedString::from)
        .unwrap_or_else(|| DEFAULT_MONO_FONT_FAMILY.into());

    div()
        .w_full()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted)
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .font_family(face.clone())
                .text_size(px(f32::from(size)))
                .text_color(theme.foreground)
                .child("Halo — intercept, inspect, replay"),
        )
        .child(
            div()
                .font_family(face)
                .text_size(px(f32::from(size)))
                .text_color(theme.muted_foreground)
                .child("The quick brown fox jumps over the lazy dog · 0123456789"),
        )
        .child(
            div()
                .font_family(mono_face.clone())
                .text_size(px(f32::from(size) - 1.))
                .text_color(theme.foreground)
                .child(format!("fn main() {{ println!(\"{mono_face}\"); }}")),
        )
}

//! Row and control helpers shared by the settings tabs.
//!
//! Each helper keeps the "label + description on the left, control on the
//! right" rhythm so a tab file only describes *what* is configurable.
//!
//! [`section`] is deliberately lighter than [`crate::pages::chrome::section`]:
//! it is a bare title + description, with no panel header or padding, so the
//! Coder / Agent tabs keep the tight rhythm they had as a standalone window.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::number_stepper::NumberStepper;
use celestia_ui::components::primitive::switch::Switch;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenu;

/// One titled block: header, optional description, then its rows.
///
/// `pb_8` is twice the `gap_4` inside a section, so the space *between* two
/// sections always outruns the space *within* one. Without that ratio a long
/// stack of sections reads as a single undifferentiated list.
pub(super) fn section(
    title: &'static str,
    description: &'static str,
    rows: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .pb_8()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::NORMAL).child(title))
                .child(div().text_xs().text_color(muted).child(description)),
        )
        .child(div().w_full().flex().flex_col().gap_4().child(rows))
}

/// Label + description on the left, control on the right.
pub(super) fn row(
    label: &'static str,
    description: impl Into<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().child(label))
                .child(div().text_xs().text_color(muted).child(description.into())),
        )
        .child(div().flex_none().child(control))
}

/// Vertical stack of rows.
pub(super) fn rows() -> Div {
    div().w_full().flex().flex_col().gap_4()
}

/// A controlled switch that reports the requested next value.
pub(super) fn switch(
    id: &'static str,
    checked: bool,
    on_change: impl Fn(bool, &mut App) + 'static,
) -> Switch {
    Switch::new(id)
        .checked(checked)
        .on_change(move |next, _window, cx| on_change(*next, cx))
}

/// A −/value/+ stepper clamped to `min..=max`.
pub(super) fn stepper(
    id: &'static str,
    value: i64,
    min: i64,
    max: i64,
    on_change: impl Fn(i64, &mut App) + 'static,
) -> NumberStepper {
    NumberStepper::new(id, value)
        .range(min, max)
        .on_change(move |next, _window, cx| on_change(*next, cx))
}

/// A selectable chip — the segmented-choice control used across the window.
pub(super) fn chip(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&mut App) + 'static,
) -> Button {
    Button::new(ElementId::Name(id.into()))
        .variant(if selected {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Outline
        })
        .size(ButtonSize::Small)
        .label(label)
        .on_click(move |_, _, cx| on_click(cx))
}

/// Outline trigger that opens a menu — used for long option lists (fonts).
pub(super) fn dropdown_select(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
) -> Button {
    Button::new(ElementId::Name(id.into()))
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Small)
        .label(label)
        .trailing_icon(PhosphorIcon::CaretDown)
        .dropdown_menu(build)
}

/// Small secondary action button.
pub(super) fn action(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&mut App) + 'static,
) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Secondary)
        .size(ButtonSize::Small)
        .label(label)
        .on_click(move |_, _, cx| on_click(cx))
}

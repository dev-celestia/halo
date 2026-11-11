//! Form-based request headers editor (key / value / enabled rows).

use std::collections::HashMap;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::checkbox::Checkbox;
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::{Input, InputState};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;

struct HeaderRow {
    id: u64,
    key: Entity<InputState>,
    value: Entity<InputState>,
    enabled: bool,
}

/// Live form state for the Repeater Headers tab.
pub struct HeadersForm {
    rows: Vec<HeaderRow>,
    next_id: u64,
}

impl HeadersForm {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            next_id: 1,
        }
    }

    /// Replace rows from a header map (insertion order is not preserved).
    pub fn set_from_map(
        &mut self,
        headers: &HashMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rows.clear();
        let mut entries: Vec<_> = headers.iter().collect();
        entries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        for (key, value) in entries {
            self.push_row(key.clone(), value.clone(), true, window, cx);
        }
        if self.rows.is_empty() {
            self.push_row(String::new(), String::new(), true, window, cx);
        }
        cx.notify();
    }

    /// Enabled rows with a non-empty key, ready for Send / Save.
    pub fn to_map(&self, cx: &App) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for row in &self.rows {
            if !row.enabled {
                continue;
            }
            let key = row.key.read(cx).value().to_string();
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            let value = row.value.read(cx).value().to_string();
            map.insert(key.to_string(), value);
        }
        map
    }

    pub fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.push_row(String::new(), String::new(), true, window, cx);
        cx.notify();
    }

    pub fn remove_row(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.rows.retain(|row| row.id != id);
        if self.rows.is_empty() {
            self.push_row(String::new(), String::new(), true, window, cx);
        }
        cx.notify();
    }

    pub fn set_enabled(&mut self, id: u64, enabled: bool, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
            row.enabled = enabled;
            cx.notify();
        }
    }

    fn push_row(
        &mut self,
        key: String,
        value: String,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let key_input = cx.new(|cx| InputState::new(window, cx).placeholder("Header name"));
        let value_input = cx.new(|cx| InputState::new(window, cx).placeholder("Value"));
        key_input.update(cx, |state, cx| {
            state.set_value(key, window, cx);
        });
        value_input.update(cx, |state, cx| {
            state.set_value(value, window, cx);
        });
        self.rows.push(HeaderRow {
            id,
            key: key_input,
            value: value_input,
            enabled,
        });
    }
}

impl Render for HeadersForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let rows: Vec<(u64, Entity<InputState>, Entity<InputState>, bool)> = self
            .rows
            .iter()
            .map(|row| (row.id, row.key.clone(), row.value.clone(), row.enabled))
            .collect();

        div()
            .id("headers-form")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .child(
                        div()
                            .w(px(28.))
                            .flex_none()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.muted_foreground)
                            .child(""),
                    )
                    .child(
                        div()
                            .w(px(180.))
                            .flex_none()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.muted_foreground)
                            .child("HEADER"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.muted_foreground)
                            .child("VALUE"),
                    )
                    .child(div().w(px(28.)).flex_none()),
            )
            .child(
                div()
                    .id("headers-rows")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .children(rows.into_iter().map(|(id, key, value, enabled)| {
                        let form = cx.entity().clone();
                        div()
                            .id(SharedString::from(format!("header-row-{id}")))
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(28.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Checkbox::new(SharedString::from(format!(
                                            "header-enabled-{id}"
                                        )))
                                        .checked(enabled)
                                        .on_click({
                                            let form = form.clone();
                                            move |next, _, cx| {
                                                let enabled = *next;
                                                form.update(cx, |form, cx| {
                                                    form.set_enabled(id, enabled, cx);
                                                });
                                            }
                                        }),
                                    ),
                            )
                            .child(div().w(px(180.)).flex_none().child(Input::new(&key)))
                            .child(div().flex_1().min_w_0().child(Input::new(&value)))
                            .child(
                                Button::icon(
                                    SharedString::from(format!("header-del-{id}")),
                                    Phosphor::new(PhosphorIcon::Trash).size(px(12.)),
                                )
                                .size(ButtonSize::IconSm)
                                .variant(ButtonVariant::Ghost)
                                .accessibility_label("Remove header")
                                .tooltip("Remove header")
                                .on_click({
                                    let form = form.clone();
                                    move |_, window, cx| {
                                        form.update(cx, |form, cx| {
                                            form.remove_row(id, window, cx);
                                        });
                                    }
                                }),
                            )
                    })),
            )
            .child(
                div().w_full().flex().justify_start().child(
                    Button::new("headers-add")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Plus)
                        .label("Add header")
                        .on_click({
                            let form = cx.entity().clone();
                            move |_, window, cx| {
                                form.update(cx, |form, cx| {
                                    form.add_row(window, cx);
                                });
                            }
                        }),
                ),
            )
    }
}

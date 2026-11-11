//! Form-based environment variable editor (key / value / enabled rows).

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::checkbox::Checkbox;
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::{Input, InputState};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use serde_json::{Map, Value};

struct EnvVarRow {
    id: u64,
    key: Entity<InputState>,
    value: Entity<InputState>,
    enabled: bool,
}

/// Live form state for the environment variables modal.
pub struct EnvVarsForm {
    rows: Vec<EnvVarRow>,
    next_id: u64,
}

impl EnvVarsForm {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            next_id: 1,
        }
    }

    /// Replace rows from the stored JSON (flat or Postman-style nested).
    pub fn set_from_json(&mut self, json: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.rows.clear();
        for (key, value, enabled) in parse_env_entries(json) {
            self.push_row(key, value, enabled, window, cx);
        }
        if self.rows.is_empty() {
            self.push_row(String::new(), String::new(), true, window, cx);
        }
        cx.notify();
    }

    /// Serialize current rows to Postman-style nested JSON.
    pub fn to_json(&self, cx: &App) -> String {
        let mut map = Map::new();
        for row in &self.rows {
            let key = row.key.read(cx).value().to_string();
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            let value = row.value.read(cx).value().to_string();
            map.insert(
                key.to_string(),
                Value::Object({
                    let mut inner = Map::new();
                    inner.insert("value".into(), Value::String(value));
                    inner.insert("enabled".into(), Value::Bool(row.enabled));
                    inner
                }),
            );
        }
        Value::Object(map).to_string()
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
        let key_input = cx.new(|cx| InputState::new(window, cx).placeholder("Variable name"));
        let value_input = cx.new(|cx| InputState::new(window, cx).placeholder("Value"));
        key_input.update(cx, |state, cx| {
            state.set_value(key, window, cx);
        });
        value_input.update(cx, |state, cx| {
            state.set_value(value, window, cx);
        });
        self.rows.push(EnvVarRow {
            id,
            key: key_input,
            value: value_input,
            enabled,
        });
    }
}

impl Render for EnvVarsForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let rows: Vec<(u64, Entity<InputState>, Entity<InputState>, bool)> = self
            .rows
            .iter()
            .map(|row| (row.id, row.key.clone(), row.value.clone(), row.enabled))
            .collect();

        div()
            .id("env-vars-form")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
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
                            .w(px(160.))
                            .flex_none()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.muted_foreground)
                            .child("VARIABLE"),
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
                    .id("env-vars-rows")
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
                            .id(SharedString::from(format!("env-var-row-{id}")))
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
                                            "env-var-enabled-{id}"
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
                            .child(div().w(px(160.)).flex_none().child(Input::new(&key)))
                            .child(div().flex_1().min_w_0().child(Input::new(&value)))
                            .child(
                                Button::icon(
                                    SharedString::from(format!("env-var-del-{id}")),
                                    Phosphor::new(PhosphorIcon::Trash).size(px(12.)),
                                )
                                .size(ButtonSize::IconSm)
                                .variant(ButtonVariant::Ghost)
                                .accessibility_label("Remove variable")
                                .tooltip("Remove variable")
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
                    Button::new("env-vars-add")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Plus)
                        .label("Add variable")
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

fn parse_env_entries(json: &str) -> Vec<(String, String, bool)> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (key, val) in obj {
        match val {
            Value::String(s) => out.push((key.clone(), s.clone(), true)),
            Value::Number(n) => out.push((key.clone(), n.to_string(), true)),
            Value::Bool(b) => out.push((key.clone(), b.to_string(), true)),
            Value::Object(inner) => {
                let enabled = inner
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let value = inner
                    .get("value")
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => String::new(),
                    })
                    .unwrap_or_default();
                out.push((key.clone(), value, enabled));
            }
            _ => {}
        }
    }
    out
}

//! Keyboard — mapped application and window shortcuts.

use celestia_ui::components::primitive::kbd::Kbd;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::keymap::{HotkeyEntry, hotkey_catalog};
use crate::pages::chrome::section;

pub fn render_keyboard_tab(cx: &App) -> impl IntoElement {
    let mut root = div().w_full().flex().flex_col().gap_4();

    for section_def in hotkey_catalog() {
        root = root.child(section(
            section_def.title,
            None,
            shortcut_list(&section_def.entries, cx),
            cx,
        ));
    }

    root
}

fn shortcut_list(entries: &[HotkeyEntry], cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;

    let mut list = div().w_full().flex().flex_col();

    for (i, entry) in entries.iter().enumerate() {
        let mut chords = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1p5()
            .flex_shrink_0();
        for chord in &entry.chords {
            match Keystroke::parse(chord) {
                Ok(stroke) => {
                    chords = chords.child(Kbd::new(stroke).outline());
                }
                Err(_) => {
                    chords = chords.child(div().text_xs().text_color(muted).child(chord.clone()));
                }
            }
        }

        list = list.child(
            div()
                .id(SharedString::from(format!(
                    "hotkey-{}",
                    entry.label.to_lowercase().replace(' ', "-")
                )))
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .py_2p5()
                .when(i + 1 < entries.len(), |row| {
                    row.border_b_1().border_color(border.opacity(0.6))
                })
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::NORMAL)
                                .child(entry.label),
                        )
                        .child(div().text_xs().text_color(muted).child(entry.description)),
                )
                .child(chords),
        );
    }

    list
}

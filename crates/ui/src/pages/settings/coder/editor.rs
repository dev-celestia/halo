//! Editor section — typography, view chrome, and save behaviour.
//!
//! Values live on shared [`BridgeState::coder_settings`]. The open Coder window
//! re-applies them to its live `CodeEditor` widgets on the next render.

use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, BridgeState, CoderLineEnding, font_label, font_options};

use crate::pages::settings::widgets::{chip, dropdown_select, row, rows, section, stepper, switch};

pub(in crate::pages::settings) fn render(state: &BridgeState, cx: &App) -> impl IntoElement {
    let settings = state.coder_settings.clone();
    let selected_font_id = settings.font_id.clone();

    let font_select = dropdown_select(
        "coder-font-select",
        font_label(&settings.font_id),
        move |menu, _, _| {
            let mut menu = menu;
            for font in font_options() {
                let id = font.id.clone();
                let checked = id == selected_font_id;
                menu = menu.item(
                    PopupMenuItem::new(font.label.clone())
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            let id = id.clone();
                            Bridge::update_coder_settings(cx, move |s| s.font_id = id);
                        }),
                );
            }
            menu
        },
    );

    let mut ending_chips = div().flex().flex_wrap().gap_2();
    for ending in CoderLineEnding::ALL {
        ending_chips = ending_chips.child(chip(
            SharedString::from(format!("coder-line-ending-{}", ending.id())),
            ending.label(),
            settings.line_ending == ending,
            move |cx| {
                Bridge::update_coder_settings(cx, move |s| s.line_ending = ending);
            },
        ));
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section(
            "Typography",
            "Typeface, size, and indentation for Coder editor tabs and the Explorer.",
            rows()
                .child(row(
                    "Font",
                    "Applies to the open Coder editor and the file Explorer. Bundled faces ship with the app; Menlo / SF Mono / Monaco / Consolas use the OS when present.",
                    font_select,
                    cx,
                ))
                .child(row(
                    "Font size",
                    format!("{} px — applies to the editor and Explorer", settings.font_size),
                    stepper(
                        "coder-font-size",
                        i64::from(settings.font_size),
                        10,
                        24,
                        |value, cx| {
                            Bridge::update_coder_settings(cx, move |s| s.font_size = value as u16);
                        },
                    ),
                    cx,
                ))
                .child(row(
                    "Tab size",
                    format!("{} spaces per indent level", settings.tab_size),
                    stepper(
                        "coder-tab-size",
                        i64::from(settings.tab_size),
                        1,
                        8,
                        |value, cx| {
                            Bridge::update_coder_settings(cx, move |s| s.tab_size = value as u8);
                        },
                    ),
                    cx,
                ))
                .child(row(
                    "Word wrap",
                    "Wrap long lines instead of scrolling horizontally. Toggle with Alt+Z (or ⌘⌥Z / Ctrl+Alt+Z).",
                    switch("coder-word-wrap", settings.word_wrap, |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| s.word_wrap = next);
                    }),
                    cx,
                )),
            cx,
        ))
        .child(section(
            "View",
            "Chrome drawn around the buffer.",
            rows()
                .child(row(
                    "Minimap",
                    "Overview strip on the right edge of the editor.",
                    switch("coder-minimap", settings.minimap, |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| s.minimap = next);
                    }),
                    cx,
                ))
                .child(row(
                    "Line numbers",
                    "Show the gutter with absolute line numbers.",
                    switch("coder-line-numbers", settings.line_numbers, |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| s.line_numbers = next);
                    }),
                    cx,
                )),
            cx,
        ))
        .child(section(
            "Save behaviour",
            "What happens when a buffer is written back to disk.",
            rows()
                .child(row(
                    "Autosave",
                    "Write the buffer shortly after you stop typing.",
                    switch("coder-autosave", settings.autosave, |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| s.autosave = next);
                    }),
                    cx,
                ))
                .child(row(
                    "Format on save",
                    "Run the file's formatter before writing, when one is configured.",
                    switch("coder-format-on-save", settings.format_on_save, |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| s.format_on_save = next);
                    }),
                    cx,
                ))
                .child(row(
                    "Trim trailing whitespace",
                    "Strip trailing spaces and tabs on every save.",
                    switch(
                        "coder-trim-whitespace",
                        settings.trim_trailing_whitespace,
                        |next, cx| {
                            Bridge::update_coder_settings(cx, move |s| {
                                s.trim_trailing_whitespace = next;
                            });
                        },
                    ),
                    cx,
                ))
                .child(row(
                    "Line endings",
                    "Written for new files and normalised on save.",
                    ending_chips,
                    cx,
                )),
            cx,
        ))
}

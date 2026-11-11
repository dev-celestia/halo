//! Terminal section — shell, scrollback, and dock behaviour.

use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::editors::ToolEditors;

use crate::pages::settings::widgets::{action, chip, row, rows, section, stepper, switch};

const SCROLLBACK_CHOICES: [u32; 4] = [1_000, 5_000, 10_000, 25_000];

pub(in crate::pages::settings) fn render(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let settings = state.coder_settings.clone();

    let mut scrollback_chips = div().flex().flex_wrap().gap_2();
    for lines in SCROLLBACK_CHOICES {
        scrollback_chips = scrollback_chips.child(chip(
            SharedString::from(format!("coder-scrollback-{lines}")),
            format!("{lines}"),
            settings.scrollback_lines == lines,
            move |cx| {
                Bridge::update_coder_settings(cx, move |s| s.scrollback_lines = lines);
            },
        ));
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section(
            "Shell",
            "Program launched for every new terminal tab.",
            shell_block(editors, cx),
            cx,
        ))
        .child(section(
            "Starting directory",
            "Where new terminals open. The open Coder folder still wins when one is set.",
            cwd_block(state, editors, cx),
            cx,
        ))
        .child(section(
            "Display",
            "How much history the dock keeps and how large it renders.",
            rows()
                .child(row(
                    "Font size",
                    format!("{} px in the terminal dock", settings.terminal_font_size),
                    stepper(
                        "coder-terminal-font-size",
                        i64::from(settings.terminal_font_size),
                        9,
                        20,
                        |value, cx| {
                            Bridge::update_coder_settings(cx, move |s| {
                                s.terminal_font_size = value as u16;
                            });
                        },
                    ),
                    cx,
                ))
                .child(row(
                    "Scrollback",
                    "Lines retained per terminal tab.",
                    scrollback_chips,
                    cx,
                )),
            cx,
        ))
        .child(section(
            "Behaviour",
            "What the dock does when a process is still running.",
            rows().child(row(
                "Confirm before closing",
                "Ask before closing a tab whose process has not exited.",
                switch(
                    "coder-confirm-close-running",
                    settings.confirm_close_running,
                    |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| s.confirm_close_running = next);
                    },
                ),
                cx,
            )),
            cx,
        ))
}

/// Default working directory: a path, so it gets the same full-width field as
/// the shell. Blank means "project folder, else `$HOME`".
fn cwd_block(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let input = editors.coder_settings_default_cwd.clone();
    let current = state
        .coder_settings
        .effective_cwd()
        .map(str::to_string)
        .unwrap_or_else(|| "$HOME".to_string());

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(div().text_sm().child("Default directory"))
                .child(div().text_xs().text_color(muted).child(format!(
                    "Blank falls back to the open folder, then $HOME. Starting in {current}."
                ))),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&editors.coder_settings_default_cwd)),
                )
                .child(action("coder-default-cwd-apply", "Apply", move |cx| {
                    let dir = input.read(cx).value().to_string();
                    Bridge::set_terminal_default_cwd(&dir, cx);
                })),
        )
}

/// Shell path gets a full-width field — a chip row cannot express `$SHELL`
/// fallbacks or an absolute path.
fn shell_block(editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let input = editors.coder_settings_shell.clone();

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(div().text_sm().child("Shell path"))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Absolute path, or blank to inherit the login shell ($SHELL)."),
                ),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&editors.coder_settings_shell)),
                )
                .child(action("coder-shell-apply", "Apply", move |cx| {
                    let shell = input.read(cx).value().to_string();
                    Bridge::set_coder_shell(&shell, cx);
                })),
        )
}

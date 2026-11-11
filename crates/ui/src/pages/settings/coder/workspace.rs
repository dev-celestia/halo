//! Workspace section — startup, explorer visibility, and ignore rules.

use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::editors::ToolEditors;

use crate::pages::settings::widgets::{action, row, rows, section, switch};

pub(in crate::pages::settings) fn render(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let settings = state.coder_settings.clone();
    let root = state.coder.root_path.clone();

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section(
            "Startup",
            "What the IDE reopens for you on launch.",
            rows().child(row(
                "Restore last project",
                "Reopen the most recent folder and its editor tabs.",
                switch(
                    "coder-restore-last-project",
                    settings.restore_last_project,
                    |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| s.restore_last_project = next);
                    },
                ),
                cx,
            )),
            cx,
        ))
        .child(section(
            "Explorer",
            "Which entries the file tree shows.",
            rows()
                .child(row(
                    "Show hidden files",
                    "Include dotfiles such as .gitignore and .env in the tree.",
                    switch(
                        "coder-show-hidden-files",
                        settings.show_hidden_files,
                        |next, cx| {
                            Bridge::update_coder_settings(cx, move |s| s.show_hidden_files = next);
                        },
                    ),
                    cx,
                ))
                .child(ignore_block(editors, &settings.ignore_globs_summary(), cx)),
            cx,
        ))
        .child(section(
            "Current project",
            "The folder this window's editors and terminal are rooted in.",
            project_block(root.as_deref(), cx),
            cx,
        ))
}

/// Ignore globs are newline-separated in state but edited comma-separated —
/// a single-line field is the only shape `InputState` renders well.
fn ignore_block(editors: &ToolEditors, summary: &str, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let input = editors.coder_settings_ignore_globs.clone();

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
                .child(div().text_sm().child("Ignore rules"))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("Comma separated. Currently: {summary}")),
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
                        .child(Input::new(&editors.coder_settings_ignore_globs)),
                )
                .child(action("coder-ignore-apply", "Apply", move |cx| {
                    let globs = input.read(cx).value().to_string();
                    Bridge::set_coder_ignore_globs(&globs, cx);
                })),
        )
}

fn project_block(root: Option<&str>, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let has_root = root.is_some();
    let root = root.unwrap_or("No folder open").to_string();

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
                .gap_0p5()
                .child(div().text_sm().truncate().child(root))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Change it from the Projects menu in the title bar."),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(action("coder-open-folder", "Open folder…", |cx| {
                    let _ = Bridge::coder_open_folder(cx);
                }))
                .when(has_root, |this| {
                    this.child(action("coder-close-project", "Close project", |cx| {
                        let _ = Bridge::coder_close_project(cx);
                    }))
                }),
        )
}

//! Bottom status strip for the Coder page.

use std::path::Path;

use celestia_ui::components::Tooltip;
use celestia_ui::components::composite::status_bar::StatusBar;
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, SettingsTab};

use super::lang;
use super::run_panel::sync_run_panel_inputs;
use crate::pages::editors::ToolEditors;
use crate::pages::source_control::{render_branch_chip, render_changes_chip, render_project_chip};

fn language_label_for_path(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "Rust",
        "md" | "markdown" => "Markdown",
        "js" | "mjs" | "cjs" => "JavaScript",
        "jsx" => "JavaScript React",
        "ts" => "TypeScript",
        "tsx" => "TypeScript React",
        "json" => "JSON",
        "toml" => "TOML",
        "yaml" | "yml" => "YAML",
        "html" | "htm" => "HTML",
        "css" | "scss" => "CSS",
        "py" | "pyi" | "pyw" => "Python",
        "go" => "Go",
        "sh" | "bash" | "zsh" | "ksh" | "fish" => "Shell",
        "http" => "HTTP",
        "txt" => "Plain Text",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "avif" | "tif" | "tiff"
        | "svg" => "Image",
        _ => "Plain Text",
    }
}

/// Padding shared by status chips. No hover: this frame is not a control.
fn status_frame(cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius_sm())
}

fn status_chip(label: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    status_frame(cx).child(label.into())
}

fn status_chip_with_icon(
    icon: PhosphorIcon,
    label: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    status_frame(cx)
        .child(Phosphor::new(icon).size(px(12.)))
        .child(label.into())
}

fn hint(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |window, cx| Tooltip::new(text.clone()).build(window, cx)
}

/// Shorten `"Saved /abs/path/file.tsx"` → `"Saved file.tsx"` for the footer.
fn saved_status_label(message: &str) -> Option<String> {
    let path = message.strip_prefix("Saved ")?;
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    Some(format!("Saved {name}"))
}

pub(super) fn render_coder_status_bar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let active_path = state.coder.focused_active_path().map(str::to_string);
    let language = active_path
        .as_deref()
        .map(language_label_for_path)
        .unwrap_or("Plain Text");
    let language_mark = active_path
        .as_deref()
        .and_then(|p| Path::new(p).file_name().and_then(|n| n.to_str()))
        .unwrap_or("file.txt")
        .to_string();
    let file_name = active_path
        .as_deref()
        .and_then(|p| Path::new(p).file_name()?.to_str())
        .unwrap_or("—")
        .to_string();
    let term_count = state.coder.terminals.len();
    let term_alive = state
        .coder
        .terminals
        .iter()
        .filter(|t| state.terminal_sessions.alive(&t.id))
        .count();
    let lsp_label = state.coder.lsp.label();
    let saved_label = state
        .coder
        .status_message
        .as_deref()
        .and_then(saved_status_label);

    let show_run = state.coder.run_active || Bridge::coder_run_command_preview(cx).is_some();
    let mut bar = StatusBar::new()
        .w_full()
        .flex_none()
        .left(render_branch_chip(state, cx))
        .left(render_project_chip(state, cx))
        .left(render_changes_chip(state, cx));
    if show_run {
        bar = bar.left(render_run_chip(state, cx));
    }
    bar.left(render_build_chip(state, editors, cx))
        .child(if let Some(label) = saved_label {
            let ok = cx.theme().success;
            div()
                .id("coder-status-saved")
                .flex()
                .items_center()
                .gap_1()
                .px_1p5()
                .py_0p5()
                .rounded(cx.theme().radius_sm())
                .text_color(ok)
                .child(
                    Phosphor::new(PhosphorIcon::CheckCircle)
                        .size(px(12.))
                        .color(ok),
                )
                .child(label)
                .into_any_element()
        } else {
            div().into_any_element()
        })
        .right(
            div()
                .id("coder-status-settings")
                .cursor_pointer()
                .hover(|s| s.bg(cx.theme().accent.opacity(0.5)))
                .rounded(cx.theme().radius_sm())
                .on_click(|_, _, cx| {
                    crate::windows::open_settings_window(SettingsTab::CoderEditor, cx);
                })
                .child(status_chip_with_icon(PhosphorIcon::Gear, "settings", cx)),
        )
        .right(status_chip("UTF-8", cx))
        .right(status_chip("LF", cx))
        .right(
            status_frame(cx)
                .id("coder-status-lang")
                .child(lang::file_mark(&language_mark, px(14.)))
                .child(language),
        )
        .right(div().id("coder-status-lsp").child(status_chip_with_icon(
            PhosphorIcon::BracketsCurly,
            lsp_label,
            cx,
        )))
        .right(div().id("coder-status-file").child(status_chip_with_icon(
            PhosphorIcon::File,
            file_name,
            cx,
        )))
        .right(
            div()
                .id("coder-status-terminals")
                .cursor_pointer()
                .hover(|s| s.bg(cx.theme().accent.opacity(0.5)))
                .rounded(cx.theme().radius_sm())
                .on_click(|_, _, cx| {
                    if !Bridge::state(cx).read(cx).coder.terminal_open {
                        let _ = Bridge::coder_toggle_terminal(cx);
                    }
                })
                .child(status_chip_with_icon(
                    PhosphorIcon::TerminalWindow,
                    if term_count == 0 {
                        "Terminal".into()
                    } else {
                        format!("{term_alive}/{term_count}")
                    },
                    cx,
                )),
        )
}

/// Play runs the active Cargo or package command. Stop replaces it while a run is active.
fn render_run_chip(state: &BridgeState, cx: &App) -> impl IntoElement {
    let running = state.coder.run_active;
    let preview = Bridge::coder_run_command_preview(cx);
    let theme = cx.theme();
    let hover_bg = theme.accent.opacity(0.5);

    if running {
        let tip: SharedString = preview.unwrap_or_else(|| "Stop".to_string()).into();
        let danger = theme.danger;
        return status_frame(cx)
            .id("coder-status-run-stop")
            .cursor_pointer()
            .text_color(danger)
            .hover(move |s| s.bg(hover_bg))
            .tooltip(hint(tip))
            .on_click(|_, _, cx| {
                let _ = Bridge::coder_stop_run(cx);
            })
            .child(Phosphor::new(PhosphorIcon::Stop).size(px(12.)).color(danger))
            .child("Stop")
            .into_any_element();
    }

    let Some(command) = preview else {
        return div().into_any_element();
    };
    let ok = theme.success;
    status_frame(cx)
        .id("coder-status-run")
        .cursor_pointer()
        .text_color(ok)
        .hover(move |s| s.bg(hover_bg))
        .tooltip(hint(command))
        .on_click(|_, _, cx| {
            let _ = Bridge::coder_run_project(cx);
        })
        .child(Phosphor::new(PhosphorIcon::Play).size(px(12.)).color(ok))
        .child("Run")
        .into_any_element()
}

/// Hammer toggles the Run/Build dock. The command stays in the tooltip.
fn render_build_chip(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let panel_open = state.coder.run_panel_open;
    let theme = cx.theme();
    let hover_bg = theme.accent.opacity(0.5);
    let open_bg = theme.accent.opacity(0.5);
    let tip: SharedString = match Bridge::coder_build_command_preview(cx) {
        Some(command) => command.into(),
        None => "No build command for this project".into(),
    };
    let editors = editors.clone();
    status_frame(cx)
        .id("coder-status-build")
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .when(panel_open, move |d| d.bg(open_bg))
        .tooltip(hint(tip))
        .on_click(move |_, window, cx| {
            let opening = !Bridge::state(cx).read(cx).coder.run_panel_open;
            if opening {
                sync_run_panel_inputs(&editors, window, cx);
                let _ = Bridge::coder_set_run_panel_open(true, cx);
            } else {
                let _ = Bridge::coder_set_run_panel_open(false, cx);
            }
        })
        .child(Phosphor::new(PhosphorIcon::Hammer).size(px(12.)))
        .child("Build")
}

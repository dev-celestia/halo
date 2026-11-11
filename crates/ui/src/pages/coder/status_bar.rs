//! Bottom status strip for the Coder page.

use std::path::Path;

use celestia_ui::components::composite::status_bar::StatusBar;
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, SettingsTab};

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

fn status_chip(label: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius_sm())
        .hover(|s| s.bg(cx.theme().accent.opacity(0.5)))
        .child(label.into())
}

fn status_chip_with_icon(
    icon: PhosphorIcon,
    label: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius_sm())
        .hover(|s| s.bg(cx.theme().accent.opacity(0.5)))
        .child(Phosphor::new(icon).size(px(12.)))
        .child(label.into())
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

    StatusBar::new()
        .w_full()
        .flex_none()
        .left(render_branch_chip(state, cx))
        .left(render_project_chip(state, cx))
        .left(render_changes_chip(state, cx))
        .left(render_build_chip(state, editors, cx))
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
                .on_click(|_, _, cx| {
                    crate::windows::open_settings_window(SettingsTab::CoderEditor, cx);
                })
                .child(status_chip_with_icon(PhosphorIcon::Gear, "settings", cx)),
        )
        .right(status_chip("UTF-8", cx))
        .right(status_chip("LF", cx))
        .right(status_chip(language, cx))
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

/// Hammer toggles the Run/Build dock; Stop interrupts an active run.
fn render_build_chip(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let running = state.coder.run_active;
    let panel_open = state.coder.run_panel_open;
    let theme = cx.theme();

    if running {
        return div()
            .id("coder-status-build-stop")
            .cursor_pointer()
            .on_click(|_, _, cx| {
                let _ = Bridge::coder_stop_run(cx);
            })
            .child(status_chip_with_icon(PhosphorIcon::Stop, "Stop", cx))
            .into_any_element();
    }

    let run_preview = Bridge::coder_run_command_preview(cx);
    let label: SharedString = if panel_open {
        "Build".into()
    } else {
        match &run_preview {
            Some(c) => format!("Build ({c})").into(),
            None => "Build".into(),
        }
    };
    let editors = editors.clone();
    div()
        .id("coder-status-build")
        .cursor_pointer()
        .rounded(theme.radius_sm())
        .when(panel_open, |d| d.bg(theme.accent.opacity(0.5)))
        .on_click(move |_, window, cx| {
            let opening = !Bridge::state(cx).read(cx).coder.run_panel_open;
            if opening {
                sync_run_panel_inputs(&editors, window, cx);
                let _ = Bridge::coder_set_run_panel_open(true, cx);
            } else {
                let _ = Bridge::coder_set_run_panel_open(false, cx);
            }
        })
        .child(status_chip_with_icon(PhosphorIcon::Hammer, label, cx))
        .into_any_element()
}

//! Right dock: Run / Build actions and script customization.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon, PhosphorWeight};
use celestia_ui::components::primitive::input::Input;
use celestia_ui::components::traits::Selectable as _;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, CoderProjectKind};

use crate::pages::editors::ToolEditors;

const BODY_PAD: f32 = 12.;

pub(super) fn sync_run_panel_inputs(editors: &ToolEditors, window: &mut Window, cx: &mut App) {
    let run = Bridge::coder_run_command_preview(cx).unwrap_or_default();
    let build = Bridge::coder_build_command_preview(cx).unwrap_or_default();
    editors.coder_run_cmd_input.update(cx, |s, cx| {
        s.set_value(run, window, cx);
    });
    editors.coder_build_cmd_input.update(cx, |s, cx| {
        s.set_value(build, window, cx);
    });
}

pub(super) fn render_run_panel(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let run_cmd = Bridge::coder_run_command_preview(cx);
    let build_cmd = Bridge::coder_build_command_preview(cx);
    let scripts = Bridge::coder_list_node_scripts(cx);
    let kind = state.coder.project_kind;
    let has_node = kind.is_some_and(|k| k.has_node());
    let has_rust = kind.is_some_and(|k| k.has_rust());
    let run_auto = state.coder.run_command_override.is_none();
    let build_auto = state.coder.build_command_override.is_none();

    div()
        .id("coder-run-panel")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.background)
        .child(panel_header(cx))
        .child(
            div()
                .id("coder-run-panel-body")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap(px(16.))
                .p(px(BODY_PAD))
                .child(actions_block(run_cmd.as_deref(), build_cmd.as_deref(), cx))
                .child(scripts_block(
                    editors,
                    run_cmd.as_deref(),
                    build_cmd.as_deref(),
                    &scripts,
                    has_node,
                    has_rust,
                    run_auto,
                    build_auto,
                    kind,
                    cx,
                )),
        )
}

fn panel_header(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex_none()
        .px_2()
        .py_1()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme.border)
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child("RUN / BUILD"),
        )
        .child(
            Button::new("coder-run-panel-close")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Icon)
                .leading_icon(PhosphorIcon::X)
                .tooltip("Close")
                .on_click(|_, _, cx| {
                    let _ = Bridge::coder_set_run_panel_open(false, cx);
                }),
        )
}

fn actions_block(run_cmd: Option<&str>, build_cmd: Option<&str>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let run_tip: SharedString = match run_cmd {
        Some(c) => format!("Run ({c})").into(),
        None => "Open a Rust (Cargo.toml) or Node (package.json) project".into(),
    };
    let build_tip: SharedString = match build_cmd {
        Some(c) => format!("Build ({c})").into(),
        None => "No build command for this project".into(),
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div().w_full().child(
                Button::new("coder-run-panel-run")
                    .variant(ButtonVariant::Success)
                    .label("Run")
                    .tooltip(run_tip)
                    .disabled(run_cmd.is_none())
                    .leading_icon(
                        Phosphor::new(PhosphorIcon::Play)
                            .weight(PhosphorWeight::Fill)
                            .size(px(16.))
                            .color(theme.success_foreground),
                    )
                    .on_click(|_, _, cx| {
                        let _ = Bridge::coder_run_project(cx);
                    }),
            ),
        )
        .child(
            div().w_full().child(
                Button::new("coder-run-panel-build")
                    .variant(ButtonVariant::Outline)
                    .label("Build")
                    .tooltip(build_tip)
                    .disabled(build_cmd.is_none())
                    .leading_icon(PhosphorIcon::Hammer)
                    .on_click(|_, _, cx| {
                        let _ = Bridge::coder_build_project(cx);
                    }),
            ),
        )
}

fn scripts_block(
    editors: &ToolEditors,
    run_cmd: Option<&str>,
    build_cmd: Option<&str>,
    scripts: &[String],
    has_node: bool,
    has_rust: bool,
    run_auto: bool,
    build_auto: bool,
    kind: Option<CoderProjectKind>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let show_rust =
        has_rust && matches!(kind, Some(CoderProjectKind::Rust | CoderProjectKind::Both));

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child("Scripts"),
        )
        .child(command_section(
            "Run command",
            "coder-run-cmd",
            &editors.coder_run_cmd_input,
            run_cmd,
            scripts,
            has_node,
            show_rust,
            run_auto,
            "cargo run",
            |script, cx| {
                let _ = Bridge::coder_pick_run_script(script, cx);
            },
            |cx| {
                let _ = Bridge::coder_reset_run_command(cx);
            },
            |text, cx| {
                let _ = Bridge::coder_set_run_command_override(Some(text), cx);
            },
            cx,
        ))
        .child(command_section(
            "Build command",
            "coder-build-cmd",
            &editors.coder_build_cmd_input,
            build_cmd,
            scripts,
            has_node,
            show_rust,
            build_auto,
            "cargo build",
            |script, cx| {
                let _ = Bridge::coder_pick_build_script(script, cx);
            },
            |cx| {
                let _ = Bridge::coder_reset_build_command(cx);
            },
            |text, cx| {
                let _ = Bridge::coder_set_build_command_override(Some(text), cx);
            },
            cx,
        ))
}

fn command_section(
    title: &'static str,
    id_prefix: &'static str,
    input: &Entity<celestia_ui::components::primitive::input::InputState>,
    current_cmd: Option<&str>,
    scripts: &[String],
    show_node_scripts: bool,
    show_rust_preset: bool,
    is_auto: bool,
    rust_preset: &'static str,
    on_pick_script: impl Fn(&str, &mut App) + Clone + 'static,
    on_reset: impl Fn(&mut App) + Clone + 'static,
    on_apply_custom: impl Fn(String, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let current = current_cmd.unwrap_or("");
    let selected_script = scripts
        .iter()
        .find(|s| command_ends_with_script(current, s))
        .cloned();
    let rust_selected = !is_auto && current == rust_preset;

    let input_entity = input.clone();
    let apply_custom = on_apply_custom.clone();
    let is_run_section = id_prefix.contains("run");

    let sync_input = {
        let input_entity = input_entity.clone();
        move |window: &mut Window, cx: &mut App| {
            let cmd = if is_run_section {
                Bridge::coder_run_command_preview(cx).unwrap_or_default()
            } else {
                Bridge::coder_build_command_preview(cx).unwrap_or_default()
            };
            input_entity.update(cx, |s, cx| {
                s.set_value(cmd, window, cx);
            });
        }
    };

    let mut chips = div()
        .id(SharedString::from(format!("{id_prefix}-chips")))
        .w_full()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.))
        .child({
            let reset = on_reset.clone();
            let sync_input = sync_input.clone();
            chip(
                format!("{id_prefix}-auto"),
                "Auto",
                is_auto,
                move |_, window, cx| {
                    reset(cx);
                    sync_input(window, cx);
                },
            )
        });

    if show_rust_preset {
        let on_apply = on_apply_custom.clone();
        let sync_input = sync_input.clone();
        chips = chips.child(chip(
            format!("{id_prefix}-rust"),
            rust_preset,
            rust_selected,
            move |_, window, cx| {
                on_apply(rust_preset.to_string(), cx);
                sync_input(window, cx);
            },
        ));
    }

    if show_node_scripts {
        for script in scripts {
            let selected = !is_auto && selected_script.as_deref() == Some(script.as_str());
            let name = script.clone();
            let pick = on_pick_script.clone();
            let sync_input = sync_input.clone();
            chips = chips.child(chip(
                format!("{id_prefix}-script-{script}"),
                script.clone(),
                selected,
                move |_, window, cx| {
                    pick(&name, cx);
                    sync_input(window, cx);
                },
            ));
        }
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(div().text_sm().font_weight(FontWeight::NORMAL).child(title))
        .child(chips)
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(input).cleanable(true)),
                )
                .child({
                    let sync_input = sync_input.clone();
                    Button::new(format!("{id_prefix}-apply"))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("Set")
                        .tooltip("Use this custom command")
                        .on_click(move |_, window, cx| {
                            let text = input_entity.read(cx).value().to_string();
                            apply_custom(text, cx);
                            sync_input(window, cx);
                        })
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Clear the field and Set to restore Auto."),
        )
}

fn chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .variant(if selected {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Outline
        })
        .size(ButtonSize::Small)
        .label(label)
        .selected(selected)
        .on_click(on_click)
}

fn command_ends_with_script(cmd: &str, script: &str) -> bool {
    let cmd = cmd.trim();
    if cmd.ends_with(&format!(" run {script}")) {
        return true;
    }
    cmd.starts_with("yarn ") && cmd == format!("yarn {script}")
}

//! Coder workspace shell — resizable pane layout (explorer / editor / run).

use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::BridgeState;

use super::editor_pane::render_editor_pane;
use super::run_panel::render_run_panel;
use super::sidebar::render_sidebar_panel;
use crate::pages::editors::ToolEditors;

const SIDEBAR_WIDTH: Pixels = px(260.);
const SIDEBAR_MIN: Pixels = px(160.);
const SIDEBAR_MAX: Pixels = px(480.);

const RUN_PANEL_WIDTH: Pixels = px(300.);
const RUN_PANEL_MIN: Pixels = px(240.);
const RUN_PANEL_MAX: Pixels = px(520.);

const TERMINAL_HEIGHT: Pixels = px(200.);
const TERMINAL_MIN: Pixels = px(80.);
const TERMINAL_MAX: Pixels = px(520.);

pub(super) fn render_workspace(
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let sidebar_open = state.coder.sidebar_open;
    let terminal_open = state.coder.terminal_open;
    let run_panel_open = state.coder.run_panel_open;

    // Agent is its own top-level app — hide any leftover transcript mount.
    editors
        .halo_agent_transcript
        .update(cx, |view, _| view.hide());

    div()
        .id("coder-workspace")
        .size_full()
        .flex()
        .flex_row()
        .overflow_hidden()
        .child(render_shell(
            state,
            editors,
            sidebar_open,
            run_panel_open,
            terminal_open,
            window,
            cx,
        ))
}

/// Horizontal shell: optional explorer (left), center column, optional Run/Build (right).
///
/// Side panels are omitted (not merely hidden) when closed. A hidden first panel
/// still leaves a leading resize handle on the next sibling.
fn render_shell(
    state: &BridgeState,
    editors: &ToolEditors,
    explorer_sidebar: bool,
    run_panel: bool,
    terminal_open: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    // Separate ids so panel-size state does not bleed across layouts that have
    // a different number of children.
    let group_id = match (explorer_sidebar, run_panel) {
        (true, true) => "coder-shell-both",
        (true, false) => "coder-shell",
        (false, true) => "coder-shell-run",
        (false, false) => "coder-shell-wide",
    };

    ResizablePanelGroup::horizontal(group_id)
        .with_handle(true)
        .when(explorer_sidebar, |group| {
            group.child(
                resizable_panel()
                    .size(SIDEBAR_WIDTH)
                    .size_range(SIDEBAR_MIN..SIDEBAR_MAX)
                    .child(
                        div()
                            .size_full()
                            .overflow_hidden()
                            .child(render_sidebar_panel(state, editors, cx)),
                    ),
            )
        })
        .child(
            resizable_panel().child(
                ResizablePanelGroup::vertical("coder-center")
                    .with_handle(true)
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .min_h_0()
                                .overflow_hidden()
                                .child(render_editor_pane(state, editors, cx)),
                        ),
                    )
                    .child(
                        resizable_panel()
                            .size(TERMINAL_HEIGHT)
                            .size_range(TERMINAL_MIN..TERMINAL_MAX)
                            .visible(terminal_open)
                            .child(div().size_full().overflow_hidden().child(
                                super::terminal::render_terminal_panel(state, editors, window, cx),
                            )),
                    ),
            ),
        )
        .when(run_panel, |group| {
            group.child(
                resizable_panel()
                    .size(RUN_PANEL_WIDTH)
                    .size_range(RUN_PANEL_MIN..RUN_PANEL_MAX)
                    .child(
                        div()
                            .size_full()
                            .overflow_hidden()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .child(render_run_panel(state, editors, cx)),
                    ),
            )
        })
}

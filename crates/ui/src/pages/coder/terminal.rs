//! Docked CLI terminal — tab strip, toolbar, and focusable VT surface.

use celestia_ui::components::composite::tab_bar::{TabBar, TabBarItem};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, CoderTerminalKind};

use crate::pages::editors::ToolEditors;
use crate::pages::source_control::{
    git_log_open, git_log_refresh_button, git_log_tab_button, render_git_log_pane,
};

/// Palette for the Coder terminal dock, taken from the active theme family.
#[derive(Clone, Copy)]
struct TerminalPalette {
    bg: Hsla,
    header_bg: Hsla,
    fg: Hsla,
    cursor: Hsla,
    running: Hsla,
    stopped: Hsla,
    dark: bool,
}

fn terminal_palette(cx: &App) -> TerminalPalette {
    let theme = cx.theme();
    TerminalPalette {
        bg: theme.background,
        header_bg: theme.title_bar,
        fg: theme.foreground,
        cursor: theme.primary,
        running: theme.success,
        stopped: theme.danger,
        dark: theme.is_dark(),
    }
}

pub(super) fn render_terminal_panel(
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let pal = terminal_palette(cx);
    let mono = cx.theme().mono_font_family.clone();
    let active_id = state.coder.active_terminal_id.clone().unwrap_or_default();
    // Liveness lives in the session store, not the work-tab projection, so a
    // tab restored from `WorkData` cannot report a live shell as stopped.
    let alive = state.terminal_sessions.alive(&active_id);
    // The Git Log tab has no PTY: it is a static, read-only transcript, so the
    // shell controls (interrupt / restart / clear / kill) do not apply.
    let log_open = git_log_open(state);
    let (status, status_color) = if log_open {
        ("read-only", pal.fg.opacity(0.6))
    } else if alive {
        ("running", pal.running)
    } else {
        ("stopped", pal.stopped)
    };
    let surfaces = editors.terminal_surfaces.clone();
    // One surface per session; a read-only tab has none.
    let term_surface =
        (!log_open && !active_id.is_empty()).then(|| surfaces.get_or_create(&active_id, cx));
    let font_size = state.coder_settings.terminal_font_size;
    let scrollback = state.coder_settings.scrollback_lines;

    // Palette + settings are window-local; the grid itself survives tab
    // switches because the entity is keyed by session id.
    if let Some(surface) = term_surface.as_ref() {
        surface.update(cx, |surface, cx| {
            surface.set_palette(pal.fg, pal.bg, pal.cursor, pal.dark);
            surface.set_settings(font_size, scrollback);
            surface.sync(cx);
        });
    }

    // Same neutral strip as the editor tab bar — primary accent belongs on CTAs,
    // not on the dock chrome (active chip outline + bottom rule).
    let tab_border = cx.theme().window_border;
    let mut tabs = TabBar::new("coder-terminal-tabs")
        .active(active_id)
        .active_border(tab_border)
        .border_color(tab_border);
    for term in &state.coder.terminals {
        tabs = tabs.item(TabBarItem::new(term.id.clone(), term.title.clone()).closable(true));
    }
    tabs = tabs
        .trailing(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(git_log_tab_button())
                .child(
                    Button::new("coder-term-new")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .leading_icon(PhosphorIcon::Plus)
                        .tooltip("New terminal")
                        .on_click({
                            let surfaces = surfaces.clone();
                            move |_, window, cx| {
                                let _ = Bridge::coder_terminal_new(cx);
                                focus_active_terminal(&surfaces, window, cx);
                            }
                        }),
                ),
        )
        .on_select({
            let surfaces = surfaces.clone();
            move |id, window, cx| {
                let id = id.to_string();
                let _ = Bridge::coder_terminal_select(&id, cx);
                // The log tab has no VT surface to focus; leaving the keyboard
                // on it would swallow the next keystroke.
                let is_log = Bridge::state(cx)
                    .read(cx)
                    .coder
                    .terminals
                    .iter()
                    .any(|t| t.id == id && t.kind == CoderTerminalKind::GitLog);
                if !is_log {
                    surfaces.get_or_create(&id, cx).update(cx, |surface, cx| {
                        surface.focus(window, cx);
                    });
                }
            }
        })
        .on_close({
            let surfaces = surfaces.clone();
            move |id, window, cx| {
                let id = id.to_string();
                let _ = Bridge::coder_terminal_close(&id, cx);
                // The closed tab owned the focus handle; without this the next
                // keystrokes go nowhere until the grid is clicked.
                focus_active_terminal(&surfaces, window, cx);
            }
        });

    let _ = window; // focus handled via surface

    // Shell controls only make sense for a live PTY. The read-only log tab
    // swaps them for a single reload action; Hide stays in both cases.
    let mut actions = div().flex().items_center().gap_1();
    if log_open {
        actions = actions.child(git_log_refresh_button());
    } else {
        actions = actions
            .child(
                Button::new("coder-term-interrupt")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .leading_icon(PhosphorIcon::Stop)
                    .disabled(!alive)
                    .tooltip("Interrupt (Ctrl+C)")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::coder_pty_interrupt(cx);
                    }),
            )
            .child(
                Button::new("coder-term-restart")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .leading_icon(PhosphorIcon::ArrowClockwise)
                    .tooltip("Restart shell")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::coder_pty_restart(cx);
                    }),
            )
            .child(
                Button::new("coder-term-clear")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .leading_icon(PhosphorIcon::Trash)
                    .tooltip("Clear screen (Ctrl+L)")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::coder_pty_clear(cx);
                    }),
            )
            .child(
                Button::new("coder-term-kill")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .leading_icon(PhosphorIcon::X)
                    .disabled(!alive)
                    .tooltip("Kill session")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::coder_pty_kill(cx);
                    }),
            );
    }
    actions = actions.child(
        Button::new("coder-term-hide")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .leading_icon(PhosphorIcon::CaretDown)
            .tooltip("Hide terminal")
            .on_click(|_, _, cx| {
                let _ = Bridge::coder_toggle_terminal(cx);
            }),
    );

    // No border_t — the vertical shell's resize handle is the editor/terminal
    // separator. A second top border stacks into a double rule that drifts
    // apart while dragging.
    div()
        .id("coder-terminal")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(pal.bg)
        .child(
            // No outer py — TabBar owns the top inset + underline. Toolbar is
            // 30px (chip height) and items_end so it shares the same baseline.
            div()
                .w_full()
                .flex_none()
                .flex()
                .items_end()
                .bg(pal.header_bg)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(tabs.bg(pal.header_bg).px_2().pt(px(6.))),
                )
                .child(
                    div()
                        .flex_none()
                        .h(px(30.))
                        .pr_2()
                        .flex()
                        .items_center()
                        .gap_1()
                        .border_b_1()
                        .border_color(tab_border)
                        .child(
                            div()
                                .flex_none()
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(status_color)
                                .child(status),
                        )
                        .child(actions),
                ),
        )
        .child(
            div()
                .id("coder-terminal-body")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(if log_open {
                    render_git_log_pane(state, pal.fg, font_size, mono).into_any_element()
                } else if let Some(surface) = term_surface {
                    surface.into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
}

/// Focus the VT surface of whichever tab is active after a tab operation.
fn focus_active_terminal(
    surfaces: &crate::terminal::TerminalSurfaces,
    window: &mut Window,
    cx: &mut App,
) {
    let id = Bridge::state(cx).read(cx).coder.active_terminal_id.clone();
    if let Some(id) = id {
        surfaces.get_or_create(&id, cx).update(cx, |surface, cx| {
            surface.focus(window, cx);
        });
    }
}

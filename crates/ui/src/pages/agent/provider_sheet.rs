//! Harness picker sheet. Lists Pi, OpenCode, Antigravity, and Cursor with the
//! install and sign-in state Halo already probes, and reuses those actions.

use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, HarnessReadiness};

use super::model_picker::{harness_mark, render_mark};
use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};
use crate::pages::editors::ToolEditors;

const ORDER: [&str; 4] = ["pi", "opencode", "antigravity", "cursor"];

/// What the row's button does. Install progress is a loading style on the
/// same action — npm and the ACP installer cannot be cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    None,
    Install,
    Upgrade,
    FixOwnership,
    Connect,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowActionInput<'a> {
    pub id: &'a str,
    pub installed: bool,
    pub needs_upgrade: bool,
    pub state_blocked: bool,
    pub auth_ready: bool,
    pub login_waiting: bool,
}

pub fn row_action(input: &RowActionInput<'_>) -> RowAction {
    if input.login_waiting && input.id == "antigravity" {
        return RowAction::Cancel;
    }
    if input.state_blocked {
        return RowAction::FixOwnership;
    }
    if input.needs_upgrade {
        return RowAction::Upgrade;
    }
    if !input.installed {
        return RowAction::Install;
    }
    if !input.auth_ready && matches!(input.id, "antigravity" | "cursor") {
        return RowAction::Connect;
    }
    RowAction::None
}

struct SheetRow {
    id: String,
    subtitle: String,
    installed: bool,
    needs_upgrade: bool,
    state_blocked: bool,
    auth_ready: bool,
    pending: bool,
}

/// Focus the sheet the first time it opens, and schedule a readiness probe
/// when the rows are still empty. Paint must not write bridge state itself.
pub(super) fn prepare(
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) {
    let visible = Bridge::agent_provider_sheet_visible(state);
    if visible && !editors.agent_provider_sheet_seen_open.get() {
        editors.agent_provider_sheet_focus.focus(window, cx);
    }
    editors.agent_provider_sheet_seen_open.set(visible);

    let needs_probe = visible
        && state.coder.agent_harness_readiness.is_empty()
        && !state.coder.agent_harness_readiness_loading
        && !editors.agent_provider_probe_scheduled.get();
    if needs_probe {
        editors.agent_provider_probe_scheduled.set(true);
        cx.spawn(async move |cx: &mut AsyncApp| {
            let _ = cx.update(|cx| {
                Bridge::halo_agent_ensure_provider_readiness(cx);
            });
        })
        .detach();
    }
    if state.coder.agent_harness_readiness_loading
        || !state.coder.agent_harness_readiness.is_empty()
    {
        editors.agent_provider_probe_scheduled.set(false);
    }
}

pub(super) fn render(state: &BridgeState, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let focus = editors.agent_provider_sheet_focus.clone();
    let rows = sheet_rows(state);
    let running = state.coder.agent_running;
    let probing = state.coder.agent_harness_readiness_loading || rows.iter().any(|row| row.pending);
    let pending_harness = state.coder.agent_provider_pending_harness.clone();
    let pending_action = state.coder.agent_provider_pending_action.clone();
    let install_busy = state.coder.agent_installing.is_some()
        || state.doctor.is_installing("harness:cursor")
        || state.doctor.is_installing("system:agent-state")
        || pending_action.is_some();
    let login_waiting = state.antigravity_acp_login.is_waiting();

    let header = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_sm()
                        .line_height(CARD_TITLE_LINE)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child("Choose your agent harness"),
                )
                .child(
                    div()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(theme.muted_foreground)
                        .child("Select the runtime that powers your coding sessions."),
                ),
        )
        .child(
            Button::icon("agent-provider-sheet-close", PhosphorIcon::X)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .accessibility_label("Close harness picker")
                .on_click(|_, _, cx| {
                    Bridge::halo_agent_dismiss_provider_sheet(cx);
                }),
        );
    let status = probing.then(|| {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .child(
                Loader::grid_shift()
                    .size(px(12.))
                    .color(theme.muted_foreground),
            )
            .child(
                div()
                    .text_xs()
                    .line_height(META_LINE_HEIGHT)
                    .text_color(theme.muted_foreground)
                    .child("Checking harnesses…"),
            )
            .into_any_element()
    });
    let footer = div()
        .text_xs()
        .line_height(META_LINE_HEIGHT)
        .text_color(theme.muted_foreground)
        .child("You can change this for each project later.");
    let list = row_list(
        state,
        &rows,
        pending_harness.as_deref(),
        pending_action.as_deref(),
        running,
        install_busy,
        login_waiting,
        probing,
        cx,
    );
    let panel = div()
        .id("agent-provider-sheet-panel")
        .role(Role::Dialog)
        .aria_label("Choose your agent harness")
        .track_focus(&focus)
        .on_key_down(|event: &KeyDownEvent, _, cx| {
            let keystroke = &event.keystroke;
            if keystroke.modifiers.alt
                || keystroke.modifiers.control
                || keystroke.modifiers.shift
                || keystroke.modifiers.platform
            {
                return;
            }
            if keystroke.key.as_str() == "escape" {
                Bridge::halo_agent_dismiss_provider_sheet(cx);
            }
        })
        .w(px(560.))
        .max_w(relative(0.92))
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(18.))
        .py(px(18.))
        .rounded(theme.radius_xl())
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow(vec![BoxShadow {
            offset: point(px(0.), px(18.)),
            blur_radius: px(48.),
            spread_radius: px(0.),
            inset: false,
            color: hsla(0., 0., 0., 0.35),
        }])
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(header)
        .children(status)
        .child(list)
        .child(footer);

    div()
        .id("agent-provider-sheet")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background.opacity(0.60))
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            Bridge::halo_agent_dismiss_provider_sheet(cx);
        })
        .child(panel)
        .into_any_element()
}

fn row_list(
    state: &BridgeState,
    rows: &[SheetRow],
    pending_harness: Option<&str>,
    pending_action: Option<&str>,
    running: bool,
    install_busy: bool,
    login_waiting: bool,
    probing: bool,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let mut list = div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .when(probing, |el| el.opacity(0.92));
    for (index, row) in rows.iter().enumerate() {
        let rendered = provider_row(
            state,
            row,
            pending_harness,
            pending_action,
            index + 1 == rows.len(),
            running,
            install_busy,
            login_waiting,
            cx,
        );
        list = list.child(rendered);
    }
    list.into_any_element()
}

fn provider_row(
    state: &BridgeState,
    row: &SheetRow,
    pending_harness: Option<&str>,
    pending_action: Option<&str>,
    last: bool,
    running: bool,
    install_busy: bool,
    login_waiting: bool,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let selected = pending_harness == Some(row.id.as_str())
        || (pending_harness.is_none() && state.coder.agent_harness == row.id);
    let (mark, tint) = harness_mark(&row.id);
    let action = if row.pending {
        RowAction::None
    } else {
        row_action(&RowActionInput {
            id: &row.id,
            installed: row.installed,
            needs_upgrade: row.needs_upgrade,
            state_blocked: row.state_blocked,
            auth_ready: row.auth_ready,
            login_waiting: login_waiting && row.id == "antigravity",
        })
    };
    let this_busy = row_is_busy(state, row) || pending_action == Some(row.id.as_str());
    let name = HarnessReadiness::display_name(&row.id).to_string();
    let id = row.id.clone();
    let error = if row.pending || this_busy {
        None
    } else {
        row_error(state, &row.id)
    };

    let mark_el = div()
        .flex_none()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .child(render_mark(
            mark,
            tint,
            px(18.),
            PhosphorIcon::Cpu,
            theme.foreground,
        ));
    let title = div()
        .text_sm()
        .line_height(CARD_TITLE_LINE)
        .font_weight(FontWeight::NORMAL)
        .text_color(theme.foreground)
        .child(name);
    let live_stage =
        if this_busy && state.coder.agent_installing.as_deref() == Some(row.id.as_str()) {
            state.coder.agent_install_stage.clone()
        } else {
            None
        };
    let subtitle_text = live_stage.unwrap_or_else(|| row.subtitle.clone());
    let subtitle = if row.pending || this_busy {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                Loader::grid_shift()
                    .size(px(10.))
                    .color(theme.muted_foreground),
            )
            .child(
                div()
                    .text_xs()
                    .line_height(META_LINE_HEIGHT)
                    .text_color(theme.muted_foreground)
                    .child(subtitle_text),
            )
    } else {
        div()
            .text_xs()
            .line_height(META_LINE_HEIGHT)
            .text_color(theme.muted_foreground)
            .child(subtitle_text)
    };
    let mut text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(title)
        .child(subtitle);
    if let Some(error) = error {
        let error_line = div()
            .text_xs()
            .line_height(px(16.))
            .text_color(theme.danger)
            .child(error);
        text = text.child(error_line);
    }
    let select_id = id.clone();
    let select = div()
        .id(SharedString::from(format!("agent-provider-select-{id}")))
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(12.))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            let id = select_id.clone();
            Bridge::state(cx).update(cx, |s| {
                if s.coder.agent_running || s.coder.agent_harness == id || !known_harness(&id) {
                    return;
                }
                s.coder.agent_provider_pending_harness = Some(id.clone());
            });
            let _ = Bridge::halo_agent_choose_provider(&id, cx);
        })
        .child(radio(selected, theme.primary, theme.border))
        .child(mark_el)
        .child(text);
    let actions = if row.pending {
        div()
            .flex_none()
            .w(px(88.))
            .flex()
            .items_center()
            .justify_end()
            .child(
                Loader::grid_shift()
                    .size(px(12.))
                    .color(theme.muted_foreground),
            )
            .into_any_element()
    } else {
        row_actions(
            action,
            &id,
            this_busy,
            running,
            install_busy,
            theme.warning,
            theme.success,
        )
        .into_any_element()
    };
    let mut row_el = div()
        .id(SharedString::from(format!("agent-provider-row-{id}")))
        .w_full()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(12.))
        .child(select)
        .child(actions);
    if !last {
        row_el = row_el.border_b_1().border_color(theme.border);
    }
    if selected {
        row_el = row_el.bg(theme.muted.opacity(0.35));
    }
    row_el.into_any_element()
}

fn row_actions(
    action: RowAction,
    id: &str,
    this_busy: bool,
    running: bool,
    install_busy: bool,
    warning: Hsla,
    success: Hsla,
) -> impl IntoElement {
    let id_owned = id.to_string();
    let blocked = running || (install_busy && !this_busy);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .when(action == RowAction::None && !this_busy, |el| {
            el.child(ready_chip(success))
        })
        .when(action == RowAction::Upgrade, |el| {
            el.child(update_chip(warning))
        })
        .when(action == RowAction::Cancel, |el| {
            el.child(div().text_xs().text_color(warning).child("Connecting…"))
        })
        .when(action != RowAction::None, |el| {
            el.child(action_button(action, &id_owned, this_busy, blocked))
        })
}

fn action_button(action: RowAction, id: &str, this_busy: bool, blocked: bool) -> impl IntoElement {
    let label = if this_busy {
        match action {
            RowAction::Upgrade => "Upgrading…",
            RowAction::FixOwnership => "Fixing…",
            RowAction::Install => "Installing…",
            RowAction::Cancel => "Cancel",
            RowAction::Connect => "Connect",
            RowAction::None => "",
        }
    } else {
        match action {
            RowAction::Install => "Install",
            RowAction::Upgrade => "Upgrade",
            RowAction::FixOwnership => "Fix ownership",
            RowAction::Connect => "Connect",
            RowAction::Cancel => "Cancel",
            RowAction::None => "",
        }
    };
    let variant = match action {
        RowAction::Connect => ButtonVariant::Primary,
        RowAction::Cancel => ButtonVariant::Outline,
        _ => ButtonVariant::Secondary,
    };
    let harness = id.to_string();
    let show_loading = this_busy && !matches!(action, RowAction::Cancel | RowAction::Connect);
    Button::new(SharedString::from(format!("agent-provider-action-{id}")))
        .variant(variant)
        .size(ButtonSize::Small)
        .label(label)
        .loading(show_loading)
        .loading_style(ButtonLoadingStyle::GridShift)
        .disabled(blocked && !matches!(action, RowAction::Cancel))
        .on_click(move |_, window, cx| {
            // Install work is already off the UI thread. Starting it here, on
            // the click, means a dropped window update cannot skip it.
            dispatch_action(action, &harness, window, cx);
        })
}

fn dispatch_action(action: RowAction, id: &str, window: &mut Window, cx: &mut App) {
    match action {
        RowAction::Install | RowAction::Upgrade => {
            if known_harness(id) {
                Bridge::state(cx).update(cx, |s| {
                    s.coder.agent_provider_pending_action = Some(id.to_string());
                });
            }
            let started = Bridge::halo_agent_install_harness(id, cx);
            let installing = Bridge::state(cx)
                .read(cx)
                .coder
                .agent_installing
                .as_deref()
                == Some(id);
            if started.is_err() || !installing {
                Bridge::state(cx).update(cx, |s| {
                    if s.coder.agent_provider_pending_action.as_deref() == Some(id)
                        && s.coder.agent_installing.as_deref() != Some(id)
                    {
                        s.coder.agent_provider_pending_action = None;
                    }
                });
            }
        }
        RowAction::FixOwnership => {
            crate::pages::doctor::request_fix(window, cx, "system:agent-state");
        }
        RowAction::Connect => match id {
            "antigravity" => {
                let _ = Bridge::halo_agent_choose_provider("antigravity", cx);
                Bridge::coder_antigravity_acp_clear_login_error(cx);
                let _ = Bridge::coder_antigravity_acp_begin_login(cx);
            }
            "cursor" => {
                let _ = Bridge::halo_agent_choose_provider("cursor", cx);
                Bridge::halo_agent_dismiss_provider_sheet(cx);
            }
            _ => {}
        },
        RowAction::Cancel => Bridge::coder_antigravity_acp_cancel_login(cx),
        RowAction::None => {}
    }
}

fn ready_chip(success: Hsla) -> impl IntoElement {
    div()
        .px(px(8.))
        .py(px(3.))
        .rounded_full()
        .bg(success.opacity(0.14))
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(success)
        .child("Ready")
}

fn update_chip(warning: Hsla) -> impl IntoElement {
    div()
        .px(px(8.))
        .py(px(3.))
        .rounded_full()
        .bg(warning.opacity(0.16))
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(warning)
        .child("Update required")
}

fn radio(selected: bool, primary: Hsla, border: Hsla) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(16.))
        .rounded_full()
        .border_1()
        .border_color(if selected { primary } else { border })
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |el| {
            el.child(div().size(px(8.)).rounded_full().bg(primary))
        })
}

fn known_harness(id: &str) -> bool {
    ORDER.contains(&id)
}

fn row_is_busy(state: &BridgeState, row: &SheetRow) -> bool {
    if state.coder.agent_installing.as_deref() == Some(row.id.as_str()) {
        return true;
    }
    if row.id == "cursor" && state.doctor.is_installing("harness:cursor") {
        return true;
    }
    row.id == "opencode" && row.state_blocked && state.doctor.is_installing("system:agent-state")
}

fn row_error(state: &BridgeState, id: &str) -> Option<String> {
    if state.coder.agent_install_error_harness.as_deref() == Some(id) {
        if let Some(err) = state.coder.agent_install_error.as_deref() {
            return Some(brief(err));
        }
    }
    let doctor_id = match id {
        "cursor" => Some("harness:cursor"),
        "opencode"
            if state
                .coder
                .agent_harness_readiness
                .iter()
                .find(|row| row.id == "opencode")
                .is_some_and(|row| row.state_blocked) =>
        {
            Some("system:agent-state")
        }
        _ => None,
    }?;
    state
        .doctor
        .last_log_for(doctor_id)
        .filter(|entry| entry.is_error)
        .map(|entry| brief(&entry.message))
}

fn brief(err: &str) -> String {
    let line = err
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(err)
        .trim();
    let mut chars = line.chars();
    let clipped: String = chars.by_ref().take(180).collect();
    if chars.next().is_some() {
        format!("{clipped}…")
    } else {
        clipped
    }
}

fn sheet_rows(state: &BridgeState) -> Vec<SheetRow> {
    if state.coder.agent_harness_readiness.is_empty() {
        return ORDER
            .into_iter()
            .map(|id| {
                if id == state.coder.agent_harness.as_str() {
                    live_selected(state, id)
                } else {
                    pending(id)
                }
            })
            .collect();
    }
    ORDER
        .into_iter()
        .map(|id| {
            state
                .coder
                .agent_harness_readiness
                .iter()
                .find(|row| row.id == id)
                .map(|row| from_readiness(state, row))
                .unwrap_or_else(|| pending(id))
        })
        .collect()
}

fn from_readiness(state: &BridgeState, row: &HarnessReadiness) -> SheetRow {
    let mut auth_ready = row.auth_ready;
    let mut subtitle = row.subtitle.clone();
    if row.id == "antigravity"
        && row.installed
        && !row.needs_upgrade
        && !row.state_blocked
        && antigravity_authed(state)
    {
        auth_ready = true;
        subtitle = HarnessReadiness::ready_blurb("antigravity").to_string();
    }
    SheetRow {
        id: row.id.clone(),
        subtitle,
        installed: row.installed,
        needs_upgrade: row.needs_upgrade,
        state_blocked: row.state_blocked,
        auth_ready,
        pending: false,
    }
}

fn live_selected(state: &BridgeState, id: &str) -> SheetRow {
    let hint = state.coder.agent_pi_setup_hint.clone();
    let installed = state.coder.agent_harness_installed;
    let needs_upgrade = state.coder.agent_pi_needs_upgrade;
    let state_blocked = state.coder.agent_state_blocked;
    let cursor_key = id == "cursor" && hint.as_deref().is_some_and(|hint| hint.contains("API key"));
    let binary = if cursor_key {
        true
    } else {
        installed || needs_upgrade || state_blocked
    };
    let auth_ready = match id {
        "antigravity" => antigravity_authed(state),
        "cursor" => installed && !cursor_key,
        _ => true,
    };
    let ready = binary && !needs_upgrade && !state_blocked && auth_ready;
    let subtitle = if ready {
        HarnessReadiness::ready_blurb(id).to_string()
    } else {
        hint.unwrap_or_else(|| "Checking…".into())
    };
    SheetRow {
        id: id.to_string(),
        subtitle,
        installed: binary,
        needs_upgrade,
        state_blocked,
        auth_ready,
        pending: false,
    }
}

fn pending(id: &str) -> SheetRow {
    SheetRow {
        id: id.to_string(),
        subtitle: "Checking…".into(),
        installed: false,
        needs_upgrade: false,
        state_blocked: false,
        auth_ready: false,
        pending: true,
    }
}

fn antigravity_authed(state: &BridgeState) -> bool {
    state.antigravity_acp_signed_in
        || (state.coder.agent_harness == "antigravity"
            && state
                .coder
                .agent_sessions
                .iter()
                .find(|session| session.id == state.coder.agent_active_session_id)
                .is_some_and(|session| session.harness_session_id.is_some()))
}

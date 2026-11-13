//! Universal AI agent surface — Halo Agent for now.
//!
//! Layout: a left Sessions rail, a centred chat column (transcript + composer),
//! and a right Context rail (session, subagents, skills, MCP, artifacts,
//! sources). Session state still lives on [`halo_bridge::CoderWork`] agent
//! fields until a dedicated agent work model lands.

mod artifacts;
mod blocks;
mod completion;
mod composer;
mod connect;
mod context_rail;
mod header;
mod history;
mod markdown;
mod model_picker;
mod permission;
mod provider_sheet;
mod recent;
mod subagent;
mod subagent_drawer;
mod text_cache;
mod transcript;
mod transcript_state;
mod transcript_view;
mod trays;
pub use provider_sheet::{RowAction, RowActionInput, row_action};
pub use subagent_drawer::SubagentTranscript;
pub use transcript_view::AgentTranscript;

pub use composer::sync_composer_mode;

pub fn preview_end(source: &str) -> usize {
    transcript_state::preview_end(source)
}

pub fn row_splice_parts(old: &[String], new: &[String]) -> Option<(usize, usize, usize)> {
    transcript_state::row_splice(old, new)
        .map(|splice| (splice.old.start, splice.old.end, splice.inserted))
}

pub(crate) use completion::apply_accept as accept_completion;

use celestia_ui::components::composite::banner::{Banner, BannerTone};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::editors::ToolEditors;

/// Width of the centred column. Transcript and composer share it so their
/// edges line up when a session starts.
const COLUMN_MAX: Pixels = px(760.);
/// Horizontal breathing room on narrow windows.
const GUTTER: Pixels = px(16.);

// Chat text metrics — locked to the app rem steps at the default 16px root
// (`text_base` / `text_sm` / `text_xs`). Absolute px keep streaming rows on a
// stable line box; Appearance rem still scales every `.text_*()` call site.
pub(crate) const TEXT_SIZE: Pixels = px(16.); // text_base — markdown heading base
pub(crate) const LINE_HEIGHT: Pixels = px(25.);
pub(crate) const BLOCK_GAP: Pixels = px(4.);
pub(crate) const META_LINE_HEIGHT: Pixels = px(18.); // with text_xs
pub(crate) const CARD_TITLE_LINE: Pixels = px(20.); // with text_sm

/// Display name for the active harness. Catalog name first, then the same
/// fallbacks the model picker uses before the catalog has loaded.
pub(crate) fn harness_label(state: &BridgeState) -> String {
    let id = state.coder.agent_harness.as_str();
    state
        .coder
        .agent_harness_catalog
        .iter()
        .find(|harness| harness.id == id)
        .map(|harness| harness.display_name.clone())
        .unwrap_or_else(|| match id {
            "opencode" => "OpenCode".into(),
            "antigravity" => "Antigravity".into(),
            "cursor" => "Cursor".into(),
            _ => "Pi".into(),
        })
}

pub fn render_agent(
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    editors
        .halo_agent_transcript
        .update(cx, |view, cx| view.mount(cx));
    sync_composer_mode(editors, !state.coder.agent_messages.is_empty(), window, cx);
    provider_sheet::prepare(state, editors, window, cx);
    let provider_sheet_open = Bridge::agent_provider_sheet_visible(state);

    let empty = state.coder.agent_messages.is_empty();
    // A child transcript takes the right slot: 760px column + 300px context
    // rail + 320px drawer does not fit most windows, and the open child is the
    // focused surface while it is visible.
    let drawer_open = state
        .coder
        .agent_open_subagent
        .as_deref()
        .is_some_and(|session| {
            state
                .coder
                .agent_subagents
                .iter()
                .any(|child| child.session_id == session)
        });

    div()
        .id("agent-panel")
        .relative()
        .size_full()
        .flex()
        .flex_row()
        .overflow_hidden()
        .bg(cx.theme().background)
        .child(history::render_sessions_rail(state, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(header::render_top_bar(state, cx))
                // Keep landing / conversation mounted — setup sits above the
                // prompt, not as a full-column swap. Chat switches instantly from
                // in-memory sessions; workspace hydrate runs in the background.
                .child(if empty {
                    render_landing(state, editors, cx).into_any_element()
                } else {
                    render_conversation(state, editors, cx).into_any_element()
                }),
        )
        .child(if drawer_open {
            subagent_drawer::render_subagent_drawer(state, editors, cx)
        } else {
            context_rail::render_context_rail(state, editors, cx)
        })
        .when(provider_sheet_open, |el| {
            el.child(provider_sheet::render(state, editors, cx))
        })
        .into_any_element()
}

/// Harness missing / upgrade / blocked — stacked directly above the prompt input.
fn render_setup_above_prompt(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> Option<AnyElement> {
    let needs_setup = !state.coder.agent_harness_installed
        || state.coder.agent_state_blocked
        || (state.coder.agent_harness == "pi" && state.coder.agent_pi_needs_upgrade);
    if !needs_setup {
        return None;
    }

    Some(
        div()
            .id("agent-setup-above-prompt")
            .w_full()
            .child(render_harness_setup_card(state, editors, cx))
            .into_any_element(),
    )
}

/// Inline workspace folder prompt when no project folder is open.
fn render_workspace_notice(state: &BridgeState, cx: &App) -> Option<AnyElement> {
    if state.coder.root_path.is_some() {
        return None;
    }
    let theme = cx.theme();
    Some(
        div()
            .id("agent-workspace-notice")
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(10.))
            .px(px(12.))
            .py(px(8.))
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .flex_1()
                    .child(
                        Phosphor::new(PhosphorIcon::FolderOpen)
                            .size(px(14.))
                            .color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .text_color(theme.muted_foreground)
                            .child("Select a project folder so the agent can read and edit your workspace."),
                    ),
            )
            .child(
                Button::new("agent-pick-workspace-folder")
                    .variant(ButtonVariant::Secondary)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::FolderOpen)
                    .label("Open Folder…")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::coder_open_folder(cx);
                    }),
            )
            .into_any_element(),
    )
}

/// First-run surface: the composer sits in the middle of the page with nothing
/// competing for attention.
fn render_landing(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(GUTTER)
        .pb(px(32.))
        .child(
            div()
                .w_full()
                .max_w(COLUMN_MAX)
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(permission::render_permission_panel(state, cx))
                .children(render_notices_above_prompt(state, editors, cx))
                .child(composer::render_composer(state, editors, cx))
                .children(backend_failure_note(state))
                .child(composer::render_checkout_row(state, cx))
                // Recent chats + open plan steps: the empty state keeps the
                // last work one click away instead of starting cold.
                .children(recent::render_recent_work(state, cx)),
        )
}

/// Setup + auth notices stacked above the prompt (full column width).
fn render_notices_above_prompt(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> Vec<AnyElement> {
    // The sheet is the gate. Setup and auth cards return once it is dismissed.
    if Bridge::agent_provider_sheet_visible(state) {
        return Vec::new();
    }
    [
        render_workspace_notice(state, cx),
        render_setup_above_prompt(state, editors, cx),
        connect::render_auth_card(state, editors, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Live session: the transcript scrolls and the composer docks under it — the
/// same column, so the prompt never moves horizontally between the two states.
fn render_conversation(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .px(GUTTER)
        .child(
            div()
                .id("agent-transcript")
                .w_full()
                .max_w(COLUMN_MAX)
                .flex_1()
                .min_h_0()
                .child(editors.halo_agent_transcript.clone()),
        )
        .child(
            div()
                .w_full()
                .max_w(COLUMN_MAX)
                .flex_none()
                .flex()
                .flex_col()
                // Trays + approval are one group (8px); composer sits 16px below
                // so controls stay distinct from the chat chrome.
                .gap(px(8.))
                .pb(px(10.))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(render_desktop_strip(state, cx))
                        .child(subagent::render_subagent_tray(state, cx))
                        .child(trays::render_todo_tray(
                            state,
                            state.coder.agent_pending_input.is_some()
                                && !state.coder.agent_dock_todo_open,
                            cx,
                        ))
                        .child(trays::render_queue_tray(
                            state,
                            state.coder.agent_pending_input.is_some()
                                && !state.coder.agent_dock_queue_open,
                            cx,
                        ))
                        .child(permission::render_permission_panel(state, cx)),
                )
                .child(
                    div()
                        .w_full()
                        .mt(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .children(render_notices_above_prompt(state, editors, cx))
                        .child(composer::render_composer(state, editors, cx))
                        .children(backend_failure_note(state))
                        .child(composer::render_checkout_row(state, cx)),
                ),
        )
}

/// One line while this chat holds the desktop lease. Absent when idle.
fn render_desktop_strip(state: &BridgeState, cx: &App) -> AnyElement {
    if !state.coder.agent_desktop.lease_held {
        return Empty.into_any_element();
    }
    let theme = cx.theme();
    let label = match state.coder.agent_desktop.lease_app.as_deref() {
        Some(app) if !app.is_empty() => format!("Controlling {app}"),
        _ => "Controlling the desktop".into(),
    };
    div()
        .id("agent-desktop-strip")
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .py(px(6.))
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(theme.foreground)
                .child(label),
        )
        .child(
            div().flex_none().child(
                Button::new("agent-desktop-stop")
                    .variant(ButtonVariant::Secondary)
                    .size(ButtonSize::Small)
                    .label("Stop")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::halo_agent_release_desktop(cx);
                    }),
            ),
        )
        .into_any_element()
}

/// Native boot failure notice: the app fell back to the stub backend, so the
/// agent (and proxy) cannot run. Carries the boot error itself — the plain
/// "unavailable in stub mode" toast made this look like an agent bug.
fn backend_failure_note(state: &BridgeState) -> Option<AnyElement> {
    let reason = state.backend_failure.clone()?;
    Some(
        Banner::new("agent-backend-failed", "Backend failed to start")
            .tone(BannerTone::Danger)
            .body(format!(
                "Halo Agent and proxy are offline for this session. {reason}"
            ))
            .into_any_element(),
    )
}

mod setup_cards;

use setup_cards::*;

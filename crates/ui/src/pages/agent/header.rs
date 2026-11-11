//! Slim top bar for the full-page agent surface — identity, run state, actions.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, HarnessReadiness, SettingsTab};

use super::CARD_TITLE_LINE;
use crate::components::confirm_destructive;
use crate::pages::coder::project_display_name;

pub(super) fn render_top_bar(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let success = theme.success;
    let running = state.coder.agent_running;
    let history_open = state.coder.agent_history_open;
    let context_open = state.coder.agent_context_open;
    let project = state.coder.root_path.as_deref().map(project_display_name);

    div()
        .id("agent-top-bar")
        .w_full()
        .flex_none()
        .h(px(40.))
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .border_b_1()
        .border_color(theme.border)
        .child(
            Phosphor::new(PhosphorIcon::Folder)
                .size(px(14.))
                .color(if project.is_some() { foreground } else { muted }),
        )
        .when_some(project, |el, project| {
            el.child(
                div()
                    .text_sm()
                    .line_height(CARD_TITLE_LINE)
                    .font_weight(FontWeight::NORMAL)
                    .text_color(foreground)
                    .child(project),
            )
        })
        .when(running, |el| {
            let label = state
                .coder
                .agent_sessions
                .iter()
                .find(|s| s.id == state.coder.agent_active_session_id)
                .and_then(|s| s.run_started_at)
                .map(|started| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(started);
                    let secs = now.saturating_sub(started) / 1000;
                    if secs < 60 {
                        format!("Worked for {secs}s")
                    } else {
                        let mins = secs / 60;
                        let rem = secs % 60;
                        if rem == 0 {
                            format!("Worked for {mins}m")
                        } else {
                            format!("Worked for {mins}m {rem}s")
                        }
                    }
                })
                .unwrap_or_else(|| "Running".into());
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px(px(7.))
                    .py(px(3.))
                    .rounded_full()
                    .child(div().size(px(6.)).rounded_full().bg(success))
                    .child(div().text_xs().text_color(muted).child(label)),
            )
        })
        .child(div().flex_1())
        .child(
            Button::new("agent-change-harness")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label(HarnessReadiness::display_name(&state.coder.agent_harness))
                .tooltip("Change harness")
                .on_click(|_, _, cx| {
                    Bridge::halo_agent_open_provider_sheet(cx);
                }),
        )
        .child(
            Button::icon("coder-agent-history", PhosphorIcon::SidebarSimple)
                .variant(if history_open {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::IconSm)
                .accessibility_label("Toggle sessions")
                .tooltip(if history_open {
                    "Hide sessions"
                } else {
                    "Show sessions"
                })
                .on_click(|_, _, cx| {
                    let _ = Bridge::halo_agent_toggle_history(cx);
                }),
        )
        .child(
            Button::icon("coder-agent-context", PhosphorIcon::SquaresFour)
                .variant(if context_open {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::IconSm)
                .accessibility_label("Toggle context")
                .tooltip(if context_open {
                    "Hide context"
                } else {
                    "Show context"
                })
                .on_click(|_, _, cx| {
                    let _ = Bridge::halo_agent_toggle_context(cx);
                }),
        )
        .child(
            Button::icon("coder-agent-settings", PhosphorIcon::Gear)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .accessibility_label("Agent settings")
                .tooltip("Agent settings")
                .on_click(|_, _, cx| {
                    crate::windows::open_settings_window(SettingsTab::Agent, cx);
                }),
        )
        .child(
            Button::icon("coder-agent-clear", PhosphorIcon::Trash)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .accessibility_label("Clear transcript")
                .tooltip("Clear transcript")
                .on_click(|_, window, cx| {
                    confirm_destructive(
                        window,
                        cx,
                        "Clear transcript?",
                        "Every message in the current session will be removed. This cannot be undone.",
                        "Clear",
                        |_, cx| {
                            let _ = Bridge::halo_agent_clear(cx);
                        },
                    );
                }),
        )
        .child(
            Button::icon("agent-close", PhosphorIcon::X)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .accessibility_label("Close agent")
                .tooltip("Close agent")
                .on_click(|_, _, cx| {
                    if let Some(id) = Bridge::state(cx).read(cx).active_work_tab_id.clone() {
                        let _ = Bridge::close_work_tab(&id, cx);
                    }
                }),
        )
}

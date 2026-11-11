//! Reactive [`BridgeState`] defaults and the tab/session projection helpers.

use super::*;
use crate::work::CoderWork;

pub(crate) fn is_agent_token(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::TextDelta { .. }
            | AgentEvent::ReasoningDelta { .. }
            | AgentEvent::ContentUpdated { .. }
    )
}

pub(crate) fn coder_chrome_eq(next: &CoderWork, prev: &CoderWork) -> bool {
    next.agent_open == prev.agent_open
        && next.agent_running == prev.agent_running
        && next.agent_active_session_id == prev.agent_active_session_id
        && next.agent_draft == prev.agent_draft
        && next.agent_attachments == prev.agent_attachments
        && next.agent_plan_mode == prev.agent_plan_mode
        && next.agent_approval_mode == prev.agent_approval_mode
        && next.agent_approval_picker_open == prev.agent_approval_picker_open
        && next.agent_pending_input == prev.agent_pending_input
        && next.agent_queue == prev.agent_queue
        && next.agent_file_changes == prev.agent_file_changes
        && next.agent_todos == prev.agent_todos
        && next.agent_messages.len() == prev.agent_messages.len()
        && next.root_path == prev.root_path
        && next.agent_sessions.len() == prev.agent_sessions.len()
        && next
            .agent_sessions
            .iter()
            .zip(&prev.agent_sessions)
            .all(|(next, prev)| {
                next.id == prev.id
                    && next.status == prev.status
                    && next.title == prev.title
                    && next.archived == prev.archived
                    && next.messages.len() == prev.messages.len()
                    && next.run_id == prev.run_id
                    && next.queue == prev.queue
                    && next.pending_input == prev.pending_input
                    && next.file_changes == prev.file_changes
                    && next.draft == prev.draft
                    && next.attachments == prev.attachments
                    && next.plan_mode == prev.plan_mode
                    && next.approval_mode == prev.approval_mode
            })
}

/// Harness install, upgrade, and the provider sheet. Token text must not hide
/// these, or Install / Upgrade / Fix Ownership look like the click did nothing.
pub(crate) fn harness_shell_eq(next: &CoderWork, prev: &CoderWork) -> bool {
    next.agent_harness == prev.agent_harness
        && next.agent_harness_installed == prev.agent_harness_installed
        && next.agent_pi_setup_hint == prev.agent_pi_setup_hint
        && next.agent_pi_needs_upgrade == prev.agent_pi_needs_upgrade
        && next.agent_state_blocked == prev.agent_state_blocked
        && next.agent_installing == prev.agent_installing
        && next.agent_install_error == prev.agent_install_error
        && next.agent_install_error_harness == prev.agent_install_error_harness
        && next.agent_harness_readiness == prev.agent_harness_readiness
        && next.agent_provider_sheet_open == prev.agent_provider_sheet_open
        && next.agent_provider_sheet_dismissed == prev.agent_provider_sheet_dismissed
}

mod events;
mod notes;
mod overviews;
mod projection;
mod tabs;

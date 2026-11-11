//! Live run status and work-timer derivation for the transcript header.

use super::*;

pub(crate) const USER_PAD_X: Pixels = px(16.);
pub(crate) const USER_PAD_Y: Pixels = px(10.);
/// User bubble corner — theme base radius (quieter than the 16px chat default).
pub(crate) const USER_RADIUS: Pixels = px(8.);

/// What the agent is doing right now, for the stretches the transcript cannot
/// show on its own.
#[derive(Clone, PartialEq)]
pub(crate) struct LiveStatus {
    pub(crate) icon: PhosphorIcon,
    pub(crate) label: String,
}

/// Elapsed-work counter shown under the transcript while a run is live (and
/// kept briefly after it settles).
#[derive(Clone, PartialEq)]
pub(crate) struct WorkTimer {
    pub secs: u64,
    pub running: bool,
    pub detail: Option<LiveStatus>,
    pub expanded: bool,
}

/// Resolve the in-flight marker, or `None` when the run is idle or is already
/// speaking for itself.
///
/// The rule is "is there a visible gap between parts?": streaming text is its
/// own progress, and a streaming `Reasoning` already carries a shimmering
/// header, so both suppress the marker. A freshly submitted turn, an empty
/// assistant message, a tool in flight, and the pause after a tool returns all
/// leave a gap and therefore get one.
pub(crate) fn live_status(state: &BridgeState) -> Option<LiveStatus> {
    if !state.coder.agent_running {
        return None;
    }
    let last = state.coder.agent_messages.last()?;

    // The turn was just submitted — the harness has not emitted anything yet.
    if last.role != HaloAgentRole::Assistant {
        return Some(LiveStatus {
            icon: PhosphorIcon::Brain,
            label: "Thinking…".into(),
        });
    }

    match last.parts.last() {
        Some(HaloAgentPart::Text(_))
        | Some(HaloAgentPart::Error(_))
        | Some(HaloAgentPart::Interrupted)
        | Some(HaloAgentPart::Attachments(_)) => None,
        Some(HaloAgentPart::Reasoning { .. }) => None,
        // Tool labels are already verb phrases ("Run cargo test", "Read x"), so
        // they are used as-is rather than behind another verb.
        Some(HaloAgentPart::ToolCall {
            label, done: false, ..
        }) => Some(LiveStatus {
            icon: PhosphorIcon::Wrench,
            label: format!("{label}…"),
        }),
        // Between parts: a tool just returned, or the message is still empty.
        _ => Some(LiveStatus {
            icon: PhosphorIcon::Brain,
            label: "Thinking…".into(),
        }),
    }
}

/// Live / settled "Worked for Xs" counter for the active session.
pub(crate) fn work_timer(state: &BridgeState, expanded: bool) -> Option<WorkTimer> {
    let session = state
        .coder
        .agent_sessions
        .iter()
        .find(|s| s.id == state.coder.agent_active_session_id)?;
    let secs = if let Some(started) = session.run_started_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(started);
        now.saturating_sub(started) / 1000
    } else {
        session.last_worked_secs?
    };
    if !state.coder.agent_running && session.last_worked_secs.is_none() {
        return None;
    }
    Some(WorkTimer {
        secs,
        running: state.coder.agent_running,
        detail: live_status(state),
        expanded,
    })
}

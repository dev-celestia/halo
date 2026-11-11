//! Work-tab shell layout persistence.

use super::*;

pub(crate) fn apply_shell_layout(
    state: &mut BridgeState,
    layout: halo_backend::commands::shell::ShellLayout,
    coder_seed: Option<&CoderWork>,
) {
    if layout.sessions.is_empty() && layout.tabs.is_empty() {
        return;
    }

    let sessions: Vec<ActivitySession> = layout
        .sessions
        .into_iter()
        .map(|session| ActivitySession {
            id: session.id,
            title: session.title,
            active_work_tab_id: session.active_work_tab_id,
            history: Vec::new(),
        })
        .collect();

    let mut tabs = Vec::new();
    for tab in layout.tabs {
        let Some(kind) = WorkKind::from_str(&tab.kind) else {
            continue;
        };
        // Drop alpha apps in shipping builds (same idea as retired Terminal).
        if !kind.ships() {
            continue;
        }
        let data = if kind == WorkKind::Coder {
            coder_seed
                .cloned()
                .map(WorkData::Coder)
                .unwrap_or_else(|| WorkData::for_kind(kind))
        } else {
            WorkData::for_kind(kind)
        };
        tabs.push(WorkTab {
            id: tab.id,
            title: tab.title,
            kind,
            session_id: tab.session_id,
            data,
        });
    }

    if sessions.is_empty() && tabs.is_empty() {
        return;
    }

    state.activity_sessions = if sessions.is_empty() {
        let id = uuid::Uuid::new_v4().to_string();
        vec![ActivitySession {
            id: id.clone(),
            title: "Session".into(),
            active_work_tab_id: layout.active_tab_id.clone(),
            history: Vec::new(),
        }]
    } else {
        sessions
    };

    let active_session = layout
        .active_session_id
        .filter(|id| state.activity_sessions.iter().any(|s| &s.id == id))
        .or_else(|| state.activity_sessions.first().map(|s| s.id.clone()));
    state.active_activity_session_id = active_session;

    // Drop tabs that point at unknown sessions.
    let known_sessions: std::collections::HashSet<&str> = state
        .activity_sessions
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    tabs.retain(|tab| known_sessions.contains(tab.session_id.as_str()));
    state.work_tabs = tabs;

    let active_tab = layout
        .active_tab_id
        .filter(|id| state.work_tabs.iter().any(|t| &t.id == id))
        .or_else(|| {
            state
                .active_activity_session_id
                .as_ref()
                .and_then(|sid| {
                    state
                        .activity_sessions
                        .iter()
                        .find(|s| &s.id == sid)
                        .and_then(|s| s.active_work_tab_id.clone())
                })
                .filter(|id| state.work_tabs.iter().any(|t| &t.id == id))
        })
        .or_else(|| {
            state.active_activity_session_id.as_ref().and_then(|sid| {
                state
                    .work_tabs
                    .iter()
                    .rev()
                    .find(|t| &t.session_id == sid)
                    .map(|t| t.id.clone())
            })
        });
    state.active_work_tab_id = active_tab;
    if state.active_work_tab_id.is_some() {
        state.load_active_tab();
    }
}

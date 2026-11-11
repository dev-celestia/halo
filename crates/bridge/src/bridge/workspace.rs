//! Intercept snapshot refresh, work tabs and activity (workspace) sessions.

use super::*;

impl Bridge {
    pub fn refresh_intercept(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let enabled = backend.intercept_enabled()?;
        let paused = backend.intercept_paused()?;
        Self::state(cx).update(cx, |s| {
            s.intercept.enabled = enabled;
            s.intercept.paused_count = paused.len();
            let active_id = s.active_work_tab_id.clone();
            let active_is_intercept = s.active_work_kind() == Some(WorkKind::Intercept);
            if active_is_intercept {
                let tab_id = active_id.as_deref();
                let still = s.selected_paused_id.as_ref().is_some_and(|selected| {
                    paused
                        .iter()
                        .any(|row| &row.id == selected && row.tab_id.as_deref() == tab_id)
                });
                if !still {
                    s.selected_paused_id = paused
                        .iter()
                        .find(|row| row.tab_id.as_deref() == tab_id)
                        .map(|row| row.id.clone());
                }
            }
            for tab in &mut s.work_tabs {
                if active_is_intercept && s.active_work_tab_id.as_deref() == Some(tab.id.as_str()) {
                    continue;
                }
                if let WorkData::Intercept(work) = &mut tab.data {
                    let tab_id = tab.id.as_str();
                    let still = work.selected_paused_id.as_ref().is_some_and(|selected| {
                        paused
                            .iter()
                            .any(|row| &row.id == selected && row.tab_id.as_deref() == Some(tab_id))
                    });
                    if !still {
                        work.selected_paused_id = paused
                            .iter()
                            .find(|row| row.tab_id.as_deref() == Some(tab_id))
                            .map(|row| row.id.clone());
                    }
                }
            }
            s.paused = paused;
            if active_is_intercept {
                s.commit_active_tab();
            }
            s.last_error = None;
        });
        Ok(())
    }

    pub(super) fn sync_active_intercept_scope(cx: &mut App) -> BridgeResult<()> {
        let (tab_id, patterns) = {
            let state = Self::state(cx).read(cx);
            if state.active_work_kind() != Some(WorkKind::Intercept) {
                return Ok(());
            }
            let Some(tab_id) = state.active_work_tab_id.clone() else {
                return Ok(());
            };
            (tab_id, state.intercept.capture_patterns.clone())
        };
        let backend = Self::backend(cx)?;
        backend.intercept_set_scope(&tab_id, &patterns)?;
        Ok(())
    }

    /// Open a new tool tab and make it active. Returns the tab id and kind.
    pub fn open_work_tab(kind: WorkKind, cx: &mut App) -> BridgeResult<(String, WorkKind)> {
        if !kind.ships() {
            return Err(BridgeError::message(format!(
                "{} is alpha and is not available in this build",
                kind.label()
            )));
        }
        Self::state(cx).update(cx, |s| {
            s.commit_active_tab();
            s.push_work_tab(kind);
            s.env_modal_open = false;
            s.env_modal_open = false;
            s.apply_ok_msg(format!("Opened {}", kind.label()));
        });
        if kind == WorkKind::Intercept {
            Self::sync_active_intercept_scope(cx)?;
        }
        if kind == WorkKind::HttpHistory {
            Self::bind_http_tab_session(cx)?;
        }
        if kind == WorkKind::Coder || kind == WorkKind::Agent || kind == WorkKind::SourceControl {
            let _ = Self::coder_restore_workspace(cx);
        }
        if kind == WorkKind::SourceControl {
            let _ = Self::git_refresh(cx);
        }
        if kind == WorkKind::Notes {
            let _ = Self::refresh_notes(cx);
        }
        if kind == WorkKind::Doctor {
            Self::doctor_refresh(cx);
        }
        Self::persist_shell_layout(cx);
        Ok((
            Self::state(cx)
                .read(cx)
                .active_work_tab_id
                .clone()
                .unwrap_or_default(),
            kind,
        ))
    }

    /// Focus the latest tab of `kind`, or create one.
    pub fn focus_or_open_work_tab(
        kind: WorkKind,
        cx: &mut App,
    ) -> BridgeResult<(String, WorkKind)> {
        if !kind.ships() {
            return Err(BridgeError::message(format!(
                "{} is alpha and is not available in this build",
                kind.label()
            )));
        }
        let existing = {
            let state = Self::state(cx).read(cx);
            let session_id = state.active_activity_session_id.clone();
            state
                .work_tabs
                .iter()
                .rev()
                .find(|tab| tab.kind == kind && session_id.as_ref() == Some(&tab.session_id))
                .map(|tab| tab.id.clone())
        };
        if let Some(id) = existing {
            Self::select_work_tab(&id, cx)?;
            Ok((id, kind))
        } else {
            Self::open_work_tab(kind, cx)
        }
    }

    pub fn select_work_tab(tab_id: &str, cx: &mut App) -> BridgeResult<WorkKind> {
        let kind = Self::state(cx)
            .read(cx)
            .work_tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.kind)
            .ok_or_else(|| BridgeError::message("Work tab not found"))?;
        Self::state(cx).update(cx, |s| {
            s.commit_active_tab();
            s.active_work_tab_id = Some(tab_id.to_string());
            s.load_active_tab();
            s.env_modal_open = false;
        });
        if kind == WorkKind::Intercept {
            Self::sync_active_intercept_scope(cx)?;
            let _ = Self::refresh_intercept(cx);
        }
        if kind == WorkKind::HttpHistory {
            Self::bind_http_tab_session(cx)?;
        }
        if kind == WorkKind::Coder || kind == WorkKind::Agent || kind == WorkKind::SourceControl {
            let _ = Self::coder_restore_workspace(cx);
        }
        if kind == WorkKind::SourceControl {
            let _ = Self::git_refresh(cx);
        }
        if kind == WorkKind::Notes {
            let _ = Self::refresh_notes(cx);
        }
        if kind == WorkKind::Doctor {
            Self::doctor_refresh(cx);
        }
        Self::persist_shell_layout(cx);
        Ok(kind)
    }

    /// Close a tab. `Ok(None)` means no tabs remain.
    pub fn close_work_tab(tab_id: &str, cx: &mut App) -> BridgeResult<Option<WorkKind>> {
        let kind = Self::state(cx)
            .read(cx)
            .work_tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.kind)
            .ok_or_else(|| BridgeError::message("Work tab not found"))?;
        if kind == WorkKind::Intercept {
            if let Ok(backend) = Self::backend(cx) {
                let _ = backend.intercept_forward_tab(tab_id);
            }
        }
        let closing_active = Self::state(cx).read(cx).active_work_tab_id.as_deref() == Some(tab_id);
        let next = Self::state(cx).update(cx, |s| {
            let closing_active = s.active_work_tab_id.as_deref() == Some(tab_id);
            if !closing_active {
                s.commit_active_tab();
            }
            let session_id = s
                .work_tabs
                .iter()
                .find(|tab| tab.id == tab_id)
                .map(|tab| tab.session_id.clone())
                .unwrap_or_default();
            let session_ids: Vec<String> = s
                .work_tabs
                .iter()
                .filter(|tab| tab.session_id == session_id)
                .map(|tab| tab.id.clone())
                .collect();
            let idx = session_ids.iter().position(|id| id == tab_id).unwrap_or(0);
            s.work_tabs.retain(|tab| tab.id != tab_id);
            s.paused.retain(|row| row.tab_id.as_deref() != Some(tab_id));
            s.intercept.paused_count = s.paused.len();
            let remaining: Vec<String> =
                session_ids.into_iter().filter(|id| id != tab_id).collect();
            if closing_active {
                s.env_modal_open = false;
                if remaining.is_empty() {
                    s.active_work_tab_id = None;
                    None
                } else {
                    let next = idx.min(remaining.len() - 1);
                    let next_id = remaining[next].clone();
                    s.active_work_tab_id = Some(next_id.clone());
                    s.load_active_tab();
                    s.work_tabs
                        .iter()
                        .find(|tab| tab.id == next_id)
                        .map(|tab| tab.kind)
                }
            } else {
                s.active_work_kind()
            }
        });
        if next == Some(WorkKind::Intercept) {
            Self::sync_active_intercept_scope(cx)?;
        }
        if closing_active && next == Some(WorkKind::HttpHistory) {
            Self::bind_http_tab_session(cx)?;
        }
        Self::persist_shell_layout(cx);
        Ok(next)
    }

    /// Close every session work tab except `keep_id`.
    pub fn close_other_work_tabs(keep_id: &str, cx: &mut App) -> BridgeResult<()> {
        let ids = Self::session_work_tab_ids(cx)
            .into_iter()
            .filter(|id| id != keep_id)
            .collect::<Vec<_>>();
        Self::close_work_tabs(&ids, cx)
    }

    /// Close session work tabs to the left of `tab_id`.
    pub fn close_work_tabs_to_left(tab_id: &str, cx: &mut App) -> BridgeResult<()> {
        let ids = Self::session_work_tab_ids(cx);
        let Some(pos) = ids.iter().position(|id| id == tab_id) else {
            return Ok(());
        };
        Self::close_work_tabs(&ids[..pos], cx)
    }

    /// Close session work tabs to the right of `tab_id`.
    pub fn close_work_tabs_to_right(tab_id: &str, cx: &mut App) -> BridgeResult<()> {
        let ids = Self::session_work_tab_ids(cx);
        let Some(pos) = ids.iter().position(|id| id == tab_id) else {
            return Ok(());
        };
        Self::close_work_tabs(&ids[pos + 1..], cx)
    }

    /// Close every work tab in the active session.
    pub fn close_all_work_tabs(cx: &mut App) -> BridgeResult<()> {
        let ids = Self::session_work_tab_ids(cx);
        Self::close_work_tabs(&ids, cx)
    }

    fn session_work_tab_ids(cx: &App) -> Vec<String> {
        Self::state(cx)
            .read(cx)
            .session_work_tabs()
            .into_iter()
            .map(|tab| tab.id.clone())
            .collect()
    }

    fn close_work_tabs(ids: &[String], cx: &mut App) -> BridgeResult<()> {
        for id in ids {
            let _ = Self::close_work_tab(id, cx);
        }
        Ok(())
    }

    /// Start a blank session. The current tabs and captured activity stay on the previous one.
    pub fn create_activity_session(cx: &mut App) -> BridgeResult<String> {
        let id = Self::state(cx).update(cx, |s| {
            s.ensure_activity_session();
            s.park_active_session();
            let id = uuid::Uuid::new_v4().to_string();
            let title = format!("Session {}", s.activity_sessions.len() + 1);
            s.activity_sessions.push(ActivitySession {
                id: id.clone(),
                title: title.clone(),
                active_work_tab_id: None,
                history: Vec::new(),
            });
            s.show_activity_session(&id);
            s.apply_ok_msg(format!("Started {title}"));
            id
        });
        Self::persist_shell_layout(cx);
        Ok(id)
    }

    /// Show a session and the activity it was holding.
    pub fn select_activity_session(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        let kind = {
            let known = Self::state(cx)
                .read(cx)
                .activity_sessions
                .iter()
                .any(|session| session.id == session_id);
            if !known {
                return Err(BridgeError::message("Session not found"));
            }
            Self::state(cx).update(cx, |s| {
                if s.active_activity_session_id.as_deref() == Some(session_id) {
                    return s.active_work_kind();
                }
                s.park_active_session();
                s.show_activity_session(session_id);
                s.active_work_kind()
            })
        };
        if kind == Some(WorkKind::HttpHistory) {
            Self::bind_http_tab_session(cx)?;
        }
        if kind == Some(WorkKind::Intercept) {
            Self::sync_active_intercept_scope(cx)?;
            let _ = Self::refresh_intercept(cx);
        }
        Self::persist_shell_layout(cx);
        Ok(())
    }

    /// Remove a session and the work tabs it owned. Keeps at least one session.
    pub fn remove_activity_session(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        let (title, tab_ids, was_active, fallback_id) = {
            let state = Self::state(cx).read(cx);
            if state.activity_sessions.len() <= 1 {
                return Err(BridgeError::message("Keep at least one session"));
            }
            let Some(session) = state
                .activity_sessions
                .iter()
                .find(|session| session.id == session_id)
            else {
                return Err(BridgeError::message("Session not found"));
            };
            let title = session.title.clone();
            let tab_ids: Vec<String> = state
                .work_tabs
                .iter()
                .filter(|tab| tab.session_id == session_id)
                .map(|tab| tab.id.clone())
                .collect();
            let was_active = state.active_activity_session_id.as_deref() == Some(session_id);
            let fallback_id = state
                .activity_sessions
                .iter()
                .find(|session| session.id != session_id)
                .map(|session| session.id.clone());
            (title, tab_ids, was_active, fallback_id)
        };

        if was_active {
            if let Some(fallback_id) = fallback_id.as_deref() {
                Self::select_activity_session(fallback_id, cx)?;
            }
        }

        Self::close_work_tabs(&tab_ids, cx)?;

        Self::state(cx).update(cx, |s| {
            s.activity_sessions
                .retain(|session| session.id != session_id);
            if s.active_activity_session_id.as_deref() == Some(session_id) {
                if let Some(fallback_id) = fallback_id.as_deref() {
                    s.show_activity_session(fallback_id);
                } else {
                    s.ensure_activity_session();
                }
            }
            s.apply_ok_msg(format!("Removed {title}"));
        });

        let kind = Self::state(cx).read(cx).active_work_kind();
        if kind == Some(WorkKind::HttpHistory) {
            Self::bind_http_tab_session(cx)?;
        }
        if kind == Some(WorkKind::Intercept) {
            Self::sync_active_intercept_scope(cx)?;
            let _ = Self::refresh_intercept(cx);
        }
        Self::persist_shell_layout(cx);
        Ok(())
    }

    pub(super) fn persist_shell_layout(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let layout = Self::state(cx).read(cx).shell_layout_snapshot();
        let _ = backend.shell_save_layout(&layout);
        Self::coder_persist_open_tabs(cx);
    }
}

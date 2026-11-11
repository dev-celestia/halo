//! HTTP capture sessions and cross-tool hand-off (repeater / intruder / intercept).

use super::*;

impl Bridge {
    /// Bind the HTTP tab to a disk-backed capture session so traffic survives restarts.
    /// Reuses the active (or latest) SQLite session instead of creating a fresh empty one.
    pub(super) fn bind_http_tab_session(cx: &mut App) -> BridgeResult<()> {
        let (title, session_id) = {
            let state = Self::state(cx).read(cx);
            if state.active_work_kind() != Some(WorkKind::HttpHistory) {
                return Ok(());
            }
            let title = state
                .active_work_tab()
                .map(|tab| tab.title.clone())
                .unwrap_or_else(|| "HTTP".into());
            (title, state.history_filter.session_id.clone())
        };
        let backend = Self::backend(cx)?;
        if let Some(id) = session_id {
            backend.session_set_active(&id)?;
        } else {
            let sessions = backend.sessions_list().unwrap_or_default();
            let reused = sessions
                .iter()
                .find(|s| s.is_active)
                .or_else(|| sessions.first())
                .map(|s| s.id.clone());
            if let Some(id) = reused {
                backend.session_set_active(&id)?;
                Self::state(cx).update(cx, |s| {
                    s.history_filter.session_id = Some(id);
                    s.commit_active_tab();
                });
            } else {
                let created = backend.session_create(&title)?;
                Self::state(cx).update(cx, |s| {
                    s.history_filter.session_id = Some(created.id);
                    s.commit_active_tab();
                });
            }
        }
        if let Ok(sessions) = backend.sessions_list() {
            Self::state(cx).update(cx, |s| s.sessions = sessions);
        }
        Self::refresh_history(cx)
    }

    pub fn refresh_sessions(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.sessions_list() {
            Ok(sessions) => {
                Self::state(cx).update(cx, |s| {
                    s.sessions = sessions;
                    s.last_error = None;
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn create_session(name: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.session_create(name) {
            Ok(_) => {
                let _ = Self::refresh_sessions(cx);
                Self::state(cx).update(cx, |s| s.apply_ok_msg("Session created".into()));
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn set_active_session(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.session_set_active(session_id) {
            Ok(()) => {
                Self::state(cx).update(cx, |s| {
                    s.history_filter.session_id = Some(session_id.to_string());
                });
                let _ = Self::refresh_sessions(cx);
                Self::refresh_history(cx)?;
                Self::state(cx).update(cx, |s| s.apply_ok_msg("Session activated".into()));
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn send_selected_to_repeater(cx: &mut App) -> BridgeResult<()> {
        let detail = Self::state(cx)
            .read(cx)
            .selected_detail
            .clone()
            .ok_or_else(|| BridgeError::message("Select a request first"))?;
        let request = RepeaterRequestDto {
            method: detail.method,
            url: detail.url,
            headers: detail.req_headers,
            body: detail.req_body,
        };
        Self::state(cx).update(cx, |s| {
            s.pending_repeater = Some(request);
            s.apply_ok_msg("Loaded into Repeater".into());
        });
        Ok(())
    }

    pub fn send_selected_to_intruder(cx: &mut App) -> BridgeResult<()> {
        let detail = Self::state(cx)
            .read(cx)
            .selected_detail
            .clone()
            .ok_or_else(|| BridgeError::message("Select a request first"))?;
        let headers_text = detail
            .req_headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        let draft = IntruderDraft {
            method: detail.method,
            url: detail.url,
            headers_text,
            body: detail.req_body,
            payloads_text: String::new(),
            ..IntruderDraft::default()
        };
        Self::state(cx).update(cx, |s| {
            s.pending_intruder = Some(draft);
            s.apply_ok_msg("Loaded into Intruder".into());
        });
        Ok(())
    }

    pub fn send_selected_to_intercept(cx: &mut App) -> BridgeResult<()> {
        let host = Self::state(cx)
            .read(cx)
            .selected_detail
            .as_ref()
            .and_then(|d| {
                d.req_headers
                    .iter()
                    .find(|(k, _)| {
                        k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case(":authority")
                    })
                    .map(|(_, v)| v.clone())
                    .or_else(|| {
                        d.url
                            .split("://")
                            .nth(1)
                            .unwrap_or(&d.url)
                            .split('/')
                            .next()
                            .map(|h| h.split(':').next().unwrap_or(h).to_string())
                    })
            })
            .ok_or_else(|| BridgeError::message("Select a request first"))?;
        Self::add_intercept_tab_for_host(&host, cx)
    }

    /// Add the selected (or given) host to intercept capture scope — doc "Add to Scope".
    pub fn add_history_host_to_scope(host: &str, cx: &mut App) -> BridgeResult<()> {
        Self::add_intercept_tab_for_host(host, cx)
    }

    /// Load detail for `log_id` and return it (used by Copy as cURL).
    pub fn history_detail(log_id: &str, cx: &mut App) -> BridgeResult<HttpLogDetail> {
        Self::select_history_row(Some(log_id.to_string()), cx)?;
        Self::state(cx)
            .read(cx)
            .selected_detail
            .clone()
            .ok_or_else(|| BridgeError::message("Failed to load request detail"))
    }

    pub fn take_pending_repeater(cx: &mut App) -> Option<RepeaterRequestDto> {
        let mut out = None;
        Self::state(cx).update(cx, |s| out = s.pending_repeater.take());
        out
    }

    pub fn take_pending_intruder(cx: &mut App) -> Option<IntruderDraft> {
        let mut out = None;
        Self::state(cx).update(cx, |s| out = s.pending_intruder.take());
        out
    }

    pub fn select_paused(id: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.selected_paused_id = id;
        });
    }
}

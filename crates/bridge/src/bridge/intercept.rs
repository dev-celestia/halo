//! Intercept pause queue, scope rules, forwarding and CA trust helpers.

use super::util::*;
use super::*;

impl Bridge {
    pub fn add_intercept_tab_for_host(host: &str, cx: &mut App) -> BridgeResult<()> {
        let normalized = normalize_intercept_host(host);
        if normalized.is_empty() {
            return Err(BridgeError::message("Host is empty"));
        }

        let existing = Self::state(cx).update(cx, |s| {
            s.commit_active_tab();
            let session_id = s.active_activity_session_id.clone();
            s.work_tabs.iter().find_map(|tab| {
                if session_id.as_ref() != Some(&tab.session_id) {
                    return None;
                }
                let WorkData::Intercept(work) = &tab.data else {
                    return None;
                };
                work.capture_hosts
                    .iter()
                    .any(|pattern| capture_pattern_matches_host(pattern, &normalized))
                    .then(|| tab.id.clone())
            })
        });

        if let Some(id) = existing {
            Self::select_work_tab(&id, cx)?;
        } else if Self::state(cx).read(cx).active_work_kind() == Some(WorkKind::Intercept) {
            Self::state(cx).update(cx, |s| {
                if !s
                    .intercept
                    .capture_patterns
                    .iter()
                    .any(|pattern| pattern == &normalized)
                {
                    s.intercept.capture_patterns.push(normalized.clone());
                }
                s.commit_active_tab();
            });
        } else {
            Self::open_work_tab(WorkKind::Intercept, cx)?;
            Self::state(cx).update(cx, |s| {
                s.intercept.capture_patterns = vec![normalized.clone()];
                if let Some(tab) = s
                    .work_tabs
                    .iter_mut()
                    .find(|tab| s.active_work_tab_id.as_deref() == Some(tab.id.as_str()))
                {
                    tab.title = normalized.clone();
                }
                s.commit_active_tab();
            });
        }

        let backend = Self::backend(cx)?;
        let _ = backend.set_intercept_enabled(true);
        Self::state(cx).update(cx, |s| {
            s.intercept.enabled = true;
            s.apply_ok_msg(format!("Intercepting {normalized}"));
        });
        Self::sync_active_intercept_scope(cx)?;
        let _ = Self::refresh_intercept(cx);
        Ok(())
    }

    pub fn forward_paused_modified(
        id: &str,
        raw_text: &str,
        intercept_response: bool,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.intercept_forward_modified(id, raw_text, intercept_response) {
            Ok(()) => {
                let _ = Self::refresh_intercept(cx);
                Self::state(cx).update(cx, |s| {
                    s.apply_ok_msg(if intercept_response {
                        "Forwarded (response interception enabled)".into()
                    } else {
                        "Forwarded".into()
                    });
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn add_intercept_scope(pattern: &str, cx: &mut App) -> BridgeResult<()> {
        let normalized = normalize_intercept_host(pattern);
        if normalized.is_empty() {
            return Err(BridgeError::message("Scope pattern is empty"));
        }
        Self::state(cx).update(cx, |s| {
            if s.active_work_kind() == Some(WorkKind::Intercept)
                && !s
                    .intercept
                    .capture_patterns
                    .iter()
                    .any(|pattern| pattern == &normalized)
            {
                s.intercept.capture_patterns.push(normalized.clone());
            }
            s.commit_active_tab();
            s.apply_ok_msg(format!("Added scope pattern: {normalized}"));
        });
        Self::sync_active_intercept_scope(cx)
    }

    pub fn remove_intercept_scope(pattern: &str, cx: &mut App) -> BridgeResult<()> {
        let normalized = normalize_intercept_host(pattern);
        Self::state(cx).update(cx, |s| {
            if s.active_work_kind() == Some(WorkKind::Intercept) {
                s.intercept
                    .capture_patterns
                    .retain(|pattern| !capture_pattern_matches_host(pattern, &normalized));
            }
            s.commit_active_tab();
            s.apply_ok_msg(format!("Removed scope pattern: {normalized}"));
        });
        Self::sync_active_intercept_scope(cx)
    }

    pub fn dont_capture_host(id: &str, host: &str, cx: &mut App) -> BridgeResult<()> {
        let _ = Self::remove_intercept_scope(host, cx);
        let backend = Self::backend(cx)?;
        let _ = backend.intercept_forward(id);
        let _ = Self::refresh_intercept(cx);
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg(format!("Removed {host} from capture scope and forwarded"));
        });
        Ok(())
    }

    pub fn send_paused_to_repeater(id: &str, cx: &mut App) -> BridgeResult<()> {
        let state = Self::state(cx).read(cx);
        let paused = state
            .paused
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| BridgeError::message("Paused request not found"))?;

        let (method, url, headers, body) =
            crate::http_format::parse_raw_request(&paused.raw_request, &paused.url);
        let request = RepeaterRequestDto {
            method,
            url,
            headers,
            body,
        };

        let _ = state;
        Self::state(cx).update(cx, |s| {
            s.pending_repeater = Some(request);
            s.apply_ok_msg("Loaded into Repeater".into());
        });
        Ok(())
    }

    pub fn open_intercept_browser(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.intercept_open_browser() {
            Ok(()) => {
                Self::state(cx).update(cx, |s| {
                    s.apply_ok_msg("Managed Chrome browser launched with proxy & CA trust".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn trust_intercept_ca(cx: &mut App) -> BridgeResult<String> {
        let backend = Self::backend(cx)?;
        match backend.intercept_trust_ca() {
            Ok(msg) => {
                Self::state(cx).update(cx, |s| {
                    s.apply_ok_msg(msg.clone());
                });
                Ok(msg)
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn set_intercept_enabled(enabled: bool, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.set_intercept_enabled(enabled) {
            Ok(()) => {
                let _ = Self::refresh_intercept(cx);
                Self::state(cx).update(cx, |s| {
                    s.apply_ok_msg(if enabled {
                        "Intercept enabled".into()
                    } else {
                        "Intercept disabled".into()
                    });
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn forward_paused(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.intercept_forward(id) {
            Ok(()) => {
                let _ = Self::refresh_intercept(cx);
                Self::state(cx).update(cx, |s| s.apply_ok_msg("Forwarded".into()));
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn drop_paused(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.intercept_drop(id) {
            Ok(()) => {
                let _ = Self::refresh_intercept(cx);
                Self::state(cx).update(cx, |s| s.apply_ok_msg("Dropped".into()));
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn forward_all_paused(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let tab_id = Self::state(cx)
            .read(cx)
            .active_work_tab_id
            .clone()
            .unwrap_or_default();
        match backend.intercept_forward_tab(&tab_id) {
            Ok(n) => {
                let _ = Self::refresh_intercept(cx);
                Self::state(cx).update(cx, |s| {
                    s.apply_ok_msg(format!("Forwarded {n} from active tab"));
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn drop_all_paused(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let tab_id = Self::state(cx)
            .read(cx)
            .active_work_tab_id
            .clone()
            .unwrap_or_default();
        let ids: Vec<String> = Self::state(cx)
            .read(cx)
            .paused
            .iter()
            .filter(|p| p.tab_id.as_deref() == Some(tab_id.as_str()))
            .map(|p| p.id.clone())
            .collect();
        for id in &ids {
            let _ = backend.intercept_drop(id);
        }
        let _ = Self::refresh_intercept(cx);
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg(format!("Dropped {} from active tab", ids.len()));
        });
        Ok(())
    }
}

//! Proxy lifecycle commands and HTTP-history query/filter/selection commands.

use super::util::*;
use super::*;

impl Bridge {
    /// Start the proxy asynchronously. Sets [`BridgeState::proxy_busy`] while waiting
    /// so the UI can show a loading state instead of freezing until timeout.
    pub fn start_proxy(port: u16, tls_port: u16, cx: &mut App) -> BridgeResult<String> {
        let backend = Self::backend(cx)?;
        let state = Self::state(cx);
        if state.read(cx).proxy_busy {
            return Ok("Proxy start already in progress".into());
        }
        if state.read(cx).proxy.running {
            return Ok(format!(
                "Proxy already running on port {}",
                state.read(cx).proxy.port.unwrap_or(port)
            ));
        }

        state.update(cx, |s| {
            s.proxy_busy = true;
            s.last_error = None;
            s.last_message = Some("Starting proxy…".into());
        });

        Self::run_blocking(
            cx,
            {
                let backend = backend.clone();
                move || {
                    let msg = backend.start_proxy(port, tls_port)?;
                    Ok((backend.proxy_status(), msg))
                }
            },
            |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.proxy_busy = false;
                    match result {
                        Ok((status, msg)) => s.apply_ok_proxy(status, msg),
                        Err(err) => s.apply_err(err),
                    }
                });
            },
        );

        Ok("Starting proxy…".into())
    }

    /// Stop the proxy asynchronously (same loading pattern as start).
    pub fn stop_proxy(cx: &mut App) -> BridgeResult<String> {
        let backend = Self::backend(cx)?;
        let state = Self::state(cx);
        if state.read(cx).proxy_busy {
            return Ok("Proxy stop already in progress".into());
        }
        if !state.read(cx).proxy.running {
            return Err(BridgeError::message("Proxy is not running"));
        }

        state.update(cx, |s| {
            s.proxy_busy = true;
            s.last_error = None;
            s.last_message = Some("Stopping proxy…".into());
        });

        Self::run_blocking(
            cx,
            {
                let backend = backend.clone();
                move || {
                    let msg = backend.stop_proxy()?;
                    Ok((backend.proxy_status(), msg))
                }
            },
            |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.proxy_busy = false;
                    match result {
                        Ok((status, msg)) => s.apply_ok_proxy(status, msg),
                        Err(err) => s.apply_err(err),
                    }
                });
            },
        );

        Ok("Stopping proxy…".into())
    }

    pub fn refresh_history(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let filter = Self::state(cx).read(cx).history_filter.clone();
        match backend.history_recent(HISTORY_LIMIT, Some(&filter)) {
            Ok(rows) => {
                Self::state(cx).update(cx, |s| {
                    s.history = rows;
                    s.history_pending_count = 0;
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

    /// Pause/resume the live history stream while a row context menu is open.
    pub fn set_history_context_menu_open(open: bool, cx: &mut App) {
        let should_flush = Self::state(cx).update(cx, |s| {
            s.history_context_menu_open = open;
            !open && !s.history_stream_paused() && s.history_pending_count > 0
        });
        if should_flush {
            let _ = Self::refresh_history(cx);
        }
    }

    pub fn set_history_search(search: String, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.history_filter.search = if search.trim().is_empty() {
                None
            } else {
                Some(search)
            };
        });
        Self::refresh_history(cx)
    }

    pub fn toggle_method_filter(method: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            let methods = &mut s.history_filter.methods;
            if let Some(pos) = methods.iter().position(|m| m.eq_ignore_ascii_case(method)) {
                methods.remove(pos);
            } else {
                methods.push(method.to_ascii_uppercase());
            }
        });
        Self::refresh_history(cx)
    }

    pub fn toggle_status_class(class: u16, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            let classes = &mut s.history_filter.status_classes;
            if let Some(pos) = classes.iter().position(|c| *c == class) {
                classes.remove(pos);
            } else {
                classes.push(class);
            }
        });
        Self::refresh_history(cx)
    }

    pub fn toggle_filter_chip(chip: FilterChip, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| match chip {
            FilterChip::Hide304 => s.history_filter.hide_304 = !s.history_filter.hide_304,
            FilterChip::HideImages => s.history_filter.hide_images = !s.history_filter.hide_images,
            FilterChip::OnlyApi => s.history_filter.only_api = !s.history_filter.only_api,
            FilterChip::ErrorsOnly => s.history_filter.errors_only = !s.history_filter.errors_only,
        });
        Self::refresh_history(cx)
    }

    pub fn clear_history_filters(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            let session = s.history_filter.session_id.clone();
            s.history_filter = HistoryFilter {
                session_id: session,
                ..HistoryFilter::default()
            };
        });
        Self::refresh_history(cx)
    }

    pub fn select_history_row(log_id: Option<String>, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match log_id {
            None => {
                let should_flush = Self::state(cx).update(cx, |s| {
                    s.selected_log_id = None;
                    s.selected_detail = None;
                    !s.history_stream_paused() && s.history_pending_count > 0
                });
                if should_flush {
                    Self::refresh_history(cx)?;
                }
                Ok(())
            }
            Some(id) => match backend.history_detail(&id) {
                Ok(detail) => {
                    Self::state(cx).update(cx, |s| {
                        s.selected_log_id = Some(id);
                        s.selected_detail = Some(detail);
                        s.last_error = None;
                    });
                    Ok(())
                }
                Err(err) => {
                    Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                    Err(err)
                }
            },
        }
    }

    pub fn set_detail_tab(tab: DetailTab, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.detail_tab = tab);
    }

    pub fn set_detail_side(side: DetailSide, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.detail_side = side);
    }

    pub fn delete_history_row(log_id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.history_delete(log_id) {
            Ok(()) => {
                let selected = Self::state(cx).read(cx).selected_log_id.clone();
                Self::state(cx).update(cx, |s| {
                    s.history.retain(|r| r.id != log_id);
                    if selected.as_deref() == Some(log_id) {
                        s.selected_log_id = None;
                        s.selected_detail = None;
                    }
                    s.apply_ok_msg("Deleted".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn clear_history(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let session_id = Self::state(cx).read(cx).history_filter.session_id.clone();
        let cleared = if let Some(session_id) = session_id.as_deref() {
            backend.session_clear_logs(session_id).map(|_| ())
        } else {
            backend.history_clear()
        };
        match cleared {
            Ok(()) => {
                Self::state(cx).update(cx, |s| {
                    s.history.clear();
                    s.selected_log_id = None;
                    s.selected_detail = None;
                    s.history_pending_count = 0;
                    s.history_context_menu_open = false;
                    s.commit_active_tab();
                    s.apply_ok_msg("History cleared".into());
                });
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }
}

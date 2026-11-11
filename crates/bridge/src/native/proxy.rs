//! Proxy lifecycle, HTTP history, and session accessors.

use super::*;

impl BackendProxy for NativeBackend {
    fn proxy_status(&self) -> ProxyStatus {
        // `Backend::proxy_status` is infallible: a probe failure means "not
        // running", which is exactly what the caller wants to render.
        match self
            .tokio
            .block_on(commands::proxy::get_proxy_status(&self.runtime))
        {
            Ok(status) => ProxyStatus {
                running: status.running,
                port: status.port,
                tls_port: commands::proxy::get_proxy_tls_port(&self.runtime),
                default_port: status.default_port,
                connections: status.connections,
            },
            Err(_) => ProxyStatus {
                running: false,
                port: None,
                tls_port: None,
                default_port: halo_backend::default_proxy_port(),
                connections: 0,
            },
        }
    }

    fn start_proxy(&self, port: u16, tls_port: u16) -> BridgeResult<String> {
        self.tokio
            .block_on(commands::proxy::start_proxy(&self.runtime, port, tls_port))
            .map_err(BridgeError::message)
    }

    fn stop_proxy(&self) -> BridgeResult<String> {
        self.tokio
            .block_on(commands::proxy::stop_proxy(&self.runtime))
            .map_err(BridgeError::message)
    }
}

impl BackendHistory for NativeBackend {
    fn history_recent(
        &self,
        limit: u32,
        filter: Option<&HistoryFilter>,
    ) -> BridgeResult<Vec<HttpLogRow>> {
        let proxy_filter = filter.map(to_proxy_filter);
        let rows = self
            .tokio
            .block_on(commands::history::get_proxy_recent(
                &self.runtime,
                Some(limit),
                proxy_filter,
                Some("desc".into()),
            ))
            .map_err(BridgeError::message)?;
        let mut rows: Vec<_> = rows.into_iter().map(summary_to_row).collect();
        if let Some(filter) = filter {
            rows = apply_chip_filters(rows, filter);
        }
        Ok(rows)
    }

    fn history_detail(&self, log_id: &str) -> BridgeResult<HttpLogDetail> {
        let record = self
            .tokio
            .block_on(commands::history::get_proxy_detail(
                &self.runtime,
                log_id.to_string(),
            ))
            .map_err(BridgeError::message)?;
        Ok(record_to_detail(record))
    }

    fn history_delete(&self, log_id: &str) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::history::delete_proxy_by_id(
                &self.runtime,
                log_id.to_string(),
            ))
            .map_err(BridgeError::message)
    }

    fn history_clear(&self) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::history::clear_proxy_all(&self.runtime))
            .map_err(BridgeError::message)
    }

    fn sessions_list(&self) -> BridgeResult<Vec<HttpSessionDto>> {
        let sessions = self
            .tokio
            .block_on(commands::sessions::list_http_sessions(&self.runtime))
            .map_err(BridgeError::message)?;
        Ok(sessions
            .into_iter()
            .map(|s| HttpSessionDto {
                id: s.id,
                name: s.name,
                is_active: s.is_active,
                request_count: s.request_count,
                capture_mode: s.capture_mode,
            })
            .collect())
    }

    fn session_create(&self, name: &str) -> BridgeResult<HttpSessionDto> {
        let rec = self
            .tokio
            .block_on(commands::sessions::create_http_session(
                &self.runtime,
                name.to_string(),
                None,
                Some("all".to_string()),
                None,
                None,
                Some("persistent".to_string()),
            ))
            .map_err(BridgeError::message)?;
        Ok(HttpSessionDto {
            id: rec.id,
            name: rec.name,
            is_active: rec.is_active,
            request_count: 0,
            capture_mode: rec.capture_mode,
        })
    }

    fn session_set_active(&self, session_id: &str) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::sessions::set_active_http_session(
                &self.runtime,
                session_id.to_string(),
            ))
            .map_err(BridgeError::message)
    }

    fn session_clear_logs(&self, session_id: &str) -> BridgeResult<usize> {
        self.tokio
            .block_on(commands::sessions::clear_http_session_logs(
                &self.runtime,
                session_id.to_string(),
            ))
            .map_err(BridgeError::message)
    }
}

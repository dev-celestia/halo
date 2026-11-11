//! Stub HTTP history and session state.

use super::*;

impl BackendHistory for StubBackend {
    fn history_recent(
        &self,
        limit: u32,
        filter: Option<&HistoryFilter>,
    ) -> BridgeResult<Vec<HttpLogRow>> {
        let rows = lock_err(self.history.lock());
        let mut out: Vec<_> = rows.iter().take(limit as usize).cloned().collect();
        if let Some(filter) = filter {
            if let Some(search) = filter.search.as_ref().map(|s| s.to_ascii_lowercase()) {
                if !search.is_empty() {
                    out.retain(|r| {
                        r.url.to_ascii_lowercase().contains(&search)
                            || r.host
                                .as_ref()
                                .map(|h| h.to_ascii_lowercase().contains(&search))
                                .unwrap_or(false)
                    });
                }
            }
            if !filter.methods.is_empty() {
                out.retain(|r| {
                    filter
                        .methods
                        .iter()
                        .any(|m| m.eq_ignore_ascii_case(&r.method))
                });
            }
            out = crate::native::apply_chip_filters(out, filter);
        }
        Ok(out)
    }

    fn history_detail(&self, log_id: &str) -> BridgeResult<HttpLogDetail> {
        let rows = lock_err(self.history.lock());
        let row = rows
            .iter()
            .find(|r| r.id == log_id)
            .ok_or_else(|| BridgeError::message(format!("Log not found: {log_id}")))?;
        let mut req_headers = HashMap::new();
        req_headers.insert("host".into(), row.host.clone().unwrap_or_default());
        req_headers.insert(
            "user-agent".into(),
            row.user_agent.clone().unwrap_or_else(|| "stub".into()),
        );
        let mut res_headers = HashMap::new();
        if let Some(ct) = &row.content_type {
            res_headers.insert("content-type".into(), ct.clone());
        }
        Ok(HttpLogDetail {
            id: row.id.clone(),
            timestamp: row.timestamp.clone(),
            client_addr: "127.0.0.1:0".into(),
            server_addr: row.server_addr.clone(),
            method: row.method.clone(),
            url: row.url.clone(),
            req_http_version: "HTTP/1.1".into(),
            req_headers,
            req_body: String::new(),
            req_body_bytes: row.request_size,
            res_status: row.status,
            res_status_text: row.status_text.clone(),
            res_http_version: Some("HTTP/1.1".into()),
            res_headers,
            res_body: "{\"ok\":true,\"source\":\"stub\"}".into(),
            res_body_bytes: row.response_size,
        })
    }

    fn history_delete(&self, log_id: &str) -> BridgeResult<()> {
        let mut rows = lock_err(self.history.lock());
        let before = rows.len();
        rows.retain(|r| r.id != log_id);
        if rows.len() == before {
            Err(BridgeError::message("Log not found"))
        } else {
            Ok(())
        }
    }

    fn history_clear(&self) -> BridgeResult<()> {
        lock_err(self.history.lock()).clear();
        Ok(())
    }

    fn sessions_list(&self) -> BridgeResult<Vec<HttpSessionDto>> {
        Ok(lock_err(self.sessions.lock()).clone())
    }

    fn session_create(&self, name: &str) -> BridgeResult<HttpSessionDto> {
        let mut sessions = lock_err(self.sessions.lock());
        for s in sessions.iter_mut() {
            s.is_active = false;
        }
        let dto = HttpSessionDto {
            id: format!("stub-{}", now_stamp()),
            name: name.to_string(),
            is_active: true,
            request_count: 0,
            capture_mode: "all".into(),
        };
        sessions.push(dto.clone());
        Ok(dto)
    }

    fn session_set_active(&self, session_id: &str) -> BridgeResult<()> {
        let mut sessions = lock_err(self.sessions.lock());
        if !sessions.iter().any(|s| s.id == session_id) {
            return Err(BridgeError::message("Session not found"));
        }
        for s in sessions.iter_mut() {
            s.is_active = s.id == session_id;
        }
        Ok(())
    }

    fn session_clear_logs(&self, _session_id: &str) -> BridgeResult<usize> {
        let mut rows = lock_err(self.history.lock());
        let n = rows.len();
        rows.clear();
        Ok(n)
    }
}

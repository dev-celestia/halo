//! Stub intercept pause / forward / drop state.

use super::*;

impl BackendIntercept for StubBackend {
    fn intercept_enabled(&self) -> BridgeResult<bool> {
        Ok(*lock_err(self.intercept_enabled.lock()))
    }

    fn set_intercept_enabled(&self, enabled: bool) -> BridgeResult<()> {
        *lock_err(self.intercept_enabled.lock()) = enabled;
        if enabled {
            let mut paused = lock_err(self.paused.lock());
            if paused.is_empty() {
                let tab_id = lock_err(self.active_tab_id.lock()).clone();
                paused.push(PausedRow {
                    id: "stub-paused-1".into(),
                    method: "GET".into(),
                    url: "https://example.com/login".into(),
                    host: "example.com".into(),
                    path: "/login".into(),
                    has_response: false,
                    status_code: None,
                    status_text: None,
                    raw_request: "GET /login HTTP/1.1\r\nHost: example.com\r\n\r\n".into(),
                    raw_response: None,
                    timestamp: now_stamp(),
                    tab_id: Some(tab_id),
                });
            }
        }
        Ok(())
    }

    fn intercept_paused(&self) -> BridgeResult<Vec<PausedRow>> {
        Ok(lock_err(self.paused.lock()).clone())
    }

    fn intercept_scope(&self) -> BridgeResult<Vec<String>> {
        Ok(lock_err(self.scope.lock()).clone())
    }

    fn intercept_set_scope(&self, tab_id: &str, patterns: &[String]) -> BridgeResult<()> {
        *lock_err(self.active_tab_id.lock()) = tab_id.to_string();
        *lock_err(self.scope.lock()) = patterns
            .iter()
            .map(|p| p.trim().to_lowercase())
            .filter(|p| !p.is_empty())
            .collect();
        Ok(())
    }

    fn intercept_add_scope(&self, pattern: &str) -> BridgeResult<Vec<String>> {
        let mut s = lock_err(self.scope.lock());
        let pat = pattern.trim().to_lowercase();
        if !pat.is_empty() && !s.contains(&pat) {
            s.push(pat);
        }
        Ok(s.clone())
    }

    fn intercept_remove_scope(&self, pattern: &str) -> BridgeResult<Vec<String>> {
        let mut s = lock_err(self.scope.lock());
        let pat = pattern.trim().to_lowercase();
        s.retain(|p| p != &pat);
        Ok(s.clone())
    }

    fn intercept_forward_tab(&self, tab_id: &str) -> BridgeResult<usize> {
        let mut paused = lock_err(self.paused.lock());
        let before = paused.len();
        paused.retain(|row| row.tab_id.as_deref() != Some(tab_id));
        Ok(before.saturating_sub(paused.len()))
    }

    fn intercept_forward(&self, id: &str) -> BridgeResult<()> {
        let mut paused = lock_err(self.paused.lock());
        let before = paused.len();
        paused.retain(|row| row.id != id);
        if paused.len() == before {
            Err(BridgeError::message("Paused request not found"))
        } else {
            Ok(())
        }
    }

    fn intercept_forward_modified(
        &self,
        id: &str,
        _raw_text: &str,
        _intercept_response: bool,
    ) -> BridgeResult<()> {
        self.intercept_forward(id)
    }

    fn intercept_drop(&self, id: &str) -> BridgeResult<()> {
        self.intercept_forward(id)
    }

    fn intercept_forward_all(&self) -> BridgeResult<()> {
        lock_err(self.paused.lock()).clear();
        Ok(())
    }

    fn intercept_drop_all(&self) -> BridgeResult<()> {
        self.intercept_forward_all()
    }

    fn intercept_add_host_tab(&self, host: &str) -> BridgeResult<()> {
        let _ = self.intercept_add_scope(host);
        *lock_err(self.intercept_enabled.lock()) = true;
        Ok(())
    }

    fn intercept_open_browser(&self) -> BridgeResult<()> {
        Ok(())
    }

    fn intercept_trust_ca(&self) -> BridgeResult<String> {
        Ok("Halo CA trusted (stub)".into())
    }
}

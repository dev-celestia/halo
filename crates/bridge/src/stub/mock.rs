//! Stub MockForge servers/routes and port-scanner accessors.

use super::*;

impl BackendMock for StubBackend {
    fn mock_domains(&self) -> BridgeResult<Vec<MockDomainDto>> {
        Ok(lock_err(self.mock_domains.lock()).clone())
    }

    fn mock_add_domain(&self, hostname: &str, ssl: bool) -> BridgeResult<MockDomainDto> {
        let domain = MockDomainDto {
            id: uuid::Uuid::new_v4().to_string(),
            hostname: hostname.trim().to_lowercase(),
            ssl,
            status: "active".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        lock_err(self.mock_domains.lock()).push(domain.clone());
        Ok(domain)
    }

    fn mock_toggle_domain(&self, id: &str) -> BridgeResult<()> {
        let mut domains = lock_err(self.mock_domains.lock());
        if let Some(d) = domains.iter_mut().find(|d| d.id == id) {
            d.status = if d.status == "active" {
                "inactive"
            } else {
                "active"
            }
            .to_string();
        }
        Ok(())
    }

    fn mock_delete_domain(&self, id: &str) -> BridgeResult<()> {
        lock_err(self.mock_domains.lock()).retain(|d| d.id != id);
        lock_err(self.mock_routes.lock()).retain(|r| r.domain_id != id);
        Ok(())
    }

    fn mock_routes(&self) -> BridgeResult<Vec<MockRouteDto>> {
        Ok(lock_err(self.mock_routes.lock()).clone())
    }

    fn mock_add_route(
        &self,
        domain_id: &str,
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        chaos: MockChaosDto,
    ) -> BridgeResult<MockRouteDto> {
        let mut response_headers = std::collections::BTreeMap::new();
        response_headers.insert("Content-Type".to_string(), "application/json".to_string());
        let route = MockRouteDto {
            id: uuid::Uuid::new_v4().to_string(),
            domain_id: domain_id.to_string(),
            method: method.to_uppercase(),
            path: path.to_string(),
            status_code,
            response_body: response_body.to_string(),
            response_headers,
            enabled: true,
            chaos,
        };
        lock_err(self.mock_routes.lock()).push(route.clone());
        Ok(route)
    }

    fn mock_update_route(
        &self,
        id: &str,
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        chaos: MockChaosDto,
    ) -> BridgeResult<()> {
        let mut routes = lock_err(self.mock_routes.lock());
        if let Some(r) = routes.iter_mut().find(|r| r.id == id) {
            r.method = method.to_string();
            r.path = path.to_string();
            r.status_code = status_code;
            r.response_body = response_body.to_string();
            r.chaos = chaos;
        }
        Ok(())
    }

    fn mock_set_route_enabled(&self, id: &str, enabled: bool) -> BridgeResult<()> {
        let mut routes = lock_err(self.mock_routes.lock());
        if let Some(r) = routes.iter_mut().find(|r| r.id == id) {
            r.enabled = enabled;
        }
        Ok(())
    }

    fn mock_delete_route(&self, id: &str) -> BridgeResult<()> {
        lock_err(self.mock_routes.lock()).retain(|r| r.id != id);
        Ok(())
    }

    fn mock_logs(&self) -> BridgeResult<Vec<MockLogDto>> {
        Ok(lock_err(self.mock_logs.lock()).clone())
    }

    fn mock_clear_logs(&self) -> BridgeResult<()> {
        lock_err(self.mock_logs.lock()).clear();
        Ok(())
    }

    fn mock_server_start(&self, port: u16, _cors: bool) -> BridgeResult<MockServerStatusDto> {
        let status = MockServerStatusDto {
            running: true,
            port,
            url: Some(format!("http://127.0.0.1:{port}")),
        };
        *lock_err(self.mock_server.lock()) = status.clone();
        Ok(status)
    }

    fn mock_server_stop(&self) -> BridgeResult<MockServerStatusDto> {
        let mut status = lock_err(self.mock_server.lock());
        status.running = false;
        status.url = None;
        Ok(status.clone())
    }

    fn mock_server_status(&self) -> BridgeResult<MockServerStatusDto> {
        Ok(lock_err(self.mock_server.lock()).clone())
    }
    fn port_scan_start(
        &self,
        _request: &PortScanRequestDto,
        _scan_id: &str,
    ) -> BridgeResult<Vec<PortScanResultDto>> {
        // Stub backend: no sockets. Report two fake open ports so the page
        // has something to render and completes immediately.
        Ok(vec![
            PortScanResultDto {
                host: "127.0.0.1".into(),
                port: 80,
                state: "open".into(),
                service: "http".into(),
                response_time_ms: Some(1),
                banner: Some("stub/1.0".into()),
                error: None,
            },
            PortScanResultDto {
                host: "127.0.0.1".into(),
                port: 443,
                state: "open".into(),
                service: "https".into(),
                response_time_ms: Some(2),
                banner: None,
                error: None,
            },
        ])
    }

    fn port_scan_stop(&self, _scan_id: &str) -> BridgeResult<()> {
        Ok(())
    }

    // ── R2 Storage ─────────────────────────────────────────────────────────
}

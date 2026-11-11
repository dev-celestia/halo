//! MockForge servers/routes/logs and port-scanner accessors.

use super::dto::*;
use super::*;

impl BackendMock for NativeBackend {
    fn mock_domains(&self) -> BridgeResult<Vec<MockDomainDto>> {
        Ok(
            halo_backend::commands::mock_forge::mock_forge_get_domains(&self.runtime.app)
                .map_err(BridgeError::message)?
                .into_iter()
                .map(mock_domain_to_dto)
                .collect(),
        )
    }

    fn mock_add_domain(&self, hostname: &str, ssl: bool) -> BridgeResult<MockDomainDto> {
        let domain = halo_backend::commands::mock_forge::mock_forge_add_domain(
            &self.runtime.app,
            hostname,
            ssl,
        )
        .map_err(BridgeError::message)?;
        Ok(mock_domain_to_dto(domain))
    }

    fn mock_toggle_domain(&self, id: &str) -> BridgeResult<()> {
        halo_backend::commands::mock_forge::mock_forge_toggle_domain(&self.runtime.app, id)
            .map_err(BridgeError::message)
    }

    fn mock_delete_domain(&self, id: &str) -> BridgeResult<()> {
        halo_backend::commands::mock_forge::mock_forge_delete_domain(&self.runtime.app, id)
            .map_err(BridgeError::message)
    }

    fn mock_routes(&self) -> BridgeResult<Vec<MockRouteDto>> {
        Ok(
            halo_backend::commands::mock_forge::mock_forge_get_routes(&self.runtime.app)
                .map_err(BridgeError::message)?
                .into_iter()
                .map(mock_route_to_dto)
                .collect(),
        )
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
        let route =
            self.mock_add_route_impl(domain_id, method, path, status_code, response_body, chaos)?;
        Ok(mock_route_to_dto(route))
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
        use halo_backend::commands::mock_forge::MockRoutePatch;
        halo_backend::commands::mock_forge::mock_forge_update_route(
            &self.runtime.app,
            id,
            MockRoutePatch {
                method: Some(method.to_uppercase()),
                path: Some(path.to_string()),
                status_code: Some(status_code),
                response_body: Some(response_body.to_string()),
                chaos: Some(chaos_to_tools(&chaos)),
                ..Default::default()
            },
        )
        .map_err(BridgeError::message)
    }

    fn mock_set_route_enabled(&self, id: &str, enabled: bool) -> BridgeResult<()> {
        use halo_backend::commands::mock_forge::MockRoutePatch;
        halo_backend::commands::mock_forge::mock_forge_update_route(
            &self.runtime.app,
            id,
            MockRoutePatch {
                enabled: Some(enabled),
                ..Default::default()
            },
        )
        .map_err(BridgeError::message)
    }

    fn mock_delete_route(&self, id: &str) -> BridgeResult<()> {
        halo_backend::commands::mock_forge::mock_forge_delete_route(&self.runtime.app, id)
            .map_err(BridgeError::message)
    }

    fn mock_logs(&self) -> BridgeResult<Vec<MockLogDto>> {
        Ok(
            halo_backend::commands::mock_forge::mock_forge_get_logs(&self.runtime.app)
                .into_iter()
                .map(mock_log_to_dto)
                .collect(),
        )
    }

    fn mock_clear_logs(&self) -> BridgeResult<()> {
        halo_backend::commands::mock_forge::mock_forge_clear_logs(&self.runtime.app);
        Ok(())
    }

    fn mock_server_start(&self, port: u16, cors: bool) -> BridgeResult<MockServerStatusDto> {
        let status =
            halo_backend::commands::mock_forge::mock_server_start(&self.runtime.app, port, cors)
                .map_err(BridgeError::message)?;
        Ok(mock_server_status_to_dto(status))
    }

    fn mock_server_stop(&self) -> BridgeResult<MockServerStatusDto> {
        Ok(mock_server_status_to_dto(
            halo_backend::commands::mock_forge::mock_server_stop(&self.runtime.app),
        ))
    }

    fn mock_server_status(&self) -> BridgeResult<MockServerStatusDto> {
        Ok(mock_server_status_to_dto(
            halo_backend::commands::mock_forge::mock_server_get_status(&self.runtime.app),
        ))
    }
    fn port_scan_start(
        &self,
        request: &PortScanRequestDto,
        scan_id: &str,
    ) -> BridgeResult<Vec<PortScanResultDto>> {
        let tools_request = port_scanner::PortScanRequest {
            scan_id: scan_id.to_string(),
            target: request.target.clone(),
            ports: request.ports.clone(),
            timeout_ms: request.timeout_ms,
            concurrency: request.concurrency,
            banner_grab: request.banner_grab,
            scan_type: "connect".into(),
            stealth_mode: request.stealth_mode,
            delay_ms: request.delay_ms,
            jitter_ms: request.jitter_ms,
            randomize_ports: request.randomize_ports,
        };
        let cancel = port_scanner::register_cancel_for(&self.runtime.app, scan_id);
        let final_rows = self.tokio.block_on(async move {
            port_scanner::run_scan(self.runtime.app.clone(), tools_request, cancel).await
        });
        Ok(final_rows
            .into_iter()
            .map(|r| PortScanResultDto {
                host: r.host,
                port: r.port,
                state: r.state,
                service: r.service,
                response_time_ms: r.response_time_ms,
                banner: r.banner,
                error: r.error,
            })
            .collect())
    }

    fn port_scan_stop(&self, scan_id: &str) -> BridgeResult<()> {
        if let Some(state) = self
            .runtime
            .app
            .try_state::<port_scanner::PortScanState>()
            .map(|s| s.arc())
        {
            port_scanner::stop_port_scan(&state, scan_id);
        }
        Ok(())
    }
}

impl NativeBackend {
    fn mock_add_route_impl(
        &self,
        domain_id: &str,
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        chaos: MockChaosDto,
    ) -> BridgeResult<halo_backend::commands::mock_forge::MockRoute> {
        use halo_backend::commands::mock_forge::MockRoute;
        let mut response_headers = std::collections::HashMap::new();
        response_headers.insert("Content-Type".to_string(), "application/json".to_string());
        let route = MockRoute {
            id: String::new(),
            domain_id: domain_id.to_string(),
            method: method.to_uppercase(),
            path: path.to_string(),
            status_code,
            response_body: response_body.to_string(),
            response_headers,
            matchers: Vec::new(),
            chaos: chaos_to_tools(&chaos),
            enabled: true,
            matcher_enabled: true,
            request_query_params: None,
            request_body: None,
        };
        halo_backend::commands::mock_forge::mock_forge_add_route(&self.runtime.app, route)
            .map_err(BridgeError::message)
    }
}

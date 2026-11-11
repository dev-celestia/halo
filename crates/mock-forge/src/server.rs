use halo_core::tauri::{AppHandle, Emitter, Manager};
use halo_core::types::MockServerStatus;
use parking_lot::Mutex;

pub const LOCAL_MOCK_DOMAIN_ID: &str = "local_mock_server";

pub struct MockServerState {
    pub running: Mutex<bool>,
    pub port: Mutex<u16>,
    pub cors: Mutex<bool>,
}

impl Default for MockServerState {
    fn default() -> Self {
        Self {
            running: Mutex::new(false),
            port: Mutex::new(4000),
            cors: Mutex::new(true),
        }
    }
}

impl MockServerState {
    pub fn status(&self) -> MockServerStatus {
        let running = *self.running.lock();
        let port = *self.port.lock();
        MockServerStatus {
            running,
            port,
            domain_id: Some(LOCAL_MOCK_DOMAIN_ID.to_string()),
            cors_enabled: *self.cors.lock(),
            url: running.then(|| format!("http://127.0.0.1:{port}")),
        }
    }
}

pub fn mock_server_get_status(app: &AppHandle) -> MockServerStatus {
    app.try_state::<MockServerState>()
        .map(|s| s.status())
        .unwrap_or(MockServerStatus {
            running: false,
            port: 4000,
            domain_id: Some(LOCAL_MOCK_DOMAIN_ID.to_string()),
            cors_enabled: true,
            url: None,
        })
}

pub fn mock_server_start(
    app: &AppHandle,
    port: u16,
    cors: bool,
) -> Result<MockServerStatus, String> {
    let server = app
        .try_state::<MockServerState>()
        .ok_or_else(|| "Mock server state is not managed".to_string())?;

    *server.running.lock() = true;
    *server.port.lock() = port;
    *server.cors.lock() = cors;

    let status = server.status();
    let _ = app.emit(
        "mock-server-status-changed",
        &serde_json::to_value(&status).unwrap_or_default(),
    );
    Ok(status)
}

pub fn mock_server_stop(app: &AppHandle) -> MockServerStatus {
    let Some(server) = app.try_state::<MockServerState>() else {
        return mock_server_get_status(app);
    };
    *server.running.lock() = false;
    let status = server.status();
    let _ = app.emit(
        "mock-server-status-changed",
        &serde_json::to_value(&status).unwrap_or_default(),
    );
    status
}

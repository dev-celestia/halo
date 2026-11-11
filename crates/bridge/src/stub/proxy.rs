//! Stub proxy lifecycle (start / stop / status).

use super::*;

impl BackendProxy for StubBackend {
    fn proxy_status(&self) -> ProxyStatus {
        lock_err(self.status.lock()).clone()
    }

    fn start_proxy(&self, port: u16, tls_port: u16) -> BridgeResult<String> {
        let mut status = lock_err(self.status.lock());
        if status.running {
            return Ok(format!(
                "Proxy already running on port {}",
                status.port.unwrap_or(port)
            ));
        }
        status.running = true;
        status.port = Some(port);
        status.tls_port = Some(tls_port);
        status.default_port = port;
        status.connections = 0;
        Ok(format!(
            "Stub proxy started on port {port} (HTTP) and {tls_port} (HTTPS MITM)"
        ))
    }

    fn stop_proxy(&self) -> BridgeResult<String> {
        let mut status = lock_err(self.status.lock());
        if !status.running {
            return Err(BridgeError::message("Proxy is not running"));
        }
        status.running = false;
        status.port = None;
        status.tls_port = None;
        status.connections = 0;
        Ok("Stub proxy stopped".into())
    }
}

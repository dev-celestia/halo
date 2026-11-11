use halo_core::tauri::AppHandle;
use halo_core::types::{PausedRequest, ProxyRecord, WebSocketMessageRecord};

pub fn ingest_proxy_record(_app: &AppHandle, _record: &ProxyRecord) {}
pub fn ingest_intercept_paused_request(_app: &AppHandle, _paused: &PausedRequest) {}
pub fn ingest_websocket_message(
    _app: &AppHandle,
    _message: &WebSocketMessageRecord,
    _host: &str,
    _path: &str,
    _url: &str,
) {
}

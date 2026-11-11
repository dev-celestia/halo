//! Scaffold stub — automation ingest hooks from the reference proxy lifecycle.

use crate::tauri::AppHandle;
use crate::types::{
    AIInsight, CrawlPage, CrawlSession, PausedRequest, ProxyRecord, WebSocketMessageRecord,
};

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

pub fn ingest_crawled_page(_app: &AppHandle, _page: &CrawlPage) {}

pub fn ingest_scan_completed(
    _app: &AppHandle,
    _session: &CrawlSession,
    _pages: &[CrawlPage],
    _insights: &[AIInsight],
) {
}

use halo_core::tauri::AppHandle;
use halo_core::types::{AIInsight, CrawlPage, CrawlSession};

pub fn ingest_crawled_page(_app: &AppHandle, _page: &CrawlPage) {}

pub fn ingest_scan_completed(
    _app: &AppHandle,
    _session: &CrawlSession,
    _pages: &[CrawlPage],
    _insights: &[AIInsight],
) {
}

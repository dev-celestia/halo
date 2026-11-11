//! Crawler session, page, insight, log, and status DTOs.

use serde::{Deserialize, Serialize};

/// Crawl setup draft held in the Browser page UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlerCrawlConfig {
    pub target_url: String,
    pub strategy: String,
    pub max_depth: u32,
    pub max_pages: u32,
    pub same_domain_only: bool,
    pub exclude_paths: String,
    pub request_delay_ms: u64,
    pub timeout_ms: u64,
    pub enable_ai_insights: bool,
    pub network_settle_ms: u64,
    pub capture_screenshots: bool,
    pub capture_rendered_html: bool,
    pub headless: bool,
}

impl Default for CrawlerCrawlConfig {
    fn default() -> Self {
        Self {
            target_url: "https://example.com".into(),
            strategy: "bfs".into(),
            max_depth: 5,
            max_pages: 500,
            same_domain_only: true,
            exclude_paths: String::new(),
            request_delay_ms: 0,
            timeout_ms: 30_000,
            enable_ai_insights: true,
            network_settle_ms: 500,
            capture_screenshots: false,
            capture_rendered_html: false,
            headless: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CrawlerSessionDto {
    pub id: String,
    pub target_url: String,
    pub status: String,
    pub strategy: String,
    pub max_depth: u32,
    pub max_pages: u32,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CrawlerPageDto {
    pub id: String,
    pub session_id: String,
    pub url: String,
    pub title: Option<String>,
    pub status: String,
    pub depth: u32,
    pub parent_url: Option<String>,
    pub http_status: Option<u16>,
    pub links_found: u32,
    pub forms_found: u32,
    pub discovered_at: String,
    pub visited_at: Option<String>,
    pub interesting: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CrawlerInsightDto {
    pub id: String,
    pub session_id: String,
    pub page_id: Option<String>,
    pub severity: String,
    pub finding_type: String,
    pub title: String,
    pub description: String,
    pub url: Option<String>,
    pub ai_used: bool,
    pub analysis_source: Option<String>,
    pub reviewed: bool,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CrawlerLogDto {
    pub id: String,
    pub session_id: String,
    pub level: String,
    pub log_type: String,
    pub message: String,
    pub url: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CrawlerViewTab {
    #[default]
    Activity,
    Findings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlerStatus {
    pub config: CrawlerCrawlConfig,
    pub session: Option<CrawlerSessionDto>,
    pub pages: Vec<CrawlerPageDto>,
    pub insights: Vec<CrawlerInsightDto>,
    pub logs: Vec<CrawlerLogDto>,
    pub view_tab: CrawlerViewTab,
    pub config_open: bool,
    pub severity_filter: String,
    pub selected_insight_id: Option<String>,
    pub log_filter: String,
}

impl Default for CrawlerStatus {
    fn default() -> Self {
        Self {
            config: CrawlerCrawlConfig::default(),
            session: None,
            pages: Vec::new(),
            insights: Vec::new(),
            logs: Vec::new(),
            view_tab: CrawlerViewTab::Activity,
            config_open: false,
            severity_filter: "all".into(),
            selected_insight_id: None,
            log_filter: String::new(),
        }
    }
}

impl CrawlerStatus {
    pub fn status(&self) -> &str {
        self.session
            .as_ref()
            .map(|s| s.status.as_str())
            .unwrap_or("idle")
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status(), "running")
    }

    pub fn is_paused(&self) -> bool {
        matches!(self.status(), "paused")
    }

    pub fn can_start(&self) -> bool {
        matches!(
            self.status(),
            "idle" | "completed" | "failed" | "stopped" | ""
        )
    }

    pub fn pages_visited(&self) -> usize {
        self.pages
            .iter()
            .filter(|p| p.status == "visited" || p.visited_at.is_some())
            .count()
    }

    pub fn interesting_pages(&self) -> Vec<&CrawlerPageDto> {
        self.pages.iter().filter(|p| p.interesting).collect()
    }
}

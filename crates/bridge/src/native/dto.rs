//! Free DTO-conversion helpers shared by the NativeBackend accessors.

use super::*;

pub(super) fn collection_to_dto(r: halo_backend::CollectionRecord) -> CollectionDto {
    CollectionDto {
        id: r.id,
        name: r.name,
        sort_order: r.sort_order,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

pub(super) fn dto_to_collection(d: CollectionDto) -> halo_backend::CollectionRecord {
    halo_backend::CollectionRecord {
        id: d.id,
        name: d.name,
        sort_order: d.sort_order,
        created_at: d.created_at,
        updated_at: d.updated_at,
    }
}

pub(super) fn stash_to_dto(r: halo_backend::StashRecord) -> StashDto {
    StashDto {
        id: r.id,
        name: r.name,
        parent_id: r.parent_id,
        collection_id: r.collection_id,
        sort_order: r.sort_order,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

pub(super) fn dto_to_stash(d: StashDto) -> halo_backend::StashRecord {
    halo_backend::StashRecord {
        id: d.id,
        name: d.name,
        parent_id: d.parent_id,
        collection_id: d.collection_id,
        sort_order: d.sort_order,
        created_at: d.created_at,
        updated_at: d.updated_at,
    }
}

pub(super) fn endpoint_to_dto(r: halo_backend::StashEndpointRecord) -> StashEndpointDto {
    StashEndpointDto {
        id: r.id,
        stash_id: r.stash_id,
        name: r.name,
        method: r.method,
        url: r.url,
        headers: r.headers,
        body: r.body,
        body_type: r.body_type,
        pre_script: r.pre_script,
        test_script: r.test_script,
        sort_order: r.sort_order,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

pub(super) fn dto_to_endpoint(d: StashEndpointDto) -> halo_backend::StashEndpointRecord {
    halo_backend::StashEndpointRecord {
        id: d.id,
        stash_id: d.stash_id,
        name: d.name,
        method: d.method,
        url: d.url,
        headers: d.headers,
        body: d.body,
        body_type: d.body_type,
        pre_script: d.pre_script,
        test_script: d.test_script,
        sort_order: d.sort_order,
        created_at: d.created_at,
        updated_at: d.updated_at,
    }
}

pub(super) fn context_to_dto(r: halo_backend::ContextRecord) -> ContextDto {
    ContextDto {
        id: r.id,
        name: r.name,
        variables: r.variables,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

pub(super) fn dto_to_context(d: ContextDto) -> halo_backend::ContextRecord {
    halo_backend::ContextRecord {
        id: d.id,
        name: d.name,
        variables: d.variables,
        created_at: d.created_at,
        updated_at: d.updated_at,
    }
}

pub(super) fn chronicle_to_dto(r: halo_backend::ChronicleLogRecord) -> ChronicleLogDto {
    ChronicleLogDto {
        id: r.id,
        timestamp: r.timestamp,
        method: r.method,
        url: r.url,
        request_headers: r.request_headers,
        request_body: r.request_body,
        response_status: r.response_status,
        response_status_text: r.response_status_text,
        response_headers: r.response_headers,
        response_body: r.response_body,
        duration_ms: r.duration_ms,
    }
}

pub(super) fn dto_to_chronicle(d: ChronicleLogDto) -> halo_backend::ChronicleLogRecord {
    halo_backend::ChronicleLogRecord {
        id: d.id,
        timestamp: d.timestamp,
        method: d.method,
        url: d.url,
        request_headers: d.request_headers,
        request_body: d.request_body,
        response_status: d.response_status,
        response_status_text: d.response_status_text,
        response_headers: d.response_headers,
        response_body: d.response_body,
        duration_ms: d.duration_ms,
    }
}

pub(super) fn host_path_from_uri(uri: &str) -> (String, String) {
    // Minimal parse without pulling `url` into the bridge crate.
    let without_scheme = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))
        .unwrap_or(uri);
    if let Some((host, rest)) = without_scheme.split_once('/') {
        (host.to_string(), format!("/{rest}"))
    } else if without_scheme.contains(':') || without_scheme.contains('.') {
        (without_scheme.to_string(), "/".into())
    } else {
        (String::new(), uri.to_string())
    }
}

pub(super) fn to_backend_crawl_config(config: CrawlerCrawlConfig) -> CrawlConfig {
    CrawlConfig {
        target_url: config.target_url,
        strategy: Some(config.strategy),
        max_depth: config.max_depth,
        max_pages: config.max_pages,
        same_domain_only: config.same_domain_only,
        exclude_paths: if config.exclude_paths.trim().is_empty() {
            None
        } else {
            Some(config.exclude_paths)
        },
        request_delay_ms: config.request_delay_ms,
        timeout_ms: config.timeout_ms,
        enable_ai_insights: config.enable_ai_insights,
        network_settle_ms: Some(config.network_settle_ms),
        capture_screenshots: config.capture_screenshots,
        capture_rendered_html: config.capture_rendered_html,
        resume_from_url: None,
        human_input_fields: None,
        headless: config.headless,
    }
}

pub(super) fn session_to_dto(session: halo_backend::crawler::CrawlSession) -> CrawlerSessionDto {
    CrawlerSessionDto {
        id: session.id,
        target_url: session.target_url,
        status: session.status,
        strategy: session.strategy,
        max_depth: session.max_depth,
        max_pages: session.max_pages,
        started_at: session.started_at,
        finished_at: session.finished_at,
    }
}

pub(super) fn page_to_dto(page: halo_backend::crawler::CrawlPage) -> CrawlerPageDto {
    CrawlerPageDto {
        id: page.id,
        session_id: page.session_id,
        url: page.url,
        title: page.title,
        status: page.status,
        depth: page.depth,
        parent_url: page.parent_url,
        http_status: page.http_status,
        links_found: page.links_found,
        forms_found: page.forms_found,
        discovered_at: page.discovered_at,
        visited_at: page.visited_at,
        interesting: page.interesting.unwrap_or(false),
    }
}

pub(super) fn insight_to_dto(insight: halo_backend::crawler::AIInsight) -> CrawlerInsightDto {
    CrawlerInsightDto {
        id: insight.id,
        session_id: insight.session_id,
        page_id: insight.page_id,
        severity: insight.severity,
        finding_type: insight.r#type,
        title: insight.title,
        description: insight.description,
        url: insight.url,
        ai_used: insight.ai_used_for_analysis.unwrap_or(false),
        analysis_source: insight.analysis_source,
        reviewed: insight.reviewed,
        created_at: insight.created_at,
    }
}

pub(super) fn log_to_dto(log: halo_backend::crawler::ActivityLog) -> CrawlerLogDto {
    CrawlerLogDto {
        id: log.id,
        session_id: log.session_id,
        level: log.level,
        log_type: log.r#type,
        message: log.message,
        url: log.url,
        created_at: log.created_at,
    }
}

// ── API Mock / API Override (MockForge) ────────────────────────────────────

pub(super) fn mock_domain_to_dto(
    d: halo_backend::commands::mock_forge::MockDomain,
) -> MockDomainDto {
    MockDomainDto {
        id: d.id,
        hostname: d.hostname,
        ssl: d.ssl,
        status: d.status,
        created_at: d.created_at,
    }
}

pub(super) fn chaos_to_tools(c: &MockChaosDto) -> mock_forge::ChaosConfig {
    mock_forge::ChaosConfig {
        latency_mode: if c.latency_mode.is_empty() {
            "none".to_string()
        } else {
            c.latency_mode.clone()
        },
        latency_fixed: c.latency_fixed,
        latency_min: c.latency_min,
        latency_max: c.latency_max,
        error_rate: c.error_rate_bp.map(|bp| bp as f64 / 10_000.0),
        error_status: c.error_status,
    }
}

pub(super) fn chaos_from_tools(c: mock_forge::ChaosConfig) -> MockChaosDto {
    MockChaosDto {
        latency_mode: if c.latency_mode.is_empty() {
            "none".into()
        } else {
            c.latency_mode
        },
        latency_fixed: c.latency_fixed,
        latency_min: c.latency_min,
        latency_max: c.latency_max,
        error_rate_bp: c.error_rate.map(|rate| (rate * 10_000.0).round() as u32),
        error_status: c.error_status,
    }
}

pub(super) fn mock_route_to_dto(r: mock_forge::MockRoute) -> MockRouteDto {
    MockRouteDto {
        id: r.id,
        domain_id: r.domain_id,
        method: r.method,
        path: r.path,
        status_code: r.status_code,
        response_body: r.response_body,
        response_headers: r.response_headers.into_iter().collect(),
        enabled: r.enabled,
        chaos: chaos_from_tools(r.chaos),
    }
}

pub(super) fn mock_log_to_dto(l: halo_backend::commands::mock_forge::RequestLog) -> MockLogDto {
    MockLogDto {
        id: l.id,
        route_id: l.route_id,
        method: l.method,
        path: l.path,
        status_code: l.status_code,
        latency_ms: l.latency_ms,
        timestamp: l.timestamp,
        source: l.source,
    }
}

pub(super) fn mock_server_status_to_dto(
    s: halo_backend::commands::mock_forge::MockServerStatus,
) -> MockServerStatusDto {
    MockServerStatusDto {
        running: s.running,
        port: s.port,
        url: s.url,
    }
}

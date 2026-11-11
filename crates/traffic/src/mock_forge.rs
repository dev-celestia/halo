//! MockForge interception inside the proxy: response override rules.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use halo_core::tauri::{AppHandle, Manager};
use halo_core::types::{MockDomain, MockRoute, RequestLog, RequestMatcher};
use hexbuffer_proxy::{Body, RequestOrResponse};
use hyper::Response;
use parking_lot::Mutex;

use crate::lifecycle::Ctx;

pub struct MockForgeState {
    pub domains: Mutex<Vec<MockDomain>>,
    pub routes: Mutex<Vec<MockRoute>>,
    pub logs: Mutex<Vec<RequestLog>>,
}

impl Default for MockForgeState {
    fn default() -> Self {
        Self {
            domains: Mutex::new(Vec::new()),
            routes: Mutex::new(Vec::new()),
            logs: Mutex::new(Vec::new()),
        }
    }
}

pub fn mock_state(app: &AppHandle) -> Option<halo_core::tauri::State<'_, MockForgeState>> {
    app.try_state::<MockForgeState>()
}

pub fn mock_forge_record_log(app: &AppHandle, log: RequestLog) {
    if let Some(s) = app.try_state::<MockForgeState>() {
        let mut logs = s.logs.lock();
        logs.insert(0, log.clone());
        if logs.len() > 200 {
            logs.truncate(200);
        }
    }
}

pub fn extract_path_params(route_path: &str, req_path: &str) -> HashMap<String, String> {
    let clean_route = route_path.trim_matches('/');
    let clean_req = req_path.trim_matches('/');
    let r_parts: Vec<&str> = clean_route.split('/').filter(|s| !s.is_empty()).collect();
    let p_parts: Vec<&str> = clean_req.split('/').filter(|s| !s.is_empty()).collect();

    let mut params = HashMap::new();
    if r_parts.len() == p_parts.len() {
        for (r, p) in r_parts.iter().zip(p_parts.iter()) {
            if let Some(param_name) = r.strip_prefix(':') {
                params.insert(param_name.to_string(), p.to_string());
            } else if r.starts_with('{') && r.ends_with('}') && r.len() > 2 {
                let param_name = &r[1..r.len() - 1];
                params.insert(param_name.to_string(), p.to_string());
            }
        }
    }
    params
}

pub fn render_template_string(
    template: &str,
    path_params: &HashMap<String, String>,
    query_params: &HashMap<String, String>,
    req_path: &str,
    req_method: &str,
) -> String {
    let mut rendered = template.to_string();

    for (k, v) in path_params {
        rendered = rendered.replace(&format!("{{{{{}}}}}", k), v);
        rendered = rendered.replace(&format!("{{{{params.{}}}}}", k), v);
        rendered = rendered.replace(&format!("{{{{param.{}}}}}", k), v);
        rendered = rendered.replace(&format!("${{{}}}", k), v);
        rendered = rendered.replace(&format!("${{params.{}}}", k), v);
        rendered = rendered.replace(&format!("${{param.{}}}", k), v);
    }

    for (k, v) in query_params {
        rendered = rendered.replace(&format!("{{{{query.{}}}}}", k), v);
        rendered = rendered.replace(&format!("${{query.{}}}", k), v);
        if !path_params.contains_key(k) {
            rendered = rendered.replace(&format!("{{{{{}}}}}", k), v);
            rendered = rendered.replace(&format!("${{{}}}", k), v);
        }
    }

    rendered = rendered.replace("{{path}}", req_path);
    rendered = rendered.replace("${path}", req_path);
    rendered = rendered.replace("{{method}}", req_method);
    rendered = rendered.replace("${method}", req_method);

    rendered
}

fn matchers_satisfied(
    matchers: &[RequestMatcher],
    req_headers: &HashMap<String, String>,
    req_query: &HashMap<String, String>,
    req_body: &[u8],
) -> bool {
    for matcher in matchers {
        if let Some(ref hk) = matcher.header_key {
            let val = req_headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(hk))
                .map(|(_, v)| v.as_str());
            if let Some(ref hv) = matcher.header_value {
                if val != Some(hv.as_str()) {
                    return false;
                }
            } else if val.is_none() {
                return false;
            }
        }
        if let Some(ref qk) = matcher.query_key {
            let val = req_query.get(qk);
            if let Some(ref qv) = matcher.query_value {
                if val.map(|s| s.as_str()) != Some(qv.as_str()) {
                    return false;
                }
            } else if val.is_none() {
                return false;
            }
        }
        if let Some(ref bc) = matcher.body_contains {
            let body_str = String::from_utf8_lossy(req_body);
            if !body_str.contains(bc) {
                return false;
            }
        }
    }
    true
}

fn path_matches(route_path: &str, req_path: &str) -> bool {
    if route_path == req_path || route_path == "*" {
        return true;
    }
    let r_clean = route_path.trim_matches('/');
    let p_clean = req_path.trim_matches('/');
    let r_parts: Vec<&str> = r_clean.split('/').filter(|s| !s.is_empty()).collect();
    let p_parts: Vec<&str> = p_clean.split('/').filter(|s| !s.is_empty()).collect();
    if r_parts.len() != p_parts.len() {
        return false;
    }
    for (r, p) in r_parts.iter().zip(p_parts.iter()) {
        if *r == "*" || r.starts_with(':') || (r.starts_with('{') && r.ends_with('}')) {
            continue;
        }
        if r != p {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
pub fn find_matching_route(
    domains: &[MockDomain],
    routes: &[MockRoute],
    req_host: &str,
    req_method: &str,
    req_path: &str,
    req_headers: &HashMap<String, String>,
    req_query: &HashMap<String, String>,
    req_body: &[u8],
    _is_local_server: bool,
) -> Option<(MockDomain, MockRoute)> {
    let clean_host = req_host
        .split(':')
        .next()
        .unwrap_or(req_host)
        .to_lowercase();
    for route in routes.iter().filter(|r| r.enabled) {
        if !route.method.eq_ignore_ascii_case("ALL")
            && !route.method.eq_ignore_ascii_case(req_method)
        {
            continue;
        }
        let domain_match = domains.iter().find(|d| d.id == route.domain_id);
        if let Some(d) = domain_match {
            if d.status != "active" {
                continue;
            }
            let d_host = d
                .hostname
                .split(':')
                .next()
                .unwrap_or(&d.hostname)
                .to_lowercase();
            if d_host != "*" && d_host != clean_host && !clean_host.ends_with(&format!(".{d_host}"))
            {
                continue;
            }
        }
        if !path_matches(&route.path, req_path) {
            continue;
        }
        if route.matcher_enabled
            && !matchers_satisfied(&route.matchers, req_headers, req_query, req_body)
        {
            continue;
        }
        let domain = domain_match.cloned().unwrap_or_else(|| MockDomain {
            id: route.domain_id.clone(),
            hostname: req_host.to_string(),
            ssl: false,
            status: "active".to_string(),
            created_at: String::new(),
        });
        return Some((domain, route.clone()));
    }
    None
}

/// Host + path + query parsed out of the request target.
fn split_target(uri: &str) -> (String, String, HashMap<String, String>) {
    let trimmed = uri.trim();
    let after_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
    let (authority, rest) = after_scheme.split_at(path_start);
    let host = if authority.is_empty() {
        String::new()
    } else {
        authority.to_string()
    };
    let target = if rest.is_empty() { "/" } else { rest };

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };

    let mut query_map = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        query_map.insert(k.to_string(), v.to_string());
    }
    (host, path, query_map)
}

/// Consult the override route table. Returns `Some(response)` when a rule
/// matched, replacing the upstream exchange entirely.
pub async fn try_intercept(app_handle: &AppHandle, ctx: &Ctx) -> Option<RequestOrResponse> {
    let mock = mock_state(app_handle)?;
    let (domains, routes) = {
        let domains = mock.domains.lock().clone();
        let routes = mock.routes.lock().clone();
        (domains, routes)
    };
    if routes.is_empty() {
        return None;
    }

    let (target_host, path, query) = split_target(&ctx.req_uri);
    let host = if target_host.is_empty() {
        ctx.server_addr.clone()
    } else {
        target_host
    };

    let started = Instant::now();
    let Some((_domain, route)) = find_matching_route(
        &domains,
        &routes,
        &host,
        &ctx.req_method,
        &path,
        &ctx.req_headers,
        &query,
        &ctx.req_body,
        false,
    ) else {
        return None;
    };

    // Chaos: injected latency before serving.
    if route.chaos.latency_mode == "fixed" {
        if let Some(ms) = route.chaos.latency_fixed {
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
    }

    let path_params = extract_path_params(&route.path, &path);
    let rendered = render_template_string(
        &route.response_body,
        &path_params,
        &query,
        &ctx.req_uri,
        &ctx.req_method,
    );

    let mut builder = Response::builder().status(route.status_code);
    for (k, v) in &route.response_headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    builder = builder.header("content-length", rendered.len());
    let response = builder
        .body(Body::from(bytes::Bytes::from(rendered)))
        .unwrap_or_else(|_| Response::new(Body::from(bytes::Bytes::new())));

    record_override_log(
        app_handle,
        ctx,
        &host,
        &path,
        route.status_code,
        &started,
        Some(&route),
    );
    Some(RequestOrResponse::Response(response))
}

fn record_override_log(
    app_handle: &AppHandle,
    ctx: &Ctx,
    host: &str,
    path: &str,
    status_code: u16,
    started: &Instant,
    route: Option<&MockRoute>,
) {
    let log = RequestLog {
        id: uuid::Uuid::new_v4().to_string(),
        domain_id: route.map(|r| r.domain_id.clone()).unwrap_or_default(),
        route_id: route.map(|r| r.id.clone()),
        method: ctx.req_method.clone(),
        path: format!("{host}{path}"),
        status_code,
        latency_ms: started.elapsed().as_millis() as u64,
        timestamp: chrono::Utc::now().to_rfc3339(),
        request_headers: ctx.req_headers.clone(),
        request_body: Some(String::from_utf8_lossy(&ctx.req_body).into_owned()),
        source: Some("response_override".to_string()),
    };
    mock_forge_record_log(app_handle, log);
}

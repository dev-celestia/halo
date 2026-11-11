use std::collections::HashMap;

use halo_core::types::{MockDomain, MockRoute, QueryParam, RequestMatcher};

pub fn normalize_hostname(raw: &str) -> String {
    let s = raw.trim();
    let stripped = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let host_and_port = stripped.split('/').next().unwrap_or(stripped);
    let host_only = host_and_port.split(':').next().unwrap_or(host_and_port);
    host_only.to_lowercase()
}

pub fn method_matches(route_method: &str, req_method: &str) -> bool {
    let rm = route_method.trim();
    rm == "*"
        || rm.eq_ignore_ascii_case("ALL")
        || rm.eq_ignore_ascii_case("ANY")
        || rm.eq_ignore_ascii_case(req_method)
}

pub fn extract_host_and_path_from_route(route_path: &str) -> (Option<String>, String) {
    let s = route_path.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        if let Ok(u) = url::Url::parse(s) {
            let host = u.host_str().map(|h| h.to_lowercase());
            let path = format!(
                "{}{}",
                u.path(),
                if let Some(q) = u.query() {
                    format!("?{}", q)
                } else {
                    String::new()
                }
            );
            return (host, path);
        } else if let Some(pos) = s.find("://") {
            let after_scheme = &s[pos + 3..];
            if let Some(slash_pos) = after_scheme.find('/') {
                let host = after_scheme[..slash_pos]
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                let path = after_scheme[slash_pos..].to_string();
                return (Some(host), path);
            } else {
                let host = after_scheme.split(':').next().unwrap_or("").to_lowercase();
                return (Some(host), "/".to_string());
            }
        }
    }
    (None, s.to_string())
}

pub fn path_matches(route_path: &str, req_path: &str) -> bool {
    let clean_route = route_path.split('?').next().unwrap_or(route_path).trim();
    let clean_req = req_path.split('?').next().unwrap_or(req_path).trim();

    if clean_route == "*" || clean_route == "/*" || clean_route == clean_req {
        return true;
    }

    if let Some(prefix) = clean_route.strip_suffix("/**") {
        return clean_req.starts_with(prefix);
    }
    if let Some(prefix) = clean_route.strip_suffix("/*") {
        return clean_req.starts_with(prefix);
    }

    let r_parts: Vec<&str> = clean_route.split('/').filter(|s| !s.is_empty()).collect();
    let p_parts: Vec<&str> = clean_req.split('/').filter(|s| !s.is_empty()).collect();

    if r_parts.len() != p_parts.len() {
        return false;
    }

    for (r, p) in r_parts.iter().zip(p_parts.iter()) {
        if r.starts_with(':') || (r.starts_with('{') && r.ends_with('}')) || *r == "*" {
            continue;
        }
        if r != p {
            return false;
        }
    }
    true
}

pub fn extract_path_params(route_path: &str, req_path: &str) -> HashMap<String, String> {
    let clean_route = route_path.split('?').next().unwrap_or(route_path).trim();
    let clean_req = req_path.split('?').next().unwrap_or(req_path).trim();

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

pub fn matchers_satisfied(
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

// The request parts (host, method, path, headers, query, body) are each independently meaningful
// to a route matcher; grouping them would only rename them.
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
    is_local_server: bool,
) -> Option<(MockDomain, MockRoute)> {
    let req_host_norm = normalize_hostname(req_host);

    // 1. If this is the Local Mock Server (localhost / 127.0.0.1)
    if is_local_server {
        let local_routes = routes.iter().filter(|r| {
            (r.domain_id == "local_mock_server"
                || r.domain_id.is_empty()
                || r.domain_id == "localhost")
                && r.enabled
                && method_matches(&r.method, req_method)
                && path_matches(&r.path, req_path)
        });

        for r in local_routes {
            if !r.matcher_enabled
                || matchers_satisfied(&r.matchers, req_headers, req_query, req_body)
            {
                if let Some(ref expected_body) = r.request_body {
                    if !expected_body.trim().is_empty() {
                        let actual_body = String::from_utf8_lossy(req_body);
                        if trim_json(&actual_body) != trim_json(expected_body) {
                            continue;
                        }
                    }
                }
                if let Some(ref params) = r.request_query_params {
                    let active_params: Vec<&QueryParam> =
                        params.iter().filter(|p| p.enabled).collect();
                    if !active_params.is_empty() {
                        let all_match = active_params.iter().all(|p| {
                            req_query
                                .get(&p.key)
                                .map(|v| v == &p.value)
                                .unwrap_or(false)
                        });
                        if !all_match {
                            continue;
                        }
                    }
                }

                let local_domain = MockDomain {
                    id: "local_mock_server".to_string(),
                    hostname: "localhost".to_string(),
                    ssl: false,
                    status: "active".to_string(),
                    created_at: String::new(),
                };
                return Some((local_domain, r.clone()));
            }
        }
    }

    // 2. Proxy Override matching: match by hosts + path OR full url, and method
    for r in routes.iter().filter(|r| r.enabled) {
        if !method_matches(&r.method, req_method) {
            continue;
        }

        let (url_host, url_path) = extract_host_and_path_from_route(&r.path);

        let (matched_domain, effective_path) = if let Some(ref parsed_host) = url_host {
            // Full URL matching: match extracted host against request host
            let parsed_host_norm = normalize_hostname(parsed_host);
            if parsed_host_norm != req_host_norm && parsed_host_norm != "*" {
                continue;
            }

            // Check if there is an inactive domain entry explicitly disabling this host
            if let Some(d) = domains
                .iter()
                .find(|d| normalize_hostname(&d.hostname) == req_host_norm)
            {
                if d.status != "active" {
                    continue;
                }
            }

            let dom = domains
                .iter()
                .find(|d| d.id == r.domain_id || normalize_hostname(&d.hostname) == req_host_norm)
                .cloned()
                .unwrap_or_else(|| MockDomain {
                    id: r.domain_id.clone(),
                    hostname: parsed_host.clone(),
                    ssl: r.path.starts_with("https://"),
                    status: "active".to_string(),
                    created_at: String::new(),
                });

            (dom, url_path)
        } else {
            // Host + Path matching
            let dom = domains.iter().find(|d| d.id == r.domain_id);
            let target_host = if let Some(d) = dom {
                if d.status != "active" {
                    continue;
                }
                d.hostname.clone()
            } else {
                r.domain_id.clone()
            };

            let target_host_norm = normalize_hostname(&target_host);
            let host_matches = if target_host_norm == "*" {
                true
            } else if let Some(suffix) = target_host_norm.strip_prefix("*.") {
                req_host_norm.ends_with(suffix) || req_host_norm == suffix
            } else {
                target_host_norm == req_host_norm
            };

            if !host_matches {
                continue;
            }

            let effective_dom = dom.cloned().unwrap_or_else(|| MockDomain {
                id: r.domain_id.clone(),
                hostname: target_host,
                ssl: true,
                status: "active".to_string(),
                created_at: String::new(),
            });

            (effective_dom, r.path.clone())
        };

        if !path_matches(&effective_path, req_path) {
            continue;
        }

        // When matcher is disabled, match on method + host/path only
        if !r.matcher_enabled {
            return Some((matched_domain, r.clone()));
        }

        // Check request matchers (headers, query, body)
        if !matchers_satisfied(&r.matchers, req_headers, req_query, req_body) {
            continue;
        }

        // Check request body matcher (for write methods)
        if let Some(ref expected_body) = r.request_body {
            if !expected_body.trim().is_empty() {
                let actual_body = String::from_utf8_lossy(req_body);
                if trim_json(&actual_body) != trim_json(expected_body) {
                    continue;
                }
            }
        }

        // Check request query params (for read methods)
        if let Some(ref params) = r.request_query_params {
            let active_params: Vec<&QueryParam> = params.iter().filter(|p| p.enabled).collect();
            if !active_params.is_empty() {
                let all_match = active_params.iter().all(|p| {
                    req_query
                        .get(&p.key)
                        .map(|v| v == &p.value)
                        .unwrap_or(false)
                });
                if !all_match {
                    continue;
                }
            }
        }

        return Some((matched_domain, r.clone()));
    }

    None
}

pub fn trim_json(s: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        serde_json::to_string(&v).unwrap_or_else(|_| s.to_string())
    } else {
        s.trim().to_string()
    }
}

use parking_lot::Mutex;
use std::collections::HashMap;
use uuid::Uuid;

pub use super::types::*;

/// Maximum number of requests held for an intercept decision at once.
///
/// Bounds memory (each entry can pin a 32 MiB body) and the number of
/// connections/tasks parked in the pause loop. Beyond this the proxy fails
/// open: requests are forwarded unmodified and a warning is logged.
pub const MAX_PAUSED_REQUESTS: usize = 200;

/// How long a request may sit in the pause queue before it is forwarded
/// unchanged. Without a deadline a paused request pins its connection and task
/// forever, which a page full of XHRs turns into an unbounded leak.
pub const PAUSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Default)]
pub struct ProxyStateInner {
    pub records: Vec<ProxyRecord>,
    pub intercept_mode: InterceptMode,
    pub paused_requests: Vec<PausedRequest>,
    pub paused_actions: HashMap<Uuid, InterceptAction>,
    pub active_intercept_tab_id: Option<String>,
    pub intercept_capture_patterns: Vec<String>,
    pub intercept_bypass_patterns: Vec<String>,
    pub db_filter_config: ProxyDbFilterConfig,
    /// Strip `If-None-Match` / `If-Modified-Since` from forwarded requests.
    ///
    /// Off by default: removing cache validators changes what the server sees
    /// and what the client receives (200 instead of 304), which invalidates
    /// findings and alters the user's traffic. Opt in only when the goal is
    /// "always show me full bodies in the history view".
    pub strip_cache_validators: bool,
}

#[derive(Debug, Clone)]
pub enum InterceptAction {
    Forward {
        request: Option<ProxyRequest>,
        intercept_response: bool,
    },
    ForwardResponse(Option<ProxyResponse>),
    Drop,
}

pub struct ProxyState(Mutex<ProxyStateInner>);

impl ProxyState {
    pub fn new() -> Self {
        Self(Mutex::new(ProxyStateInner::default()))
    }

    /// Whether cache validators are stripped from forwarded requests.
    pub fn strip_cache_validators(&self) -> bool {
        self.0.lock().strip_cache_validators
    }

    pub fn set_strip_cache_validators(&self, strip: bool) {
        self.0.lock().strip_cache_validators = strip;
    }

    pub fn get_mode(&self) -> InterceptMode {
        self.0.lock().intercept_mode.clone()
    }

    pub fn set_mode(&self, mode: InterceptMode) {
        let mut inner = self.0.lock();
        if mode == InterceptMode::Disabled && inner.intercept_mode == InterceptMode::Enabled {
            let paused_requests = inner.paused_requests.clone();
            for paused_request in &paused_requests {
                let action = if paused_request.response.is_some() {
                    InterceptAction::ForwardResponse(None)
                } else {
                    InterceptAction::Forward {
                        request: None,
                        intercept_response: false,
                    }
                };
                inner.paused_actions.insert(paused_request.id, action);
            }
            inner.paused_requests.clear();
        }
        inner.intercept_mode = mode;
    }

    pub fn get_status(&self) -> InterceptStatus {
        let inner = self.0.lock();
        InterceptStatus {
            mode: inner.intercept_mode.clone(),
            paused_count: inner.paused_requests.len(),
        }
    }

    /// Enqueue a request awaiting an intercept decision.
    ///
    /// Returns `false` when the queue is full. The caller must then forward the
    /// request unchanged: each paused entry pins up to 32 MiB of body plus a
    /// connection and a task, so an unbounded queue is an OOM/hang primitive —
    /// and with the proxy reachable from the network, a remote one.
    pub fn add_paused_request(&self, req: PausedRequest) -> bool {
        let mut inner = self.0.lock();
        if inner.paused_requests.len() >= MAX_PAUSED_REQUESTS {
            return false;
        }
        inner.paused_requests.push(req);
        true
    }

    pub fn get_paused_request(&self, id: &Uuid) -> Option<PausedRequest> {
        self.0
            .lock()
            .paused_requests
            .iter()
            .find(|r| r.id == *id)
            .cloned()
    }

    pub fn get_all_paused(&self) -> Vec<PausedRequest> {
        self.0.lock().paused_requests.clone()
    }

    pub fn set_intercept_scope(&self, tab_id: String, capture_patterns: Vec<String>) {
        let mut inner = self.0.lock();
        inner.active_intercept_tab_id = Some(tab_id);
        inner.intercept_capture_patterns = capture_patterns
            .into_iter()
            .map(|pattern| pattern.trim().to_lowercase())
            .filter(|pattern| !pattern.is_empty())
            .collect();
    }

    pub fn get_intercept_scope(&self) -> (Option<String>, Vec<String>) {
        let inner = self.0.lock();
        (
            inner.active_intercept_tab_id.clone(),
            inner.intercept_capture_patterns.clone(),
        )
    }

    pub fn add_intercept_pattern(&self, pattern: &str) -> Vec<String> {
        let mut inner = self.0.lock();
        let pattern = pattern.trim().to_lowercase();
        if !pattern.is_empty() && !inner.intercept_capture_patterns.contains(&pattern) {
            inner.intercept_capture_patterns.push(pattern);
        }
        if inner.active_intercept_tab_id.is_none() {
            inner.active_intercept_tab_id = Some("default".into());
        }
        inner.intercept_capture_patterns.clone()
    }

    pub fn remove_intercept_pattern(&self, pattern: &str) -> Vec<String> {
        let mut inner = self.0.lock();
        let pattern = pattern.trim().to_lowercase();
        inner.intercept_capture_patterns.retain(|p| p != &pattern);
        inner.intercept_capture_patterns.clone()
    }

    pub fn matching_intercept_tab_id(&self, uri: &str) -> Option<String> {
        let host = uri
            .split("://")
            .nth(1)
            .unwrap_or(uri)
            .split('/')
            .next()
            .unwrap_or(uri)
            .to_lowercase();
        let host_without_port = host.split(':').next().unwrap_or(&host);

        let inner = self.0.lock();
        let tab_id = inner.active_intercept_tab_id.clone()?;

        if inner.intercept_capture_patterns.is_empty() {
            return None;
        }

        for pattern in &inner.intercept_capture_patterns {
            if pattern == "*" || pattern == "*.*" {
                return Some(tab_id);
            }
            if let Some(domain) = pattern.strip_prefix("*.") {
                if host_without_port.ends_with(domain) || host.ends_with(domain) {
                    return Some(tab_id);
                }
            } else if host == *pattern || host_without_port == pattern {
                return Some(tab_id);
            }
        }

        None
    }

    pub fn forward_paused_request(
        &self,
        id: &Uuid,
        request: Option<ProxyRequest>,
        intercept_response: bool,
    ) -> bool {
        let mut inner = self.0.lock();
        let existed = inner.paused_requests.iter().any(|r| r.id == *id);

        if existed {
            inner.paused_actions.insert(
                *id,
                InterceptAction::Forward {
                    request,
                    intercept_response,
                },
            );
            inner.paused_requests.retain(|request| request.id != *id);
        }

        existed
    }

    pub fn forward_paused_response(&self, id: &Uuid, response: Option<ProxyResponse>) -> bool {
        let mut inner = self.0.lock();
        let existed = inner.paused_requests.iter().any(|r| r.id == *id);

        if existed {
            inner
                .paused_actions
                .insert(*id, InterceptAction::ForwardResponse(response));
            inner.paused_requests.retain(|request| request.id != *id);
        }

        existed
    }

    pub fn drop_paused_request(&self, id: &Uuid) -> bool {
        let mut inner = self.0.lock();
        let existed = inner.paused_requests.iter().any(|r| r.id == *id);

        if existed {
            inner.paused_actions.insert(*id, InterceptAction::Drop);
            inner.paused_requests.retain(|request| request.id != *id);
        }

        existed
    }

    pub fn take_paused_action(&self, id: &Uuid) -> Option<InterceptAction> {
        self.0.lock().paused_actions.remove(id)
    }

    pub fn forward_paused_by_tab(&self, tab_id: &str) -> usize {
        let mut inner = self.0.lock();
        let matching_requests: Vec<PausedRequest> = inner
            .paused_requests
            .iter()
            .filter(|request| request.tab_id.as_deref() == Some(tab_id))
            .cloned()
            .collect();

        for request in &matching_requests {
            let action = if request.response.is_some() {
                InterceptAction::ForwardResponse(None)
            } else {
                InterceptAction::Forward {
                    request: None,
                    intercept_response: false,
                }
            };
            inner.paused_actions.insert(request.id, action);
        }

        inner
            .paused_requests
            .retain(|request| request.tab_id.as_deref() != Some(tab_id));

        matching_requests.len()
    }

    pub fn clear_records(&self) {
        self.0.lock().records.clear();
    }

    /// Drops all pending intercepted requests and queued actions. Called when
    /// the proxy stops: the tasks holding those connections die with the
    /// runtime, so any leftover entries would otherwise leak until app exit.
    pub fn clear_intercept_state(&self) {
        let mut inner = self.0.lock();
        inner.paused_requests.clear();
        inner.paused_actions.clear();
    }

    pub fn clear_records_before(&self, cutoff: &chrono::DateTime<chrono::Utc>) {
        let mut inner = self.0.lock();
        inner.records.retain(|r| r.timestamp >= *cutoff);
    }

    pub fn delete_record(&self, id: &Uuid) -> Option<ProxyRecord> {
        let mut inner = self.0.lock();
        inner
            .records
            .iter()
            .position(|r| r.id == *id)
            .map(|pos| inner.records.remove(pos))
    }

    pub fn get_bypass_patterns(&self) -> Vec<String> {
        self.0.lock().intercept_bypass_patterns.clone()
    }

    pub fn set_bypass_patterns(&self, patterns: Vec<String>) {
        self.0.lock().intercept_bypass_patterns = patterns;
    }

    pub fn add_bypass_pattern(&self, pattern: String) -> Vec<String> {
        let mut inner = self.0.lock();
        let trimmed = pattern.trim().to_string();
        if !trimmed.is_empty() && !inner.intercept_bypass_patterns.contains(&trimmed) {
            inner.intercept_bypass_patterns.push(trimmed);
        }
        inner.intercept_bypass_patterns.clone()
    }

    pub fn remove_bypass_pattern(&self, pattern: &str) -> Vec<String> {
        let mut inner = self.0.lock();
        inner.intercept_bypass_patterns.retain(|p| p != pattern);
        inner.intercept_bypass_patterns.clone()
    }

    pub fn should_bypass_uri(&self, uri: &str) -> bool {
        if crate::utils::is_captive_portal(uri) {
            return true;
        }

        let host = uri
            .split("://")
            .nth(1)
            .unwrap_or(uri)
            .split('/')
            .next()
            .unwrap_or(uri);

        let inner = self.0.lock();
        for pattern in &inner.intercept_bypass_patterns {
            if let Some(domain) = pattern.strip_prefix("*.") {
                if host.ends_with(domain) {
                    return true;
                }
            } else if host == pattern {
                return true;
            }
        }
        false
    }

    pub fn get_db_filter_config(&self) -> ProxyDbFilterConfig {
        self.0.lock().db_filter_config.clone()
    }

    pub fn set_db_filter_config(&self, config: ProxyDbFilterConfig) {
        self.0.lock().db_filter_config = config;
    }

    // ponytail: evaluate if a proxy request should be inserted into sqlite DB
    pub fn should_record_to_db(&self, record: &ProxyRecord) -> bool {
        let inner = self.0.lock();
        let config = &inner.db_filter_config;

        if !config.enabled {
            return false;
        }

        let candidate_hosts = extract_candidate_hosts(
            &record.request.uri,
            &record.server_addr,
            &record.request.headers,
        );

        // 1. Exclude blacklist check
        if !config.exclude_hosts.is_empty() {
            for pattern in &config.exclude_hosts {
                for host in &candidate_hosts {
                    if matches_host_pattern(host, pattern) {
                        return false;
                    }
                }
            }
        }

        // 2. Mode check
        match config.mode {
            ProxyRecordMode::All => true,
            ProxyRecordMode::TargetScope => {
                if config.target_hosts.is_empty() {
                    return false;
                }
                for pattern in &config.target_hosts {
                    for host in &candidate_hosts {
                        if matches_host_pattern(host, pattern) {
                            return true;
                        }
                    }
                }
                false
            }
            ProxyRecordMode::Custom => {
                if config.custom_hosts.is_empty() {
                    return false;
                }
                for pattern in &config.custom_hosts {
                    for host in &candidate_hosts {
                        if matches_host_pattern(host, pattern) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }

    // ponytail: evaluate if a websocket connection/message should be inserted into sqlite DB
    pub fn should_record_ws_to_db(
        &self,
        host: &str,
        uri: &str,
        headers: &std::collections::HashMap<String, String>,
    ) -> bool {
        let inner = self.0.lock();
        let config = &inner.db_filter_config;

        if !config.enabled {
            return false;
        }

        let mut candidate_hosts = extract_candidate_hosts(uri, host, headers);
        if !host.is_empty() && !candidate_hosts.contains(&host.to_lowercase()) {
            candidate_hosts.push(host.to_lowercase());
        }

        // 1. Exclude check
        if !config.exclude_hosts.is_empty() {
            for pattern in &config.exclude_hosts {
                for candidate in &candidate_hosts {
                    if matches_host_pattern(candidate, pattern) {
                        return false;
                    }
                }
            }
        }

        // 2. Mode check
        match config.mode {
            ProxyRecordMode::All => true,
            ProxyRecordMode::TargetScope => {
                if config.target_hosts.is_empty() {
                    return false;
                }
                for pattern in &config.target_hosts {
                    for candidate in &candidate_hosts {
                        if matches_host_pattern(candidate, pattern) {
                            return true;
                        }
                    }
                }
                false
            }
            ProxyRecordMode::Custom => {
                if config.custom_hosts.is_empty() {
                    return false;
                }
                for pattern in &config.custom_hosts {
                    for candidate in &candidate_hosts {
                        if matches_host_pattern(candidate, pattern) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }
}

fn extract_candidate_hosts(
    uri: &str,
    server_addr: &str,
    headers: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut candidates = Vec::new();

    // 1. Host or :authority header
    if let Some(hdr) = headers
        .get("host")
        .or_else(|| headers.get("Host"))
        .or_else(|| headers.get(":authority"))
    {
        let clean = hdr.split(':').next().unwrap_or("").trim().to_lowercase();
        if !clean.is_empty() && !candidates.contains(&clean) {
            candidates.push(clean);
        }
    }

    // 2. Origin header
    if let Some(origin) = headers.get("origin").or_else(|| headers.get("Origin")) {
        let clean = origin
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if !clean.is_empty() && clean != "null" && clean != "opaque" && !candidates.contains(&clean)
        {
            candidates.push(clean);
        }
    }

    // 3. Request URI host
    if uri.contains("://") {
        if let Some(after_scheme) = uri.split("://").nth(1) {
            let clean = after_scheme
                .split('/')
                .next()
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if !clean.is_empty() && !candidates.contains(&clean) {
                candidates.push(clean);
            }
        }
    } else if !uri.starts_with('/') {
        let clean = uri
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if !clean.is_empty() && !candidates.contains(&clean) {
            candidates.push(clean);
        }
    }

    // 4. Server Addr
    if !server_addr.is_empty() && !server_addr.starts_with('/') {
        let clean = server_addr
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if !clean.is_empty() && !candidates.contains(&clean) {
            candidates.push(clean);
        }
    }

    candidates
}

fn matches_host_pattern(candidate: &str, pattern: &str) -> bool {
    let cand_clean = candidate.trim().to_lowercase();
    let pat_clean = pattern
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_lowercase();

    if cand_clean.is_empty() || pat_clean.is_empty() {
        return false;
    }

    let pat_no_port = pat_clean.split(':').next().unwrap_or(&pat_clean);
    let cand_no_port = cand_clean.split(':').next().unwrap_or(&cand_clean);

    if let Some(base_domain) = pat_clean.strip_prefix("*.") {
        let base_no_port = base_domain.split(':').next().unwrap_or(base_domain);
        return cand_clean == base_domain
            || cand_clean.ends_with(&format!(".{}", base_domain))
            || cand_no_port == base_no_port
            || cand_no_port.ends_with(&format!(".{}", base_no_port));
    }

    cand_clean == pat_clean
        || cand_no_port == pat_no_port
        || cand_clean.ends_with(&format!(".{}", pat_clean))
        || cand_no_port.ends_with(&format!(".{}", pat_no_port))
        || cand_clean.contains(&pat_clean)
}

impl Default for ProxyState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_record(uri: &str, host_header: Option<&str>, server_addr: &str) -> ProxyRecord {
        let mut headers = HashMap::new();
        if let Some(h) = host_header {
            headers.insert("Host".to_string(), h.to_string());
        }

        ProxyRecord {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            client_addr: "127.0.0.1:55555".to_string(),
            server_addr: server_addr.to_string(),
            request: ProxyRequest {
                method: "GET".to_string(),
                uri: uri.to_string(),
                http_version: "HTTP/1.1".to_string(),
                headers,
                body: Vec::new(),
                content_decoded: false,
            },
            response: None,
        }
    }

    #[test]
    fn test_db_filter_all_mode() {
        let state = ProxyState::new();
        state.set_db_filter_config(ProxyDbFilterConfig {
            enabled: true,
            mode: ProxyRecordMode::All,
            custom_hosts: vec![],
            target_hosts: vec![],
            exclude_hosts: vec![],
        });

        let rec = create_test_record(
            "https://google.com/search",
            Some("google.com"),
            "google.com:443",
        );
        assert!(state.should_record_to_db(&rec));
    }

    #[test]
    fn test_db_filter_target_scope_mode() {
        let state = ProxyState::new();
        state.set_db_filter_config(ProxyDbFilterConfig {
            enabled: true,
            mode: ProxyRecordMode::TargetScope,
            custom_hosts: vec![],
            target_hosts: vec!["*.target.com".to_string(), "api.example.com".to_string()],
            exclude_hosts: vec!["analytics.target.com".to_string()],
        });

        // In scope matching wildcard
        let rec1 = create_test_record(
            "https://sub.target.com/users",
            Some("sub.target.com"),
            "sub.target.com:443",
        );
        assert!(state.should_record_to_db(&rec1));

        // In scope matching exact host
        let rec2 = create_test_record(
            "https://api.example.com/v1",
            Some("api.example.com"),
            "api.example.com:443",
        );
        assert!(state.should_record_to_db(&rec2));

        // Out of scope
        let rec3 = create_test_record(
            "https://google.com/gen_204",
            Some("google.com"),
            "google.com:443",
        );
        assert!(!state.should_record_to_db(&rec3));

        // Excluded host even if matching wildcard
        let rec4 = create_test_record(
            "https://analytics.target.com/track",
            Some("analytics.target.com"),
            "analytics.target.com:443",
        );
        assert!(!state.should_record_to_db(&rec4));
    }

    #[test]
    fn test_db_filter_custom_mode() {
        let state = ProxyState::new();
        state.set_db_filter_config(ProxyDbFilterConfig {
            enabled: true,
            mode: ProxyRecordMode::Custom,
            custom_hosts: vec!["api.internal.corp".to_string()],
            target_hosts: vec![],
            exclude_hosts: vec![],
        });

        let rec1 = create_test_record(
            "https://api.internal.corp:8443/data",
            Some("api.internal.corp:8443"),
            "api.internal.corp:8443",
        );
        assert!(state.should_record_to_db(&rec1));

        let rec2 = create_test_record(
            "https://other.corp/data",
            Some("other.corp"),
            "other.corp:443",
        );
        assert!(!state.should_record_to_db(&rec2));
    }

    #[test]
    fn test_db_filter_disabled() {
        let state = ProxyState::new();
        state.set_db_filter_config(ProxyDbFilterConfig {
            enabled: false,
            mode: ProxyRecordMode::All,
            custom_hosts: vec![],
            target_hosts: vec![],
            exclude_hosts: vec![],
        });

        let rec = create_test_record(
            "https://api.example.com",
            Some("api.example.com"),
            "api.example.com:443",
        );
        assert!(!state.should_record_to_db(&rec));
    }
}

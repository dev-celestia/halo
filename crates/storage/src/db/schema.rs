pub const CREATE_HTTP_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS http_sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    description TEXT,
    capture_mode TEXT NOT NULL DEFAULT 'all',
    capture_filter TEXT NOT NULL DEFAULT '[]',
    exclude_filter TEXT NOT NULL DEFAULT '[]',
    storage_mode TEXT NOT NULL DEFAULT 'persistent'
);

CREATE INDEX IF NOT EXISTS idx_http_sessions_active ON http_sessions(is_active);
CREATE INDEX IF NOT EXISTS idx_http_sessions_updated_at ON http_sessions(updated_at);
"#;

pub const CREATE_HTTP_LOGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS http_logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL DEFAULT '',
    timestamp TEXT NOT NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    request_headers TEXT,
    request_body BLOB,
    req_payload_ref TEXT DEFAULT '',
    req_body_size INTEGER DEFAULT 0,
    req_truncated INTEGER DEFAULT 0,
    response_status INTEGER,
    response_status_text TEXT,
    response_headers TEXT,
    response_body BLOB,
    res_payload_ref TEXT DEFAULT '',
    res_body_size INTEGER DEFAULT 0,
    res_truncated INTEGER DEFAULT 0,
    client_addr TEXT,
    server_addr TEXT,
    duration_ms INTEGER,
    FOREIGN KEY(session_id) REFERENCES http_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_http_logs_timestamp ON http_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_http_logs_method ON http_logs(method);
CREATE INDEX IF NOT EXISTS idx_http_logs_url ON http_logs(url);
CREATE INDEX IF NOT EXISTS idx_http_logs_response_status ON http_logs(response_status);
CREATE INDEX IF NOT EXISTS idx_http_logs_server_addr ON http_logs(server_addr);
CREATE INDEX IF NOT EXISTS idx_http_logs_host_status_ts ON http_logs(server_addr, response_status, timestamp DESC);
"#;

pub const CREATE_WEBSOCKET_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS websocket_connections (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL DEFAULT '',
    timestamp TEXT NOT NULL,
    url TEXT NOT NULL,
    host TEXT NOT NULL,
    path TEXT NOT NULL,
    handshake_request_headers TEXT,
    handshake_response_status INTEGER,
    handshake_response_headers TEXT,
    client_addr TEXT,
    server_addr TEXT,
    state TEXT NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    last_activity_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES http_sessions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS websocket_messages (
    id TEXT PRIMARY KEY,
    connection_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    direction TEXT NOT NULL,
    message_type TEXT NOT NULL,
    payload BLOB,
    payload_size INTEGER NOT NULL,
    FOREIGN KEY(connection_id) REFERENCES websocket_connections(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_websocket_connections_timestamp ON websocket_connections(timestamp);
CREATE INDEX IF NOT EXISTS idx_websocket_connections_host ON websocket_connections(host);
CREATE INDEX IF NOT EXISTS idx_websocket_connections_url ON websocket_connections(url);
CREATE INDEX IF NOT EXISTS idx_websocket_messages_connection_id ON websocket_messages(connection_id);
CREATE INDEX IF NOT EXISTS idx_websocket_messages_timestamp ON websocket_messages(timestamp);
"#;

pub const CREATE_DOCUMENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    title TEXT NOT NULL,
    sections TEXT NOT NULL,
    custom_sections TEXT NOT NULL DEFAULT '[]',
    removed_built_in_sections TEXT NOT NULL DEFAULT '[]',
    api_entries TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_documents_updated_at ON documents(updated_at);
"#;

pub const CREATE_CRAWLER_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS crawler_sessions (
    id TEXT PRIMARY KEY,
    target_url TEXT NOT NULL,
    strategy TEXT NOT NULL,
    status TEXT NOT NULL,
    max_depth INTEGER NOT NULL,
    max_pages INTEGER NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS crawler_pages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    url TEXT NOT NULL,
    title TEXT,
    status TEXT NOT NULL,
    depth INTEGER NOT NULL,
    parent_url TEXT,
    http_status INTEGER,
    links_found INTEGER NOT NULL DEFAULT 0,
    forms_found INTEGER NOT NULL DEFAULT 0,
    ai_summary TEXT,
    ai_used_for_analysis INTEGER,
    interesting INTEGER NOT NULL DEFAULT 0,
    screenshot_path TEXT,
    rendered_html_path TEXT,
    discovered_at TEXT NOT NULL,
    visited_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES crawler_sessions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS crawler_edges (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    from_url TEXT NOT NULL,
    to_url TEXT NOT NULL,
    source TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES crawler_sessions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS crawler_insights (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    page_id TEXT,
    url TEXT,
    severity TEXT NOT NULL,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    ai_used_for_analysis INTEGER,
    analysis_source TEXT,
    analysis_tool_id TEXT,
    analysis_tool_name TEXT,
    reviewed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES crawler_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(page_id) REFERENCES crawler_pages(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS crawler_logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    level TEXT NOT NULL,
    type TEXT NOT NULL,
    message TEXT NOT NULL,
    url TEXT,
    ai_used_for_analysis INTEGER,
    extra_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(session_id) REFERENCES crawler_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_crawler_sessions_status ON crawler_sessions(status);
CREATE INDEX IF NOT EXISTS idx_crawler_sessions_started_at ON crawler_sessions(started_at);
CREATE INDEX IF NOT EXISTS idx_crawler_pages_session_id ON crawler_pages(session_id);
CREATE INDEX IF NOT EXISTS idx_crawler_pages_url ON crawler_pages(url);
CREATE INDEX IF NOT EXISTS idx_crawler_edges_session_id ON crawler_edges(session_id);
CREATE INDEX IF NOT EXISTS idx_crawler_insights_session_id ON crawler_insights(session_id);
CREATE INDEX IF NOT EXISTS idx_crawler_logs_session_id ON crawler_logs(session_id);
CREATE INDEX IF NOT EXISTS idx_crawler_logs_created_at ON crawler_logs(created_at);
"#;

pub const CREATE_COLLABORATOR_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS collaborator_servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    api_key TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'unknown',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collaborator_payloads (
    id TEXT PRIMARY KEY,
    server_id TEXT NOT NULL,
    identifier TEXT NOT NULL,
    payload_url TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '[]',
    interaction_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    last_seen_at TEXT,
    FOREIGN KEY(server_id) REFERENCES collaborator_servers(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS collaborator_interactions (
    id TEXT PRIMARY KEY,
    payload_id TEXT NOT NULL,
    interaction_type TEXT NOT NULL,
    source_ip TEXT NOT NULL,
    method TEXT,
    path TEXT,
    headers TEXT,
    raw_request TEXT,
    request_body TEXT,
    server_response TEXT,
    timestamp TEXT NOT NULL,
    FOREIGN KEY(payload_id) REFERENCES collaborator_payloads(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_collab_payloads_server ON collaborator_payloads(server_id);
CREATE INDEX IF NOT EXISTS idx_collab_payloads_status ON collaborator_payloads(status);
CREATE INDEX IF NOT EXISTS idx_collab_payloads_identifier ON collaborator_payloads(identifier);
CREATE INDEX IF NOT EXISTS idx_collab_interactions_payload ON collaborator_interactions(payload_id);
CREATE INDEX IF NOT EXISTS idx_collab_interactions_type ON collaborator_interactions(interaction_type);
CREATE INDEX IF NOT EXISTS idx_collab_interactions_ts ON collaborator_interactions(timestamp);
"#;

pub const CREATE_REGRESSION_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS regression_scripts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    target_url TEXT NOT NULL DEFAULT '',
    yaml TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS regression_script_runs (
    id TEXT PRIMARY KEY,
    script_id TEXT NOT NULL,
    status TEXT NOT NULL,
    conditions_json TEXT NOT NULL DEFAULT '[]',
    findings_json TEXT NOT NULL DEFAULT '[]',
    logs_json TEXT NOT NULL DEFAULT '[]',
    total_templates INTEGER NOT NULL DEFAULT 0,
    total_targets INTEGER NOT NULL DEFAULT 0,
    passed_conditions INTEGER NOT NULL DEFAULT 0,
    failed_conditions INTEGER NOT NULL DEFAULT 0,
    elapsed_millis INTEGER,
    started_at TEXT,
    finished_at TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY(script_id) REFERENCES regression_scripts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_regression_script_runs_script ON regression_script_runs(script_id);
CREATE INDEX IF NOT EXISTS idx_regression_script_runs_created ON regression_script_runs(created_at);
"#;

pub const CREATE_COLLECTIONS_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_collections_sort ON collections(sort_order, name);
"#;

pub const CREATE_STASHES_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS stashes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT,
    collection_id TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS stash_endpoints (
    id TEXT PRIMARY KEY,
    stash_id TEXT NOT NULL,
    name TEXT NOT NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    headers TEXT,
    body TEXT,
    body_type TEXT,
    pre_script TEXT,
    test_script TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(stash_id) REFERENCES stashes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_stashes_parent ON stashes(parent_id);
CREATE INDEX IF NOT EXISTS idx_stashes_collection ON stashes(collection_id);
CREATE INDEX IF NOT EXISTS idx_stash_endpoints_stash ON stash_endpoints(stash_id);
"#;

pub const CREATE_CONTEXTS_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS contexts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    variables TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

pub const CREATE_CHRONICLE_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS chronicle_logs (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    request_headers TEXT,
    request_body TEXT,
    response_status INTEGER,
    response_status_text TEXT,
    response_headers TEXT,
    response_body TEXT,
    duration_ms INTEGER
);

CREATE INDEX IF NOT EXISTS idx_chronicle_timestamp ON chronicle_logs(timestamp);
"#;

pub const CREATE_MOCK_FORGE_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS mock_domains (
    id TEXT PRIMARY KEY,
    hostname TEXT NOT NULL UNIQUE,
    ssl INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mock_routes (
    id TEXT PRIMARY KEY,
    domain_id TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    response_body TEXT NOT NULL,
    response_headers TEXT NOT NULL,
    matchers TEXT NOT NULL,
    chaos TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    matcher_enabled INTEGER NOT NULL DEFAULT 1,
    request_query_params TEXT,
    request_body TEXT,
    FOREIGN KEY(domain_id) REFERENCES mock_domains(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_mock_routes_domain_id ON mock_routes(domain_id);

INSERT OR IGNORE INTO mock_domains (id, hostname, ssl, status, created_at)
VALUES ('local_mock_server', 'localhost', 0, 'active', datetime('now'));
"#;

pub const CREATE_NOTES_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    note TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_updated_at ON notes(updated_at);
CREATE INDEX IF NOT EXISTS idx_notes_created_at ON notes(created_at);
"#;

pub const CREATE_HALO_AGENT_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS halo_agent_sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    preview TEXT NOT NULL DEFAULT '',
    updated_at INTEGER NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0,
    root_path TEXT,
    harness_session_id TEXT,
    model TEXT,
    plan_mode INTEGER NOT NULL DEFAULT 0,
    approval_mode TEXT NOT NULL DEFAULT 'ask',
    draft TEXT NOT NULL DEFAULT '',
    attachments_json TEXT NOT NULL DEFAULT '[]',
    todos_json TEXT NOT NULL DEFAULT '[]',
    file_changes_json TEXT NOT NULL DEFAULT '[]',
    queue_json TEXT NOT NULL DEFAULT '[]',
    todo_expanded INTEGER NOT NULL DEFAULT 1,
    todo_dismissed INTEGER NOT NULL DEFAULT 0,
    subagents_json TEXT NOT NULL DEFAULT '[]',
    skills_json TEXT NOT NULL DEFAULT '[]',
    mcp_servers_json TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS halo_agent_messages (
    session_id TEXT NOT NULL,
    ord INTEGER NOT NULL,
    id TEXT NOT NULL,
    role TEXT NOT NULL,
    parts_json TEXT NOT NULL,
    usage_json TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (session_id, ord),
    FOREIGN KEY(session_id) REFERENCES halo_agent_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_halo_agent_sessions_updated ON halo_agent_sessions(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_halo_agent_sessions_root ON halo_agent_sessions(root_path);
CREATE INDEX IF NOT EXISTS idx_halo_agent_messages_session ON halo_agent_messages(session_id);
"#;

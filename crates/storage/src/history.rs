use std::path::PathBuf;

use crate::db::repository::{
    Database, DocumentRecord, HttpSessionRecord, HttpSessionSummary, PaginatedResponse, TreeNode,
};
use halo_core::types::{
    AIInsight, ActivityLog, CollaboratorDashboardStats, CollaboratorInteraction,
    CollaboratorPayload, CollaboratorServer, CrawlPage, CrawlSession, ProxyFilter, ProxyRecord,
    WebSocketConnectionRecord, WebSocketFilter, WebSocketMessageRecord,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyLogSummary {
    pub id: String,
    pub session_id: String,
    pub timestamp: String,
    pub method: String,
    pub url: String,
    pub response_status: Option<u16>,
    pub response_status_text: Option<String>,
    pub response_content_type: Option<String>,
    pub request_body_size: usize,
    pub response_body_size: usize,
    pub server_addr: String,
    pub user_agent: Option<String>,
    pub host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConnectionSummary {
    pub id: String,
    pub session_id: String,
    pub timestamp: String,
    pub url: String,
    pub host: String,
    pub path: String,
    pub direction: String,
    pub state: String,
    pub message_count: u32,
    pub last_activity_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConnectionDetail {
    pub connection: WebSocketConnectionRecord,
    pub messages: Vec<WebSocketMessageRecord>,
}

#[derive(Clone)]
pub struct HistoryBridge {
    db: Database,
    payload_store: Option<crate::db::PayloadStore>,
}

impl HistoryBridge {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        let db = Database::new(path).map_err(|e| e.to_string())?;
        db.init().map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            payload_store: None,
        })
    }

    pub fn from_database(db: Database) -> Self {
        Self {
            db,
            payload_store: None,
        }
    }

    pub fn from_database_and_payload_store(
        db: Database,
        payload_store: crate::db::PayloadStore,
    ) -> Self {
        Self {
            db,
            payload_store: Some(payload_store),
        }
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub fn payload_store(&self) -> Option<&crate::db::PayloadStore> {
        self.payload_store.as_ref()
    }

    // ponytail: delegates to reset internal database connection
    pub fn close_connection(&self) -> Result<(), String> {
        self.db.close_connection().map_err(|e| e.to_string())
    }

    pub fn reopen_and_init(&self) -> Result<(), String> {
        self.db.reopen_and_init().map_err(|e| e.to_string())
    }

    // ── HTTP Sessions ──────────────────────────────────────────────

    pub fn create_http_session(
        &self,
        name: &str,
        description: Option<&str>,
        capture_mode: Option<&str>,
        capture_filter: Option<&str>,
        exclude_filter: Option<&str>,
        storage_mode: Option<&str>,
    ) -> Result<HttpSessionRecord, String> {
        self.db
            .create_http_session(
                name,
                description,
                capture_mode,
                capture_filter,
                exclude_filter,
                storage_mode,
            )
            .map_err(|e| e.to_string())
    }

    pub fn update_http_session_filter(
        &self,
        session_id: &str,
        capture_mode: &str,
        capture_filter: &str,
        exclude_filter: &str,
    ) -> Result<(), String> {
        self.db
            .update_http_session_filter(session_id, capture_mode, capture_filter, exclude_filter)
            .map_err(|e| e.to_string())
    }

    pub fn list_http_sessions(&self) -> Result<Vec<HttpSessionSummary>, String> {
        self.db.list_http_sessions().map_err(|e| e.to_string())
    }

    pub fn get_active_http_session(&self) -> Result<Option<HttpSessionRecord>, String> {
        self.db.get_active_http_session().map_err(|e| e.to_string())
    }

    pub fn set_active_http_session(&self, session_id: &str) -> Result<(), String> {
        self.db
            .set_active_http_session(session_id)
            .map_err(|e| e.to_string())
    }

    pub fn delete_http_session(&self, session_id: &str) -> Result<(), String> {
        // Reclaims persistent segment files for this session (no-op for slab refs).
        if let Some(ps) = &self.payload_store {
            let _ = ps.remove_session(session_id);
        }
        self.db
            .delete_http_session(session_id, self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    pub fn rename_http_session(&self, session_id: &str, name: &str) -> Result<(), String> {
        self.db
            .rename_http_session(session_id, name)
            .map_err(|e| e.to_string())
    }

    pub fn clear_http_session_logs(&self, session_id: &str) -> Result<usize, String> {
        // Reclaims persistent segment files for this session (no-op for slab refs).
        if let Some(ps) = &self.payload_store {
            let _ = ps.remove_session(session_id);
        }
        self.db
            .clear_http_session_logs(session_id, self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    // ── Logs ───────────────────────────────────────────────────────

    pub fn insert_record(
        &self,
        record: &ProxyRecord,
        session_id: Option<&str>,
    ) -> Result<(), String> {
        self.db
            .insert_log(record, session_id, self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    pub fn insert_records_batch(
        &self,
        records: &[(ProxyRecord, Option<String>)],
    ) -> Result<(), String> {
        self.db
            .insert_logs_batch(records, self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    pub fn upsert_crawler_session(&self, session: &CrawlSession) -> Result<(), String> {
        self.db
            .upsert_crawler_session(session)
            .map_err(|e| e.to_string())
    }

    pub fn get_crawler_session(&self, session_id: &str) -> Result<Option<CrawlSession>, String> {
        self.db
            .get_crawler_session(session_id)
            .map_err(|e| e.to_string())
    }

    pub fn list_recent_crawler_sessions(&self, limit: u32) -> Result<Vec<CrawlSession>, String> {
        self.db
            .list_recent_crawler_sessions(limit)
            .map_err(|e| e.to_string())
    }

    pub fn delete_crawler_session(&self, session_id: &str) -> Result<usize, String> {
        self.db
            .delete_crawler_session(session_id)
            .map_err(|e| e.to_string())
    }

    pub fn upsert_crawler_page(&self, page: &CrawlPage) -> Result<(), String> {
        self.db.upsert_crawler_page(page).map_err(|e| e.to_string())
    }

    pub fn insert_crawler_edge(
        &self,
        session_id: &str,
        from_url: &str,
        to_url: &str,
    ) -> Result<(), String> {
        self.db
            .insert_crawler_edge(session_id, from_url, to_url, "crawler")
            .map_err(|e| e.to_string())
    }

    pub fn list_crawler_pages(&self, session_id: &str) -> Result<Vec<CrawlPage>, String> {
        self.db
            .list_crawler_pages(session_id)
            .map_err(|e| e.to_string())
    }

    pub fn clear_crawler_artifact_paths(&self) -> Result<usize, String> {
        self.db
            .clear_crawler_artifact_paths()
            .map_err(|e| e.to_string())
    }

    pub fn insert_crawler_insight(&self, insight: &AIInsight) -> Result<(), String> {
        self.db
            .insert_crawler_insight(insight)
            .map_err(|e| e.to_string())
    }

    pub fn list_crawler_insights(&self, session_id: &str) -> Result<Vec<AIInsight>, String> {
        self.db
            .list_crawler_insights(session_id)
            .map_err(|e| e.to_string())
    }

    pub fn insert_crawler_log(&self, log: &ActivityLog) -> Result<(), String> {
        self.db.insert_crawler_log(log).map_err(|e| e.to_string())
    }

    pub fn list_crawler_logs(&self, session_id: &str) -> Result<Vec<ActivityLog>, String> {
        self.db
            .list_crawler_logs(session_id)
            .map_err(|e| e.to_string())
    }

    pub fn get_documents(&self) -> Result<Vec<DocumentRecord>, String> {
        self.db.get_documents().map_err(|e| e.to_string())
    }

    pub fn save_document(&self, document: &DocumentRecord) -> Result<(), String> {
        self.db.upsert_document(document).map_err(|e| e.to_string())
    }

    pub fn delete_document(&self, document_id: &str) -> Result<(), String> {
        self.db
            .delete_document(document_id)
            .map_err(|e| e.to_string())
    }

    // --- Collections ---
    pub fn get_collections(&self) -> Result<Vec<crate::CollectionRecord>, String> {
        self.db.get_collections().map_err(|e| e.to_string())
    }

    pub fn save_collection(&self, record: &crate::CollectionRecord) -> Result<(), String> {
        self.db.upsert_collection(record).map_err(|e| e.to_string())
    }

    pub fn delete_collection(&self, id: &str) -> Result<(), String> {
        self.db.delete_collection(id).map_err(|e| e.to_string())
    }

    // --- Stashes (folders) ---
    pub fn get_stashes(&self) -> Result<Vec<crate::StashRecord>, String> {
        self.db.get_stashes().map_err(|e| e.to_string())
    }

    pub fn save_stash(&self, record: &crate::StashRecord) -> Result<(), String> {
        self.db.upsert_stash(record).map_err(|e| e.to_string())
    }

    pub fn delete_stash(&self, id: &str) -> Result<(), String> {
        self.db.delete_stash(id).map_err(|e| e.to_string())
    }

    // --- Stash Endpoints ---
    pub fn get_stash_endpoints(&self) -> Result<Vec<crate::StashEndpointRecord>, String> {
        self.db.get_stash_endpoints().map_err(|e| e.to_string())
    }

    pub fn save_stash_endpoint(&self, record: &crate::StashEndpointRecord) -> Result<(), String> {
        self.db
            .upsert_stash_endpoint(record)
            .map_err(|e| e.to_string())
    }

    pub fn delete_stash_endpoint(&self, id: &str) -> Result<(), String> {
        self.db.delete_stash_endpoint(id).map_err(|e| e.to_string())
    }

    // --- Contexts (environments) ---
    pub fn get_contexts(&self) -> Result<Vec<crate::ContextRecord>, String> {
        self.db.get_contexts().map_err(|e| e.to_string())
    }

    pub fn save_context(&self, record: &crate::ContextRecord) -> Result<(), String> {
        self.db.upsert_context(record).map_err(|e| e.to_string())
    }

    pub fn delete_context(&self, id: &str) -> Result<(), String> {
        self.db.delete_context(id).map_err(|e| e.to_string())
    }

    // --- Chronicle (request history) ---
    pub fn get_chronicle_logs(&self, limit: u32) -> Result<Vec<crate::ChronicleLogRecord>, String> {
        self.db.get_chronicle_logs(limit).map_err(|e| e.to_string())
    }

    pub fn add_chronicle_log(&self, record: &crate::ChronicleLogRecord) -> Result<(), String> {
        self.db.add_chronicle_log(record).map_err(|e| e.to_string())
    }

    pub fn clear_chronicle_logs(&self) -> Result<(), String> {
        self.db.clear_chronicle_logs().map_err(|e| e.to_string())
    }

    pub fn clear_all(&self) -> Result<(), String> {
        if let Some(ps) = &self.payload_store {
            let _ = ps.clear_all_persistent();
            ps.clear_ephemeral();
        }
        self.db.clear_logs().map_err(|e| e.to_string())
    }

    pub fn clear_before(&self, cutoff_rfc3339: &str) -> Result<usize, String> {
        self.db
            .clear_logs_before_with_store(cutoff_rfc3339, self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    pub fn delete_by_id(&self, log_id: &str) -> Result<(), String> {
        self.db
            .delete_log(log_id, self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    pub fn get_all(&self) -> Result<Vec<ProxyRecord>, String> {
        self.db
            .get_all(self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    pub fn get_by_id(&self, log_id: &str) -> Result<Option<ProxyRecord>, String> {
        self.db
            .get_by_id(log_id, self.payload_store.as_ref())
            .map_err(|e| e.to_string())
    }

    pub fn get_filtered(&self, filter: ProxyFilter) -> Result<Vec<ProxyRecord>, String> {
        let filter = self.normalize_filter(filter);

        if self.has_active_filters(&filter) {
            self.db
                .get_filtered(&filter, self.payload_store.as_ref())
                .map_err(|e| e.to_string())
        } else {
            self.db
                .get_all(self.payload_store.as_ref())
                .map_err(|e| e.to_string())
        }
    }

    pub fn get_recent(
        &self,
        limit: u32,
        filter: Option<ProxyFilter>,
        sort_order: Option<String>,
    ) -> Result<Vec<ProxyLogSummary>, String> {
        let filter = filter.map(|f| self.normalize_filter(f));
        let sort_order = self.normalize_sort_order(sort_order.as_deref());

        let result = match filter {
            Some(filter) if self.has_active_filters(&filter) => self
                .db
                .get_filtered_summary_recent(&filter, limit, sort_order),
            Some(ref filter) if filter.session_id.is_some() => self
                .db
                .get_filtered_summary_recent(filter, limit, sort_order),
            _ => self.db.get_summary_recent(None, limit, sort_order),
        }?;

        Ok(result
            .into_iter()
            .map(|r| ProxyLogSummary {
                id: r.id,
                session_id: r.session_id,
                timestamp: r.timestamp,
                method: r.method,
                url: r.url,
                response_status: r.response_status,
                response_status_text: r.response_status_text,
                request_body_size: r.request_body_size,
                response_body_size: r.response_body_size,
                server_addr: r.server_addr,
                user_agent: r.user_agent,
                host: r.host,
                response_content_type: r.response_content_type,
            })
            .collect())
    }

    pub fn get_tree(&self, filter: Option<ProxyFilter>) -> Result<Vec<TreeNode>, String> {
        let filter = self.normalize_filter(filter.unwrap_or_default());
        self.db.get_tree(&filter)
    }

    pub fn insert_websocket_connection(
        &self,
        record: &WebSocketConnectionRecord,
    ) -> Result<(), String> {
        self.db
            .insert_websocket_connection(record)
            .map_err(|e| e.to_string())
    }

    pub fn insert_websocket_message(&self, record: &WebSocketMessageRecord) -> Result<(), String> {
        self.db
            .insert_websocket_message(record)
            .map_err(|e| e.to_string())
    }

    pub fn clear_websocket_all(&self) -> Result<(), String> {
        self.db.clear_websocket_logs().map_err(|e| e.to_string())
    }

    pub fn delete_websocket_connection(&self, id: &str) -> Result<(), String> {
        self.db
            .delete_websocket_connection(id)
            .map_err(|e| e.to_string())
    }

    pub fn get_websocket_paginated(
        &self,
        page: u32,
        per_page: u32,
        filter: Option<WebSocketFilter>,
    ) -> Result<PaginatedResponse<WebSocketConnectionSummary>, String> {
        let filter = filter.map(|value| self.normalize_websocket_filter(value));

        let result = self
            .db
            .get_websocket_paginated(filter.as_ref(), page, per_page)?;

        Ok(PaginatedResponse {
            data: result
                .data
                .into_iter()
                .map(WebSocketConnectionSummary::from)
                .collect(),
            total: result.total,
            page: result.page,
            per_page: result.per_page,
            has_more: result.has_more,
        })
    }

    pub fn get_websocket_detail(
        &self,
        connection_id: &str,
    ) -> Result<Option<WebSocketConnectionDetail>, String> {
        let connection = match self.db.get_websocket_by_id(connection_id)? {
            Some(connection) => connection,
            None => return Ok(None),
        };

        let messages = self
            .db
            .get_websocket_messages_by_connection_id(connection_id)?;

        Ok(Some(WebSocketConnectionDetail {
            connection,
            messages,
        }))
    }

    fn normalize_sort_order(&self, sort_order: Option<&str>) -> &str {
        match sort_order.unwrap_or("DESC").to_uppercase().as_str() {
            "ASC" => "ASC",
            _ => "DESC",
        }
    }

    fn normalize_filter(&self, filter: ProxyFilter) -> ProxyFilter {
        ProxyFilter {
            search: normalize_optional_string(filter.search),
            path: normalize_optional_string(filter.path),
            methods: normalize_string_vec(filter.methods),
            status_codes: normalize_u16_vec(filter.status_codes),
            scope: normalize_string_vec(filter.scope),
            session_id: normalize_optional_string(filter.session_id),
        }
    }

    fn has_active_filters(&self, filter: &ProxyFilter) -> bool {
        filter.search.is_some()
            || filter.path.is_some()
            || filter.methods.is_some()
            || filter.status_codes.is_some()
            || filter.scope.is_some()
    }

    fn normalize_websocket_filter(&self, filter: WebSocketFilter) -> WebSocketFilter {
        WebSocketFilter {
            search: normalize_optional_string(filter.search),
            scope: normalize_string_vec(filter.scope),
            states: normalize_string_vec(filter.states),
            session_id: normalize_optional_string(filter.session_id),
        }
    }

    // ── Collaborator ──────────────────────────────────────────────

    pub fn list_collaborator_servers(&self) -> Result<Vec<CollaboratorServer>, String> {
        self.db
            .list_collaborator_servers()
            .map_err(|e| e.to_string())
    }

    pub fn get_collaborator_server(&self, id: &str) -> Result<Option<CollaboratorServer>, String> {
        self.db
            .get_collaborator_server(id)
            .map_err(|e| e.to_string())
    }

    pub fn insert_collaborator_server(&self, s: &CollaboratorServer) -> Result<(), String> {
        self.db
            .insert_collaborator_server(s)
            .map_err(|e| e.to_string())
    }

    pub fn update_collaborator_server(&self, s: &CollaboratorServer) -> Result<(), String> {
        self.db
            .update_collaborator_server(s)
            .map_err(|e| e.to_string())
    }

    pub fn delete_collaborator_server(&self, id: &str) -> Result<(), String> {
        self.db
            .delete_collaborator_server(id)
            .map_err(|e| e.to_string())
    }

    pub fn insert_collaborator_payload(&self, p: &CollaboratorPayload) -> Result<(), String> {
        self.db
            .insert_collaborator_payload(p)
            .map_err(|e| e.to_string())
    }

    pub fn list_collaborator_payloads(
        &self,
        sid: Option<&str>,
    ) -> Result<Vec<CollaboratorPayload>, String> {
        self.db
            .list_collaborator_payloads(sid)
            .map_err(|e| e.to_string())
    }

    pub fn get_collaborator_payload(
        &self,
        id: &str,
    ) -> Result<Option<CollaboratorPayload>, String> {
        self.db
            .get_collaborator_payload(id)
            .map_err(|e| e.to_string())
    }

    pub fn update_collaborator_payload_status(&self, id: &str, status: &str) -> Result<(), String> {
        self.db
            .update_collaborator_payload_status(id, status)
            .map_err(|e| e.to_string())
    }

    pub fn delete_collaborator_payload(&self, id: &str) -> Result<(), String> {
        self.db
            .delete_collaborator_payload(id)
            .map_err(|e| e.to_string())
    }

    pub fn increment_collaborator_payload_interactions(
        &self,
        id: &str,
        count: i64,
    ) -> Result<(), String> {
        self.db
            .increment_collaborator_payload_interactions(id, count)
            .map_err(|e| e.to_string())
    }

    pub fn insert_collaborator_interaction(
        &self,
        i: &CollaboratorInteraction,
    ) -> Result<(), String> {
        self.db
            .insert_collaborator_interaction(i)
            .map_err(|e| e.to_string())
    }

    pub fn list_collaborator_interactions(
        &self,
        payload_id: Option<&str>,
        interaction_type: Option<&str>,
    ) -> Result<Vec<CollaboratorInteraction>, String> {
        self.db
            .list_collaborator_interactions(payload_id, interaction_type)
            .map_err(|e| e.to_string())
    }

    pub fn get_collaborator_interaction(
        &self,
        id: &str,
    ) -> Result<Option<CollaboratorInteraction>, String> {
        self.db
            .get_collaborator_interaction(id)
            .map_err(|e| e.to_string())
    }

    pub fn get_collaborator_dashboard_stats(&self) -> Result<CollaboratorDashboardStats, String> {
        self.db
            .get_collaborator_dashboard_stats()
            .map_err(|e| e.to_string())
    }

    // ── Halo Agent sessions ─────────────────────────────────────

    pub fn list_halo_agent_sessions(&self) -> Result<Vec<crate::HaloAgentSessionRecord>, String> {
        self.db
            .list_halo_agent_sessions()
            .map_err(|e| e.to_string())
    }

    pub fn upsert_halo_agent_session(
        &self,
        record: &crate::HaloAgentSessionRecord,
    ) -> Result<(), String> {
        self.db
            .upsert_halo_agent_session(record)
            .map_err(|e| e.to_string())
    }

    pub fn delete_halo_agent_session(&self, id: &str) -> Result<(), String> {
        self.db
            .delete_halo_agent_session(id)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    // ── Notes ────────────────────────────────────────────────────

    pub fn list_notes(&self) -> Result<Vec<crate::NoteRecord>, String> {
        self.db.list_notes().map_err(|e| e.to_string())
    }

    pub fn upsert_note(&self, record: &crate::NoteRecord) -> Result<crate::NoteRecord, String> {
        self.db.upsert_note(record).map_err(|e| e.to_string())
    }

    pub fn import_notes(&self, records: &[crate::NoteRecord]) -> Result<usize, String> {
        self.db.import_notes(records).map_err(|e| e.to_string())
    }

    pub fn delete_note(&self, id: &str) -> Result<usize, String> {
        self.db.delete_note(id).map_err(|e| e.to_string())
    }

    pub fn delete_notes(&self, ids: &[String]) -> Result<usize, String> {
        self.db.delete_notes(ids).map_err(|e| e.to_string())
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_string_vec(values: Option<Vec<String>>) -> Option<Vec<String>> {
    values.and_then(|items| {
        let normalized: Vec<String> = items
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();

        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    })
}

fn normalize_u16_vec(values: Option<Vec<u16>>) -> Option<Vec<u16>> {
    values.filter(|items| !items.is_empty())
}

impl From<WebSocketConnectionRecord> for WebSocketConnectionSummary {
    fn from(record: WebSocketConnectionRecord) -> Self {
        Self {
            id: record.id.to_string(),
            session_id: record.session_id,
            timestamp: record.timestamp.to_rfc3339(),
            url: record.url,
            host: record.host,
            path: record.path,
            direction: "→ server".to_string(),
            state: format!("{:?}", record.state).to_lowercase(),
            message_count: record.message_count,
            last_activity_at: record.last_activity_at.to_rfc3339(),
        }
    }
}

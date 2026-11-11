use halo_core::types::{
    CollaboratorDashboardStats, CollaboratorInteraction, CollaboratorPayload, CollaboratorServer,
};
use rusqlite::{OptionalExtension, Result as SqlResult, params};

use super::Database;

impl Database {
    pub fn insert_collaborator_server(&self, server: &CollaboratorServer) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO collaborator_servers (id, name, url, api_key, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![server.id, server.name, server.url, server.api_key, server.status, server.created_at, server.updated_at],
        )?;
        Ok(())
    }

    pub fn update_collaborator_server(&self, server: &CollaboratorServer) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE collaborator_servers SET name = ?2, url = ?3, api_key = ?4, status = ?5, updated_at = ?6 WHERE id = ?1",
            params![server.id, server.name, server.url, server.api_key, server.status, server.updated_at],
        )?;
        Ok(())
    }

    pub fn delete_collaborator_server(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM collaborator_servers WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn list_collaborator_servers(&self) -> SqlResult<Vec<CollaboratorServer>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, api_key, status, created_at, updated_at FROM collaborator_servers ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CollaboratorServer {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                api_key: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        rows.collect()
    }

    pub fn get_collaborator_server(&self, id: &str) -> SqlResult<Option<CollaboratorServer>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, name, url, api_key, status, created_at, updated_at FROM collaborator_servers WHERE id = ?1",
            params![id],
            |row| {
                Ok(CollaboratorServer {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    url: row.get(2)?,
                    api_key: row.get(3)?,
                    status: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .optional()
    }

    pub fn insert_collaborator_payload(&self, p: &CollaboratorPayload) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO collaborator_payloads (id, server_id, identifier, payload_url, name, description, tags, interaction_count, status, created_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![p.id, p.server_id, p.identifier, p.payload_url, p.name, p.description, p.tags, p.interaction_count, p.status, p.created_at, p.last_seen_at],
        )?;
        Ok(())
    }

    pub fn list_collaborator_payloads(
        &self,
        server_id: Option<&str>,
    ) -> SqlResult<Vec<CollaboratorPayload>> {
        let conn = self.conn.lock();
        let result = match server_id {
            Some(sid) => {
                let mut stmt = conn.prepare(
                    "SELECT id, server_id, identifier, payload_url, name, description, tags, interaction_count, status, created_at, last_seen_at FROM collaborator_payloads WHERE server_id = ?1 ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map(params![sid], Self::map_collab_payload)?;
                rows.collect::<Result<Vec<_>, _>>()?
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, server_id, identifier, payload_url, name, description, tags, interaction_count, status, created_at, last_seen_at FROM collaborator_payloads ORDER BY created_at DESC",
                )?;
                let rows = stmt.query_map([], Self::map_collab_payload)?;
                rows.collect::<Result<Vec<_>, _>>()?
            }
        };
        Ok(result)
    }

    fn map_collab_payload(row: &rusqlite::Row) -> rusqlite::Result<CollaboratorPayload> {
        Ok(CollaboratorPayload {
            id: row.get(0)?,
            server_id: row.get(1)?,
            identifier: row.get(2)?,
            payload_url: row.get(3)?,
            name: row.get(4)?,
            description: row.get(5)?,
            tags: row.get(6)?,
            interaction_count: row.get(7)?,
            status: row.get(8)?,
            created_at: row.get(9)?,
            last_seen_at: row.get(10)?,
        })
    }

    pub fn get_collaborator_payload(&self, id: &str) -> SqlResult<Option<CollaboratorPayload>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, server_id, identifier, payload_url, name, description, tags, interaction_count, status, created_at, last_seen_at FROM collaborator_payloads WHERE id = ?1",
            params![id],
            Self::map_collab_payload,
        )
        .optional()
    }

    pub fn update_collaborator_payload_status(&self, id: &str, status: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE collaborator_payloads SET status = ?2 WHERE id = ?1",
            params![id, status],
        )?;
        Ok(())
    }

    pub fn delete_collaborator_payload(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM collaborator_payloads WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn increment_collaborator_payload_interactions(
        &self,
        id: &str,
        count: i64,
    ) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE collaborator_payloads SET interaction_count = interaction_count + ?2, last_seen_at = ?3 WHERE id = ?1",
            params![id, count, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn insert_collaborator_interaction(&self, i: &CollaboratorInteraction) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO collaborator_interactions (id, payload_id, interaction_type, source_ip, method, path, headers, raw_request, request_body, server_response, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![i.id, i.payload_id, i.interaction_type, i.source_ip, i.method, i.path, i.headers, i.raw_request, i.request_body, i.server_response, i.timestamp],
        )?;
        Ok(())
    }

    pub fn list_collaborator_interactions(
        &self,
        payload_id: Option<&str>,
        interaction_type: Option<&str>,
    ) -> SqlResult<Vec<CollaboratorInteraction>> {
        let conn = self.conn.lock();
        let mut sql = String::from(
            "SELECT id, payload_id, interaction_type, source_ip, method, path, headers, raw_request, request_body, server_response, timestamp FROM collaborator_interactions WHERE 1=1",
        );
        let mut params_vec: Vec<String> = Vec::new();

        if let Some(pid) = payload_id {
            params_vec.push(pid.to_string());
            sql.push_str(&format!(" AND payload_id = ?{}", params_vec.len()));
        }
        if let Some(itype) = interaction_type {
            params_vec.push(itype.to_string());
            sql.push_str(&format!(" AND interaction_type = ?{}", params_vec.len()));
        }
        sql.push_str(" ORDER BY timestamp DESC");

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(CollaboratorInteraction {
                id: row.get(0)?,
                payload_id: row.get(1)?,
                interaction_type: row.get(2)?,
                source_ip: row.get(3)?,
                method: row.get(4)?,
                path: row.get(5)?,
                headers: row.get(6)?,
                raw_request: row.get(7)?,
                request_body: row.get(8)?,
                server_response: row.get(9)?,
                timestamp: row.get(10)?,
            })
        })?;
        let result: Vec<CollaboratorInteraction> = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(result)
    }

    pub fn get_collaborator_interaction(
        &self,
        id: &str,
    ) -> SqlResult<Option<CollaboratorInteraction>> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, payload_id, interaction_type, source_ip, method, path, headers, raw_request, request_body, server_response, timestamp FROM collaborator_interactions WHERE id = ?1",
            params![id],
            |row| {
                Ok(CollaboratorInteraction {
                    id: row.get(0)?,
                    payload_id: row.get(1)?,
                    interaction_type: row.get(2)?,
                    source_ip: row.get(3)?,
                    method: row.get(4)?,
                    path: row.get(5)?,
                    headers: row.get(6)?,
                    raw_request: row.get(7)?,
                    request_body: row.get(8)?,
                    server_response: row.get(9)?,
                    timestamp: row.get(10)?,
                })
            },
        )
        .optional()
    }

    pub fn get_collaborator_dashboard_stats(&self) -> SqlResult<CollaboratorDashboardStats> {
        let conn = self.conn.lock();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        let active_payloads: i64 = conn.query_row(
            "SELECT COUNT(*) FROM collaborator_payloads WHERE status = 'active'",
            [],
            |r| r.get(0),
        )?;
        let interactions_today: i64 = conn.query_row(
            "SELECT COUNT(*) FROM collaborator_interactions WHERE timestamp >= ?1",
            params![today],
            |r| r.get(0),
        )?;
        let dns_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM collaborator_interactions WHERE interaction_type = 'dns'",
            [],
            |r| r.get(0),
        )?;
        let http_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM collaborator_interactions WHERE interaction_type = 'http'",
            [],
            |r| r.get(0),
        )?;
        let https_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM collaborator_interactions WHERE interaction_type = 'https'",
            [],
            |r| r.get(0),
        )?;
        let last_callback: Option<String> = conn.query_row(
            "SELECT MAX(timestamp) FROM collaborator_interactions",
            [],
            |r| r.get(0),
        )?;
        let connected_servers: i64 = conn.query_row(
            "SELECT COUNT(*) FROM collaborator_servers WHERE status = 'connected'",
            [],
            |r| r.get(0),
        )?;

        Ok(CollaboratorDashboardStats {
            active_payloads,
            interactions_today,
            dns_events,
            http_events,
            https_events,
            last_callback,
            connected_servers,
        })
    }
}

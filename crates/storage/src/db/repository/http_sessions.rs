use rusqlite::{Connection, Result as SqlResult, params};
use serde::{Deserialize, Serialize};

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpSessionRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
    pub description: Option<String>,
    #[serde(default = "default_capture_mode")]
    pub capture_mode: String,
    #[serde(default = "default_filter_json")]
    pub capture_filter: String,
    #[serde(default = "default_filter_json")]
    pub exclude_filter: String,
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpSessionSummary {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
    pub description: Option<String>,
    #[serde(default = "default_capture_mode")]
    pub capture_mode: String,
    #[serde(default = "default_filter_json")]
    pub capture_filter: String,
    #[serde(default = "default_filter_json")]
    pub exclude_filter: String,
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
    pub request_count: usize,
    pub total_size_bytes: usize,
}

fn default_capture_mode() -> String {
    "all".to_string()
}

fn default_filter_json() -> String {
    "[]".to_string()
}

fn default_storage_mode() -> String {
    "persistent".to_string()
}

impl Database {
    pub fn create_http_session(
        &self,
        name: &str,
        description: Option<&str>,
        capture_mode: Option<&str>,
        capture_filter: Option<&str>,
        exclude_filter: Option<&str>,
        storage_mode: Option<&str>,
    ) -> SqlResult<HttpSessionRecord> {
        let conn = self.conn.lock();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let mode = capture_mode.unwrap_or("all").trim();
        let cap_filter = capture_filter.unwrap_or("[]").trim();
        let exc_filter = exclude_filter.unwrap_or("[]").trim();
        let store_mode = storage_mode.unwrap_or("persistent").trim();

        // Mark existing sessions as inactive
        conn.execute("UPDATE http_sessions SET is_active = 0", [])?;

        conn.execute(
            "INSERT INTO http_sessions (id, name, created_at, updated_at, is_active, description, capture_mode, capture_filter, exclude_filter, storage_mode) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                name.trim(),
                now,
                now,
                description.map(|d| d.trim()),
                mode,
                cap_filter,
                exc_filter,
                store_mode
            ],
        )?;

        Ok(HttpSessionRecord {
            id,
            name: name.trim().to_string(),
            created_at: now.clone(),
            updated_at: now,
            is_active: true,
            description: description.map(|d| d.trim().to_string()),
            capture_mode: mode.to_string(),
            capture_filter: cap_filter.to_string(),
            exclude_filter: exc_filter.to_string(),
            storage_mode: store_mode.to_string(),
        })
    }

    pub fn update_http_session_filter(
        &self,
        session_id: &str,
        capture_mode: &str,
        capture_filter: &str,
        exclude_filter: &str,
    ) -> SqlResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE http_sessions SET capture_mode = ?1, capture_filter = ?2, exclude_filter = ?3, updated_at = ?4 WHERE id = ?5",
            params![capture_mode.trim(), capture_filter.trim(), exclude_filter.trim(), now, session_id],
        )?;
        Ok(())
    }

    pub fn get_session_storage_mode(&self, session_id: Option<&str>) -> String {
        let conn = self.conn.lock();
        if let Some(sid) = session_id {
            if !sid.is_empty() {
                if let Ok(mode) = conn.query_row(
                    "SELECT COALESCE(storage_mode, 'persistent') FROM http_sessions WHERE id = ?1",
                    params![sid],
                    |r| r.get::<_, String>(0),
                ) {
                    return mode;
                }
            }
        }

        // Default to active session's storage mode
        conn.query_row(
            "SELECT COALESCE(storage_mode, 'persistent') FROM http_sessions WHERE is_active = 1 LIMIT 1",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "persistent".to_string())
    }

    pub fn list_http_sessions(&self) -> SqlResult<Vec<HttpSessionSummary>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT 
                s.id, s.name, s.created_at, s.updated_at, s.is_active, s.description,
                COALESCE(s.capture_mode, 'all') as capture_mode,
                COALESCE(s.capture_filter, '[]') as capture_filter,
                COALESCE(s.exclude_filter, '[]') as exclude_filter,
                COALESCE(s.storage_mode, 'persistent') as storage_mode,
                COALESCE(COUNT(l.id), 0) as request_count,
                COALESCE(SUM(COALESCE(l.req_body_size, LENGTH(COALESCE(l.request_body, X''))) + COALESCE(l.res_body_size, LENGTH(COALESCE(l.response_body, X'')))), 0) as total_size_bytes
               FROM http_sessions s
               LEFT JOIN http_logs l ON s.id = l.session_id
               GROUP BY s.id
               ORDER BY s.is_active DESC, s.created_at DESC"#,
        )?;

        let rows = stmt.query_map([], |row| {
            let is_active_int: i64 = row.get(4)?;
            let req_count: i64 = row.get(10)?;
            let total_size: i64 = row.get(11)?;

            Ok(HttpSessionSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                is_active: is_active_int == 1,
                description: row.get(5)?,
                capture_mode: row.get(6)?,
                capture_filter: row.get(7)?,
                exclude_filter: row.get(8)?,
                storage_mode: row.get(9)?,
                request_count: req_count as usize,
                total_size_bytes: total_size as usize,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            let mut summary = row?;
            // If the session is ephemeral, check ephemeral_conn for live count & size
            if summary.storage_mode == "ephemeral" {
                let eph = self.ephemeral_conn.lock();
                {
                    let counts: Result<(i64, i64), _> = eph.query_row(
                        r#"SELECT COUNT(id),
                                  COALESCE(SUM(COALESCE(req_body_size, 0) + COALESCE(res_body_size, 0)), 0)
                           FROM http_logs
                           WHERE session_id = ?1"#,
                        params![summary.id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    );
                    if let Ok((c, sz)) = counts {
                        summary.request_count = c as usize;
                        summary.total_size_bytes = sz as usize;
                    }
                }
            }
            results.push(summary);
        }
        Ok(results)
    }

    pub fn get_active_http_session(&self) -> SqlResult<Option<HttpSessionRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at, updated_at, is_active, description, COALESCE(capture_mode, 'all'), COALESCE(capture_filter, '[]'), COALESCE(exclude_filter, '[]'), COALESCE(storage_mode, 'persistent') FROM http_sessions WHERE is_active = 1 LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;

        if let Some(row) = rows.next()? {
            let is_active_int: i64 = row.get(4)?;
            Ok(Some(HttpSessionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                is_active: is_active_int == 1,
                description: row.get(5)?,
                capture_mode: row.get(6)?,
                capture_filter: row.get(7)?,
                exclude_filter: row.get(8)?,
                storage_mode: row.get(9)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn get_active_session_id(conn: &rusqlite::Connection) -> Option<String> {
        conn.query_row(
            "SELECT id FROM http_sessions WHERE is_active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok()
    }

    pub fn set_active_http_session(&self, session_id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("UPDATE http_sessions SET is_active = 0", [])?;
        let rows = conn.execute(
            "UPDATE http_sessions SET is_active = 1, updated_at = ?1 WHERE id = ?2",
            params![chrono::Utc::now().to_rfc3339(), session_id],
        )?;

        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub fn rename_http_session(&self, session_id: &str, name: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE http_sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name.trim(), now, session_id],
        )?;
        Ok(())
    }

    pub fn clear_http_session_logs(
        &self,
        session_id: &str,
        payload_store: Option<&crate::db::payload_store::PayloadStore>,
    ) -> SqlResult<usize> {
        // Clear from disk DB
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let rows = tx.execute(
            "DELETE FROM http_logs WHERE session_id = ?1",
            params![session_id],
        )?;
        if let Err(e) = tx.execute("DELETE FROM websocket_messages WHERE connection_id IN (SELECT id FROM websocket_connections WHERE session_id = ?1)", params![session_id]) {
            eprintln!("[db] clear_http_session_logs: failed to delete websocket_messages: {}", e);
        }
        if let Err(e) = tx.execute(
            "DELETE FROM websocket_connections WHERE session_id = ?1",
            params![session_id],
        ) {
            eprintln!(
                "[db] clear_http_session_logs: failed to delete websocket_connections: {}",
                e
            );
        }
        tx.commit()?;
        drop(conn);

        // Clear from ephemeral DB as well
        let mut eph = self.ephemeral_conn.lock();
        let mut eph_rows = 0;
        // Collect ephemeral slab refs first so the stored bodies can be reclaimed.
        let mut payload_refs: Vec<String> = Vec::new();
        if let Ok(mut stmt) = eph
            .prepare("SELECT req_payload_ref, res_payload_ref FROM http_logs WHERE session_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for pair in rows.flatten() {
                    payload_refs.extend([pair.0, pair.1]);
                }
            }
        }
        if let Ok(tx) = eph.transaction() {
            eph_rows = tx
                .execute(
                    "DELETE FROM http_logs WHERE session_id = ?1",
                    params![session_id],
                )
                .unwrap_or(0);
            if let Err(e) = tx.execute("DELETE FROM websocket_messages WHERE connection_id IN (SELECT id FROM websocket_connections WHERE session_id = ?1)", params![session_id]) {
                eprintln!("[db] clear_http_session_logs: failed to delete ephemeral websocket_messages: {}", e);
            }
            if let Err(e) = tx.execute(
                "DELETE FROM websocket_connections WHERE session_id = ?1",
                params![session_id],
            ) {
                eprintln!(
                    "[db] clear_http_session_logs: failed to delete ephemeral websocket_connections: {}",
                    e
                );
            }
            let _ = tx.commit();
        }
        drop(eph);
        Self::release_payload_refs(payload_refs, payload_store);

        // Run vacuum to reclaim deleted space in disk DB
        let _ = self.vacuum();
        Ok(rows + eph_rows)
    }

    pub fn delete_http_session(
        &self,
        session_id: &str,
        payload_store: Option<&crate::db::payload_store::PayloadStore>,
    ) -> SqlResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        // Find if this was the active session
        let was_active: bool = tx
            .query_row(
                "SELECT is_active FROM http_sessions WHERE id = ?1",
                params![session_id],
                |row| {
                    let v: i64 = row.get(0)?;
                    Ok(v == 1)
                },
            )
            .unwrap_or(false);

        // Delete child tables first to avoid foreign key issues
        if let Err(e) = tx.execute("DELETE FROM websocket_messages WHERE connection_id IN (SELECT id FROM websocket_connections WHERE session_id = ?1)", params![session_id]) {
            eprintln!("[db] delete_http_session: failed to delete websocket_messages: {}", e);
        }
        if let Err(e) = tx.execute(
            "DELETE FROM websocket_connections WHERE session_id = ?1",
            params![session_id],
        ) {
            eprintln!(
                "[db] delete_http_session: failed to delete websocket_connections: {}",
                e
            );
        }
        if let Err(e) = tx.execute(
            "DELETE FROM http_logs WHERE session_id = ?1",
            params![session_id],
        ) {
            eprintln!(
                "[db] delete_http_session: failed to delete http_logs: {}",
                e
            );
        }
        tx.execute(
            "DELETE FROM http_sessions WHERE id = ?1",
            params![session_id],
        )?;

        // If the active session was deleted, make the latest remaining session active or create a new one
        if was_active {
            let next_id: Option<String> = tx
                .query_row(
                    "SELECT id FROM http_sessions ORDER BY created_at DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .ok();

            if let Some(nid) = next_id {
                tx.execute(
                    "UPDATE http_sessions SET is_active = 1 WHERE id = ?1",
                    params![nid],
                )?;
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();
                let name = format!("Session - {}", chrono::Local::now().format("%d %b %H:%M"));
                tx.execute(
                    "INSERT INTO http_sessions (id, name, created_at, updated_at, is_active, description, storage_mode) VALUES (?1, ?2, ?3, ?4, 1, 'Default session', 'persistent')",
                    params![id, name, now, now],
                )?;
            }
        }

        tx.commit()?;
        drop(conn);

        // Clean up from ephemeral DB
        let mut eph = self.ephemeral_conn.lock();
        // Collect ephemeral slab refs first so the stored bodies can be reclaimed.
        let mut payload_refs: Vec<String> = Vec::new();
        if let Ok(mut stmt) = eph
            .prepare("SELECT req_payload_ref, res_payload_ref FROM http_logs WHERE session_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for pair in rows.flatten() {
                    payload_refs.extend([pair.0, pair.1]);
                }
            }
        }
        if let Ok(tx) = eph.transaction() {
            if let Err(e) = tx.execute(
                "DELETE FROM http_logs WHERE session_id = ?1",
                params![session_id],
            ) {
                eprintln!(
                    "[db] delete_http_session: failed to delete ephemeral http_logs: {}",
                    e
                );
            }
            if let Err(e) = tx.execute("DELETE FROM websocket_messages WHERE connection_id IN (SELECT id FROM websocket_connections WHERE session_id = ?1)", params![session_id]) {
                eprintln!("[db] delete_http_session: failed to delete ephemeral websocket_messages: {}", e);
            }
            if let Err(e) = tx.execute(
                "DELETE FROM websocket_connections WHERE session_id = ?1",
                params![session_id],
            ) {
                eprintln!(
                    "[db] delete_http_session: failed to delete ephemeral websocket_connections: {}",
                    e
                );
            }
            if let Err(e) = tx.execute(
                "DELETE FROM http_sessions WHERE id = ?1",
                params![session_id],
            ) {
                eprintln!(
                    "[db] delete_http_session: failed to delete ephemeral http_sessions: {}",
                    e
                );
            }
            let _ = tx.commit();
        }
        drop(eph);
        Self::release_payload_refs(payload_refs, payload_store);

        // Reclaim disk space
        let _ = self.vacuum();
        Ok(())
    }

    pub fn vacuum(&self) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("VACUUM", [])?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(())
    }

    /// Ensures at least one active session exists on startup and backfills legacy data
    pub fn ensure_default_http_session(conn: &Connection) -> SqlResult<String> {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM http_sessions", [], |r| r.get(0))?;

        let active_id: Option<String> = conn
            .query_row(
                "SELECT id FROM http_sessions WHERE is_active = 1 LIMIT 1",
                [],
                |r| r.get(0),
            )
            .ok();

        let session_id = match (count, active_id) {
            (0, _) => {
                let id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();
                let name = format!("Session - {}", chrono::Local::now().format("%d %b %H:%M"));
                conn.execute(
                    "INSERT INTO http_sessions (id, name, created_at, updated_at, is_active, description, storage_mode) VALUES (?1, ?2, ?3, ?4, 1, 'Default session', 'persistent')",
                    params![id, name, now, now],
                )?;
                id
            }
            (_, Some(id)) => id,
            (_, None) => {
                let latest_id: String = conn.query_row(
                    "SELECT id FROM http_sessions ORDER BY created_at DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?;
                conn.execute(
                    "UPDATE http_sessions SET is_active = 1 WHERE id = ?1",
                    params![latest_id],
                )?;
                latest_id
            }
        };

        // Backfill legacy logs that have empty session_id
        conn.execute(
            "UPDATE http_logs SET session_id = ?1 WHERE session_id IS NULL OR session_id = ''",
            params![session_id],
        )?;
        conn.execute(
            "UPDATE websocket_connections SET session_id = ?1 WHERE session_id IS NULL OR session_id = ''",
            params![session_id],
        )?;

        Ok(session_id)
    }
}

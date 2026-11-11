use halo_core::types::{
    WebSocketConnectionRecord, WebSocketConnectionState, WebSocketFilter,
    WebSocketMessageDirection, WebSocketMessageRecord, WebSocketMessageType,
};
use rusqlite::{Result as SqlResult, params};
use uuid::Uuid;

use super::Database;
use super::types::PaginatedResponse;

pub const SELECT_WS_CONNECTION_COLS: &str = "id, session_id, timestamp, url, host, path, handshake_request_headers, handshake_response_status, handshake_response_headers, client_addr, server_addr, state, message_count, last_activity_at";

impl Database {
    pub fn insert_websocket_connection(&self, record: &WebSocketConnectionRecord) -> SqlResult<()> {
        let session_id = if !record.session_id.is_empty() {
            record.session_id.clone()
        } else {
            let conn = self.conn.lock();
            Database::get_active_session_id(&conn).unwrap_or_default()
        };

        let storage_mode = self.get_session_storage_mode(Some(&session_id));
        let request_headers =
            serde_json::to_string(&record.handshake_request_headers).unwrap_or_default();
        let response_headers =
            serde_json::to_string(&record.handshake_response_headers).unwrap_or_default();

        let conn = self.traffic_conn(&storage_mode).lock();
        conn.execute(
            r#"INSERT INTO websocket_connections (
                id, session_id, timestamp, url, host, path,
                handshake_request_headers, handshake_response_status, handshake_response_headers,
                client_addr, server_addr, state, message_count, last_activity_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
            params![
                record.id.to_string(),
                session_id,
                record.timestamp.to_rfc3339(),
                record.url,
                record.host,
                record.path,
                request_headers,
                record.handshake_response_status.map(|status| status as i64),
                response_headers,
                record.client_addr,
                record.server_addr,
                websocket_connection_state_to_str(&record.state),
                record.message_count as i64,
                record.last_activity_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_websocket_message(&self, record: &WebSocketMessageRecord) -> SqlResult<()> {
        // Find if connection is in ephemeral or persistent DB
        let conn_id = record.connection_id.to_string();
        let is_in_eph = {
            let eph = self.ephemeral_conn.lock();
            eph.query_row(
                "SELECT 1 FROM websocket_connections WHERE id = ?1",
                params![conn_id],
                |_| Ok(true),
            )
            .unwrap_or(false)
        };

        let conn = if is_in_eph {
            self.ephemeral_conn.lock()
        } else {
            self.conn.lock()
        };

        conn.execute(
            r#"INSERT INTO websocket_messages (
                id, connection_id, timestamp, direction, message_type, payload, payload_size
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            params![
                record.id.to_string(),
                conn_id,
                record.timestamp.to_rfc3339(),
                websocket_message_direction_to_str(&record.direction),
                websocket_message_type_to_str(&record.message_type),
                record.payload,
                record.payload_size as i64,
            ],
        )?;

        conn.execute(
            r#"UPDATE websocket_connections
               SET message_count = message_count + 1, last_activity_at = ?2
               WHERE id = ?1"#,
            params![conn_id, record.timestamp.to_rfc3339()],
        )?;

        let (high_watermark, baseline) = if is_in_eph {
            (150i64, 100i64)
        } else {
            (600i64, 500i64)
        };

        let msg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM websocket_messages WHERE connection_id = ?1",
                params![conn_id],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if msg_count >= high_watermark {
            if let Err(e) = conn.execute(
                "DELETE FROM websocket_messages \
                 WHERE connection_id = ?1 AND id IN ( \
                     SELECT id FROM websocket_messages \
                     WHERE connection_id = ?1 \
                     ORDER BY timestamp DESC, id DESC \
                     LIMIT -1 OFFSET ?2 \
                 )",
                params![conn_id, baseline],
            ) {
                eprintln!(
                    "[db] insert_websocket_message: failed to prune messages: {}",
                    e
                );
            } else if !is_in_eph {
                let _ = conn.execute_batch("PRAGMA incremental_vacuum(100);");
            }
        }
        Ok(())
    }

    pub fn clear_websocket_logs(&self) -> SqlResult<()> {
        {
            let conn = self.conn.lock();
            conn.execute("DELETE FROM websocket_messages", [])?;
            conn.execute("DELETE FROM websocket_connections", [])?;
            let _ = conn.execute_batch("PRAGMA incremental_vacuum(500);");
        }
        {
            let eph = self.ephemeral_conn.lock();
            let _ = eph.execute("DELETE FROM websocket_messages", []);
            let _ = eph.execute("DELETE FROM websocket_connections", []);
        }
        Ok(())
    }

    pub fn delete_websocket_connection(&self, id: &str) -> SqlResult<()> {
        {
            let conn = self.conn.lock();
            conn.execute(
                "DELETE FROM websocket_messages WHERE connection_id = ?1",
                params![id],
            )?;
            conn.execute(
                "DELETE FROM websocket_connections WHERE id = ?1",
                params![id],
            )?;
            let _ = conn.execute_batch("PRAGMA incremental_vacuum(200);");
        }
        {
            let eph = self.ephemeral_conn.lock();
            let _ = eph.execute(
                "DELETE FROM websocket_messages WHERE connection_id = ?1",
                params![id],
            );
            let _ = eph.execute(
                "DELETE FROM websocket_connections WHERE id = ?1",
                params![id],
            );
        }
        Ok(())
    }

    pub fn get_websocket_paginated(
        &self,
        filter: Option<&WebSocketFilter>,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedResponse<WebSocketConnectionRecord>, String> {
        let storage_mode =
            self.get_session_storage_mode(filter.and_then(|f| f.session_id.as_deref()));
        let conn = self.traffic_conn(&storage_mode).lock();
        let offset = (page - 1) * per_page;

        let mut sql = format!(
            "SELECT {} FROM websocket_connections WHERE 1=1",
            SELECT_WS_CONNECTION_COLS
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(filter) = filter {
            append_websocket_filter_sql(filter, &mut sql, &mut params_vec);
        }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let per_page_i64 = per_page as i64;
        let offset_i64 = offset as i64;
        let mut all_params: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|value| value.as_ref()).collect();
        all_params.push(&per_page_i64 as &dyn rusqlite::ToSql);
        all_params.push(&offset_i64 as &dyn rusqlite::ToSql);

        let rows = stmt
            .query_map(all_params.as_slice(), row_to_websocket_connection_record)
            .map_err(|e| e.to_string())?;

        let records: Vec<WebSocketConnectionRecord> = rows.filter_map(Result::ok).collect();

        let mut count_sql = String::from("SELECT COUNT(*) FROM websocket_connections WHERE 1=1");
        let mut count_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(filter) = filter {
            append_websocket_filter_sql(filter, &mut count_sql, &mut count_params);
        }

        let count_params_slice: Vec<&dyn rusqlite::ToSql> =
            count_params.iter().map(|v| v.as_ref()).collect();

        let total: i64 = conn
            .query_row(&count_sql, count_params_slice.as_slice(), |row| row.get(0))
            .map_err(|e| e.to_string())?;

        let has_more = (offset as usize + records.len()) < total as usize;

        Ok(PaginatedResponse {
            data: records,
            total: total as usize,
            page,
            per_page,
            has_more,
        })
    }

    pub fn get_websocket_by_id(
        &self,
        id: &str,
    ) -> Result<Option<WebSocketConnectionRecord>, String> {
        let sql = format!(
            "SELECT {} FROM websocket_connections WHERE id = ?1 LIMIT 1",
            SELECT_WS_CONNECTION_COLS
        );
        // Try persistent DB first
        {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                return row_to_websocket_connection_record(row)
                    .map(Some)
                    .map_err(|e| e.to_string());
            }
        }

        // Try ephemeral DB
        {
            let eph = self.ephemeral_conn.lock();
            let mut stmt = eph.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;
            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                return row_to_websocket_connection_record(row)
                    .map(Some)
                    .map_err(|e| e.to_string());
            }
        }

        Ok(None)
    }

    pub fn get_websocket_messages_by_connection_id(
        &self,
        connection_id: &str,
    ) -> Result<Vec<WebSocketMessageRecord>, String> {
        let sql = "SELECT id, connection_id, timestamp, direction, message_type, payload, payload_size FROM websocket_messages WHERE connection_id = ?1 ORDER BY timestamp ASC";
        // Try persistent DB first
        {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![connection_id], row_to_websocket_message_record)
                .map_err(|e| e.to_string())?;
            let res: Vec<WebSocketMessageRecord> = rows.filter_map(Result::ok).collect();
            if !res.is_empty() {
                return Ok(res);
            }
        }

        // Try ephemeral DB
        {
            let eph = self.ephemeral_conn.lock();
            let mut stmt = eph.prepare(sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![connection_id], row_to_websocket_message_record)
                .map_err(|e| e.to_string())?;
            Ok(rows.filter_map(Result::ok).collect())
        }
    }
}

// ── Row mappers ──────────────────────────────────────────────

fn row_to_websocket_connection_record(row: &rusqlite::Row) -> SqlResult<WebSocketConnectionRecord> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let timestamp: String = row.get(2)?;
    let url: String = row.get(3)?;
    let host: String = row.get(4)?;
    let path: String = row.get(5)?;
    let handshake_request_headers: Option<String> = row.get(6)?;
    let handshake_response_status: Option<i64> = row.get(7)?;
    let handshake_response_headers: Option<String> = row.get(8)?;
    let client_addr: Option<String> = row.get(9)?;
    let server_addr: Option<String> = row.get(10)?;
    let state: String = row.get(11)?;
    let message_count: i64 = row.get(12)?;
    let last_activity_at: String = row.get(13)?;

    Ok(WebSocketConnectionRecord {
        id: Uuid::parse_str(&id).map_err(|_| rusqlite::Error::InvalidQuery)?,
        session_id,
        timestamp: chrono::DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .with_timezone(&chrono::Utc),
        url,
        host,
        path,
        handshake_request_headers: handshake_request_headers
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default(),
        handshake_response_status: handshake_response_status.map(|status| status as u16),
        handshake_response_headers: handshake_response_headers
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default(),
        client_addr: client_addr.unwrap_or_default(),
        server_addr: server_addr.unwrap_or_default(),
        state: websocket_connection_state_from_str(&state),
        message_count: message_count as u32,
        last_activity_at: chrono::DateTime::parse_from_rfc3339(&last_activity_at)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .with_timezone(&chrono::Utc),
    })
}

fn row_to_websocket_message_record(row: &rusqlite::Row) -> SqlResult<WebSocketMessageRecord> {
    let id: String = row.get(0)?;
    let connection_id: String = row.get(1)?;
    let timestamp: String = row.get(2)?;
    let direction: String = row.get(3)?;
    let message_type: String = row.get(4)?;
    let payload: Vec<u8> = row.get(5)?;
    let payload_size: i64 = row.get(6)?;

    Ok(WebSocketMessageRecord {
        id: Uuid::parse_str(&id).map_err(|_| rusqlite::Error::InvalidQuery)?,
        connection_id: Uuid::parse_str(&connection_id)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        timestamp: chrono::DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .with_timezone(&chrono::Utc),
        direction: websocket_message_direction_from_str(&direction),
        message_type: websocket_message_type_from_str(&message_type),
        payload,
        payload_size: payload_size as usize,
    })
}

fn websocket_connection_state_to_str(state: &WebSocketConnectionState) -> &'static str {
    match state {
        WebSocketConnectionState::Open => "open",
        WebSocketConnectionState::Closed => "closed",
        WebSocketConnectionState::Error => "error",
    }
}

fn websocket_connection_state_from_str(value: &str) -> WebSocketConnectionState {
    match value.to_lowercase().as_str() {
        "open" => WebSocketConnectionState::Open,
        "error" => WebSocketConnectionState::Error,
        _ => WebSocketConnectionState::Closed,
    }
}

fn websocket_message_direction_to_str(direction: &WebSocketMessageDirection) -> &'static str {
    match direction {
        WebSocketMessageDirection::Inbound => "inbound",
        WebSocketMessageDirection::Outbound => "outbound",
    }
}

fn websocket_message_direction_from_str(value: &str) -> WebSocketMessageDirection {
    match value.to_lowercase().as_str() {
        "inbound" => WebSocketMessageDirection::Inbound,
        _ => WebSocketMessageDirection::Outbound,
    }
}

fn websocket_message_type_to_str(message_type: &WebSocketMessageType) -> &'static str {
    match message_type {
        WebSocketMessageType::Text => "text",
        WebSocketMessageType::Binary => "binary",
        WebSocketMessageType::Ping => "ping",
        WebSocketMessageType::Pong => "pong",
        WebSocketMessageType::Close => "close",
    }
}

fn websocket_message_type_from_str(value: &str) -> WebSocketMessageType {
    match value {
        "binary" => WebSocketMessageType::Binary,
        "ping" => WebSocketMessageType::Ping,
        "pong" => WebSocketMessageType::Pong,
        "close" => WebSocketMessageType::Close,
        _ => WebSocketMessageType::Text,
    }
}

// ── Filter SQL builders ─────────────────────────────────────

fn append_websocket_filter_sql(
    filter: &WebSocketFilter,
    sql: &mut String,
    params_vec: &mut Vec<Box<dyn rusqlite::ToSql>>,
) {
    if let Some(ref session_id) = filter.session_id {
        let trimmed = session_id.trim();
        if !trimmed.is_empty() {
            sql.push_str(" AND session_id = ?");
            params_vec.push(Box::new(trimmed.to_string()));
        }
    }

    if let Some(ref search) = filter.search {
        if !search.is_empty() {
            let search_pattern = format!("%{}%", search);
            sql.push_str(" AND (url LIKE ? OR host LIKE ? OR path LIKE ?)");
            params_vec.push(Box::new(search_pattern.clone()));
            params_vec.push(Box::new(search_pattern.clone()));
            params_vec.push(Box::new(search_pattern));
        }
    }

    if let Some(ref states) = filter.states {
        if !states.is_empty() {
            sql.push_str(" AND state IN (");
            for (index, state) in states.iter().enumerate() {
                if index > 0 {
                    sql.push_str(", ");
                }
                sql.push('?');
                params_vec.push(Box::new(state.clone()));
            }
            sql.push(')');
        }
    }

    if let Some(ref scope) = filter.scope {
        let scoped: Vec<String> = scope
            .iter()
            .map(|pattern| pattern.trim().to_string())
            .filter(|pattern| !pattern.is_empty())
            .collect();

        if !scoped.is_empty() {
            sql.push_str(" AND (");
            for (index, pattern) in scoped.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" OR ");
                }
                if let Some(domain) = pattern.strip_prefix("*.") {
                    sql.push_str("(host = ? OR host LIKE ?)");
                    params_vec.push(Box::new(domain.to_string()));
                    params_vec.push(Box::new(format!("%.{}", domain)));
                } else {
                    sql.push_str("host LIKE ?");
                    params_vec.push(Box::new(format!("%{}%", pattern)));
                }
            }
            sql.push(')');
        }
    }
}

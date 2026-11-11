use chrono::Utc;
use rusqlite::params;

use crate::db::payload_store::PayloadStore;
use crate::db::repository::Database;

/// Promotes an ephemeral session to persistent mode:
/// 1. Copies all metadata rows (http_logs, websocket_*) from in-memory SQLite to disk SQLite.
/// 2. Flushes RAM slab bodies into disk segment files and rewrites payload references.
/// 3. Updates session storage_mode to 'persistent' in disk SQLite.
/// 4. Cleans up ephemeral rows and slabs for this session.
pub fn promote_session(
    database: &Database,
    payload_store: &PayloadStore,
    session_id: &str,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();

    // 1. Fetch ephemeral http_logs rows
    struct EphemeralLogRow {
        id: String,
        session_id: String,
        timestamp: String,
        method: String,
        url: String,
        request_headers: Option<String>,
        request_body: Option<Vec<u8>>,
        req_payload_ref: String,
        req_body_size: usize,
        req_truncated: bool,
        response_status: Option<u16>,
        response_status_text: Option<String>,
        response_headers: Option<String>,
        response_body: Option<Vec<u8>>,
        res_payload_ref: String,
        res_body_size: usize,
        res_truncated: bool,
        client_addr: Option<String>,
        server_addr: Option<String>,
        duration_ms: Option<i64>,
    }

    let ephemeral_logs: Vec<EphemeralLogRow> = {
        let eph_conn = database.ephemeral_conn().lock();
        let mut stmt = eph_conn
            .prepare(
                r#"SELECT id, session_id, timestamp, method, url,
                          request_headers, request_body, req_payload_ref, req_body_size, req_truncated,
                          response_status, response_status_text, response_headers, response_body, res_payload_ref, res_body_size, res_truncated,
                          client_addr, server_addr, duration_ms
                   FROM http_logs
                   WHERE session_id = ?1"#,
            )
            .map_err(|e| format!("Failed to prepare ephemeral logs query: {e}"))?;

        let rows = stmt
            .query_map(params![session_id], |row| {
                let req_trunc_int: i32 = row.get(9).unwrap_or(0);
                let res_trunc_int: i32 = row.get(16).unwrap_or(0);
                Ok(EphemeralLogRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    method: row.get(3)?,
                    url: row.get(4)?,
                    request_headers: row.get(5)?,
                    request_body: row.get(6)?,
                    req_payload_ref: row.get(7).unwrap_or_default(),
                    req_body_size: row.get::<_, Option<i64>>(8)?.unwrap_or(0) as usize,
                    req_truncated: req_trunc_int != 0,
                    response_status: row.get(10)?,
                    response_status_text: row.get(11)?,
                    response_headers: row.get(12)?,
                    response_body: row.get(13)?,
                    res_payload_ref: row.get(14).unwrap_or_default(),
                    res_body_size: row.get::<_, Option<i64>>(15)?.unwrap_or(0) as usize,
                    res_truncated: res_trunc_int != 0,
                    client_addr: row.get(17)?,
                    server_addr: row.get(18)?,
                    duration_ms: row.get(19)?,
                })
            })
            .map_err(|e| format!("Failed to query ephemeral logs: {e}"))?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| format!("Row read error: {e}"))?);
        }
        list
    };

    // 2. Fetch ephemeral websocket connections & messages
    struct EphemeralWsConn {
        id: String,
        session_id: String,
        timestamp: String,
        url: String,
        host: String,
        path: String,
        handshake_request_headers: Option<String>,
        handshake_response_status: Option<u16>,
        handshake_response_headers: Option<String>,
        client_addr: Option<String>,
        server_addr: Option<String>,
        state: String,
        message_count: usize,
        last_activity_at: String,
    }

    struct EphemeralWsMsg {
        id: String,
        connection_id: String,
        timestamp: String,
        direction: String,
        message_type: String,
        payload: Option<Vec<u8>>,
        payload_size: usize,
    }

    let (ephemeral_ws_conns, ephemeral_ws_msgs) = {
        let eph_conn = database.ephemeral_conn().lock();
        let mut stmt_conns = eph_conn
            .prepare(
                r#"SELECT id, session_id, timestamp, url, host, path,
                          handshake_request_headers, handshake_response_status, handshake_response_headers,
                          client_addr, server_addr, state, message_count, last_activity_at
                   FROM websocket_connections
                   WHERE session_id = ?1"#,
            )
            .map_err(|e| format!("Failed to prepare ephemeral ws conns: {e}"))?;

        let conns = stmt_conns
            .query_map(params![session_id], |row| {
                Ok(EphemeralWsConn {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    url: row.get(3)?,
                    host: row.get(4)?,
                    path: row.get(5)?,
                    handshake_request_headers: row.get(6)?,
                    handshake_response_status: row.get(7)?,
                    handshake_response_headers: row.get(8)?,
                    client_addr: row.get(9)?,
                    server_addr: row.get(10)?,
                    state: row.get(11)?,
                    message_count: row.get::<_, Option<i64>>(12)?.unwrap_or(0) as usize,
                    last_activity_at: row.get(13)?,
                })
            })
            .map_err(|e| format!("Failed to query ws conns: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("WS Conn read error: {e}"))?;

        let mut stmt_msgs = eph_conn
            .prepare(
                r#"SELECT m.id, m.connection_id, m.timestamp, m.direction, m.message_type, m.payload, m.payload_size
                   FROM websocket_messages m
                   INNER JOIN websocket_connections c ON m.connection_id = c.id
                   WHERE c.session_id = ?1"#,
            )
            .map_err(|e| format!("Failed to prepare ephemeral ws msgs: {e}"))?;

        let msgs = stmt_msgs
            .query_map(params![session_id], |row| {
                Ok(EphemeralWsMsg {
                    id: row.get(0)?,
                    connection_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    direction: row.get(3)?,
                    message_type: row.get(4)?,
                    payload: row.get(5)?,
                    payload_size: row.get::<_, Option<i64>>(6)?.unwrap_or(0) as usize,
                })
            })
            .map_err(|e| format!("Failed to query ws msgs: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("WS Msg read error: {e}"))?;

        (conns, msgs)
    };

    // 3. Write all records into disk SQLite with payloads flushed to disk segments
    let mut slabs_to_remove: Vec<String> = Vec::new();

    {
        let mut disk_conn = database.disk_conn().lock();
        let tx = disk_conn
            .transaction()
            .map_err(|e| format!("Failed to begin disk transaction: {e}"))?;

        for log in ephemeral_logs {
            let mut new_req_ref = log.req_payload_ref.clone();
            let mut new_res_ref = log.res_payload_ref.clone();

            // Flush request body slab to disk segment
            if log.req_payload_ref.starts_with("slab:") {
                if let Ok(Some(body)) = payload_store.load_body(&log.req_payload_ref) {
                    let (stored_ref, _, _) =
                        payload_store.store_body(session_id, &body, "persistent");
                    new_req_ref = stored_ref;
                    slabs_to_remove.push(log.req_payload_ref);
                }
            }

            // Flush response body slab to disk segment
            if log.res_payload_ref.starts_with("slab:") {
                if let Ok(Some(body)) = payload_store.load_body(&log.res_payload_ref) {
                    let (stored_ref, _, _) =
                        payload_store.store_body(session_id, &body, "persistent");
                    new_res_ref = stored_ref;
                    slabs_to_remove.push(log.res_payload_ref);
                }
            }

            tx.execute(
                r#"INSERT OR REPLACE INTO http_logs (
                    id, session_id, timestamp, method, url,
                    request_headers, request_body, req_payload_ref, req_body_size, req_truncated,
                    response_status, response_status_text, response_headers, response_body, res_payload_ref, res_body_size, res_truncated,
                    client_addr, server_addr, duration_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)"#,
                params![
                    log.id,
                    log.session_id,
                    log.timestamp,
                    log.method,
                    log.url,
                    log.request_headers,
                    log.request_body,
                    new_req_ref,
                    log.req_body_size as i64,
                    if log.req_truncated { 1 } else { 0 },
                    log.response_status,
                    log.response_status_text,
                    log.response_headers,
                    log.response_body,
                    new_res_ref,
                    log.res_body_size as i64,
                    if log.res_truncated { 1 } else { 0 },
                    log.client_addr,
                    log.server_addr,
                    log.duration_ms,
                ],
            )
            .map_err(|e| format!("Failed to insert promoted http log {}: {e}", log.id))?;
        }

        // Insert WS connections & messages
        for conn in ephemeral_ws_conns {
            tx.execute(
                r#"INSERT OR REPLACE INTO websocket_connections (
                    id, session_id, timestamp, url, host, path,
                    handshake_request_headers, handshake_response_status, handshake_response_headers,
                    client_addr, server_addr, state, message_count, last_activity_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
                params![
                    conn.id,
                    conn.session_id,
                    conn.timestamp,
                    conn.url,
                    conn.host,
                    conn.path,
                    conn.handshake_request_headers,
                    conn.handshake_response_status,
                    conn.handshake_response_headers,
                    conn.client_addr,
                    conn.server_addr,
                    conn.state,
                    conn.message_count as i64,
                    conn.last_activity_at,
                ],
            )
            .map_err(|e| format!("Failed to insert promoted ws conn {}: {e}", conn.id))?;
        }

        for msg in ephemeral_ws_msgs {
            tx.execute(
                r#"INSERT OR REPLACE INTO websocket_messages (
                    id, connection_id, timestamp, direction, message_type, payload, payload_size
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
                params![
                    msg.id,
                    msg.connection_id,
                    msg.timestamp,
                    msg.direction,
                    msg.message_type,
                    msg.payload,
                    msg.payload_size as i64,
                ],
            )
            .map_err(|e| format!("Failed to insert promoted ws msg {}: {e}", msg.id))?;
        }

        // 4. Update session storage_mode in disk DB
        tx.execute(
            "UPDATE http_sessions SET storage_mode = 'persistent', updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )
        .map_err(|e| format!("Failed to update session storage mode: {e}"))?;

        tx.commit()
            .map_err(|e| format!("Failed to commit promotion transaction: {e}"))?;
    }

    // 5. Clean up ephemeral DB
    {
        let eph_conn = database.ephemeral_conn().lock();
        let _ = eph_conn.execute(
            "DELETE FROM http_logs WHERE session_id = ?1",
            params![session_id],
        );
        let _ = eph_conn.execute(
            "DELETE FROM websocket_messages WHERE connection_id IN (SELECT id FROM websocket_connections WHERE session_id = ?1)",
            params![session_id],
        );
        let _ = eph_conn.execute(
            "DELETE FROM websocket_connections WHERE session_id = ?1",
            params![session_id],
        );
    }

    // 6. Remove freed slabs from RAM
    for slab_ref in slabs_to_remove {
        let _ = payload_store.remove_body(&slab_ref);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo_core::types::{ProxyRecord, ProxyRequest, ProxyResponse};
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn test_promote_ephemeral_to_persistent() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test.db");
        let sessions_dir = temp.path().join("sessions");

        let database = Database::new(db_path).unwrap();
        database.init().unwrap();
        let payload_store = PayloadStore::new(sessions_dir);

        // 1. Create an ephemeral session
        let session = database
            .create_http_session(
                "Test Ephemeral Session",
                None,
                Some("all"),
                None,
                None,
                Some("ephemeral"),
            )
            .unwrap();
        assert_eq!(session.storage_mode, "ephemeral");

        // 2. Insert logs into ephemeral session
        let record = ProxyRecord {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            client_addr: "127.0.0.1:12345".to_string(),
            server_addr: "93.184.216.34:80".to_string(),
            request: ProxyRequest {
                method: "POST".to_string(),
                uri: "http://example.com/api/test".to_string(),
                http_version: "HTTP/1.1".to_string(),
                headers: HashMap::new(),
                body: b"RequestBodyData".to_vec(),
                content_decoded: false,
            },
            response: Some(ProxyResponse {
                status_code: 200,
                status_text: "OK".to_string(),
                http_version: "HTTP/1.1".to_string(),
                headers: HashMap::new(),
                body: b"ResponseBodyData123".to_vec(),
                content_decoded: false,
            }),
        };

        database
            .insert_log(&record, Some(&session.id), Some(&payload_store))
            .unwrap();

        // Verify log exists in ephemeral DB, but not in disk DB
        {
            let eph = database.ephemeral_conn().lock();
            let count: i64 = eph
                .query_row(
                    "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
                    params![session.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1);

            let disk = database.disk_conn().lock();
            let disk_count: i64 = disk
                .query_row(
                    "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
                    params![session.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(disk_count, 0);
        }

        // 3. Promote session to persistent
        promote_session(&database, &payload_store, &session.id).unwrap();

        // 4. Verify log moved to disk DB and deleted from ephemeral DB
        {
            let eph = database.ephemeral_conn().lock();
            let count: i64 = eph
                .query_row(
                    "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
                    params![session.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);

            let disk = database.disk_conn().lock();
            let disk_count: i64 = disk
                .query_row(
                    "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
                    params![session.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(disk_count, 1);

            let mode: String = disk
                .query_row(
                    "SELECT storage_mode FROM http_sessions WHERE id = ?1",
                    params![session.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(mode, "persistent");
        }

        // 5. Verify payload can still be read through get_by_id from disk segment
        let loaded = database
            .get_by_id(&record.id.to_string(), Some(&payload_store))
            .unwrap()
            .expect("Log should exist");

        assert_eq!(loaded.request.body, b"RequestBodyData");
        assert_eq!(loaded.response.unwrap().body, b"ResponseBodyData123");
    }

    #[test]
    fn test_ephemeral_100_row_sliding_window() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("test_window.db");
        let sessions_dir = temp.path().join("sessions");

        let database = Database::new(db_path).unwrap();
        database.init().unwrap();
        let payload_store = PayloadStore::new(sessions_dir);

        let session = database
            .create_http_session(
                "Rolling Window Session",
                None,
                Some("all"),
                None,
                None,
                Some("ephemeral"),
            )
            .unwrap();

        // 1. Insert 149 logs sequentially (should not trigger pruning yet)
        let mut created_ids = Vec::new();

        for i in 0..149 {
            let record = ProxyRecord {
                id: Uuid::new_v4(),
                timestamp: Utc::now() + chrono::Duration::milliseconds(i),
                client_addr: "127.0.0.1:12345".to_string(),
                server_addr: "93.184.216.34:80".to_string(),
                request: ProxyRequest {
                    method: "GET".to_string(),
                    uri: format!("http://example.com/api/item/{}", i),
                    http_version: "HTTP/1.1".to_string(),
                    headers: HashMap::new(),
                    body: format!("RequestBody-{}", i).into_bytes(),
                    content_decoded: false,
                },
                response: Some(ProxyResponse {
                    status_code: 200,
                    status_text: "OK".to_string(),
                    http_version: "HTTP/1.1".to_string(),
                    headers: HashMap::new(),
                    body: format!("ResponseBody-{}", i).into_bytes(),
                    content_decoded: false,
                }),
            };

            created_ids.push(record.id.to_string());
            database
                .insert_log(&record, Some(&session.id), Some(&payload_store))
                .unwrap();
        }

        // Count should be exactly 149 (no pruning before 150)
        {
            let eph = database.ephemeral_conn().lock();
            let count: i64 = eph
                .query_row(
                    "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
                    params![session.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 149);
        }

        // 2. Insert the 150th log -> triggers pruning back to 100!
        {
            let record = ProxyRecord {
                id: Uuid::new_v4(),
                timestamp: Utc::now() + chrono::Duration::milliseconds(149),
                client_addr: "127.0.0.1:12345".to_string(),
                server_addr: "93.184.216.34:80".to_string(),
                request: ProxyRequest {
                    method: "GET".to_string(),
                    uri: "http://example.com/api/item/149".to_string(),
                    http_version: "HTTP/1.1".to_string(),
                    headers: HashMap::new(),
                    body: b"RequestBody-149".to_vec(),
                    content_decoded: false,
                },
                response: Some(ProxyResponse {
                    status_code: 200,
                    status_text: "OK".to_string(),
                    http_version: "HTTP/1.1".to_string(),
                    headers: HashMap::new(),
                    body: b"ResponseBody-149".to_vec(),
                    content_decoded: false,
                }),
            };
            created_ids.push(record.id.to_string());
            database
                .insert_log(&record, Some(&session.id), Some(&payload_store))
                .unwrap();
        }

        // Verify count trimmed from 150 down to baseline 100
        {
            let eph = database.ephemeral_conn().lock();
            let count: i64 = eph
                .query_row(
                    "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
                    params![session.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 100);

            // Verify oldest 50 were pruned
            for id in &created_ids[..50] {
                let exists: bool = eph
                    .query_row("SELECT 1 FROM http_logs WHERE id = ?1", params![id], |_| {
                        Ok(true)
                    })
                    .unwrap_or(false);
                assert!(!exists, "Oldest row {} should have been pruned", id);
            }

            // Verify newest 100 exist in SQLite
            for id in &created_ids[50..] {
                let exists: bool = eph
                    .query_row("SELECT 1 FROM http_logs WHERE id = ?1", params![id], |_| {
                        Ok(true)
                    })
                    .unwrap_or(false);
                assert!(exists, "New row {} should be present", id);
            }
        }

        // Verify newest records can still load payloads
        let newest_id = &created_ids[149];
        let newest_log = database
            .get_by_id(newest_id, Some(&payload_store))
            .unwrap()
            .expect("Newest log must exist");
        assert_eq!(newest_log.request.body, b"RequestBody-149");
        assert_eq!(newest_log.response.unwrap().body, b"ResponseBody-149");
    }
}

//! Insert and prune proxy HTTP logs.

use super::*;

impl Database {
    pub fn insert_log(
        &self,
        record: &ProxyRecord,
        session_id: Option<&str>,
        payload_store: Option<&PayloadStore>,
    ) -> SqlResult<()> {
        let storage_mode = self.get_session_storage_mode(session_id);
        let target_session_id = if let Some(sid) = session_id {
            sid.to_string()
        } else {
            let conn = self.conn.lock();
            Self::ensure_default_http_session(&conn)?
        };

        // Store body in payload store if provided
        let (req_ref, req_size, req_trunc) = if let Some(ps) = payload_store {
            ps.store_body(&target_session_id, &record.request.body, &storage_mode)
        } else {
            (String::new(), record.request.body.len(), false)
        };

        let (res_ref, res_size, res_trunc) = if let Some(ps) = payload_store {
            if let Some(ref resp) = record.response {
                ps.store_body(&target_session_id, &resp.body, &storage_mode)
            } else {
                (String::new(), 0, false)
            }
        } else {
            (
                String::new(),
                record.response.as_ref().map(|r| r.body.len()).unwrap_or(0),
                false,
            )
        };

        let request_headers = serde_json::to_string(&record.request.headers).unwrap_or_default();
        let response_headers = record
            .response
            .as_ref()
            .map(|r| serde_json::to_string(&r.headers).unwrap_or_default())
            .unwrap_or_default();

        let conn = self.traffic_conn(&storage_mode).lock();
        conn.execute(
            r#"INSERT INTO http_logs (
                id, session_id, timestamp, method, url,
                request_headers, request_body, req_payload_ref, req_body_size, req_truncated,
                response_status, response_status_text,
                response_headers, response_body, res_payload_ref, res_body_size, res_truncated,
                client_addr, server_addr, duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, ?11, ?12, NULL, ?13, ?14, ?15, ?16, ?17, ?18)"#,
            params![
                record.id.to_string(),
                target_session_id,
                record.timestamp.to_rfc3339(),
                record.request.method,
                record.request.uri,
                request_headers,
                req_ref,
                req_size as i64,
                if req_trunc { 1 } else { 0 },
                record.response.as_ref().map(|r| r.status_code as i64),
                record.response.as_ref().map(|r| r.status_text.clone()),
                response_headers,
                res_ref,
                res_size as i64,
                if res_trunc { 1 } else { 0 },
                record.client_addr,
                record.server_addr,
                0i64,
            ],
        )?;

        if storage_mode == "ephemeral" {
            if let Err(e) =
                Self::prune_ephemeral_http_logs(&conn, payload_store, &target_session_id)
            {
                eprintln!("[db] insert_log: failed to prune ephemeral logs: {}", e);
            }
        } else if let Err(e) =
            Self::prune_persistent_http_logs(&conn, payload_store, &target_session_id)
        {
            eprintln!("[db] insert_log: failed to prune persistent logs: {}", e);
        }
        Ok(())
    }

    pub const EPHEMERAL_BASELINE_ROWS: usize = 100;
    pub const EPHEMERAL_HIGH_WATERMARK_ROWS: usize = 150;
    pub const PERSISTENT_BASELINE_ROWS: usize = 2_000;
    pub const PERSISTENT_HIGH_WATERMARK_ROWS: usize = 2_500;

    pub fn prune_persistent_http_logs(
        conn: &rusqlite::Connection,
        payload_store: Option<&PayloadStore>,
        session_id: &str,
    ) -> SqlResult<()> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )?;

        if count >= Self::PERSISTENT_HIGH_WATERMARK_ROWS as i64 {
            conn.execute(
                "DELETE FROM http_logs \
                 WHERE session_id = ?1 AND id IN ( \
                     SELECT id FROM http_logs \
                     WHERE session_id = ?1 \
                     ORDER BY timestamp DESC, id DESC \
                     LIMIT -1 OFFSET ?2 \
                 )",
                params![session_id, Self::PERSISTENT_BASELINE_ROWS as i64],
            )?;
            let _ = conn.execute_batch("PRAGMA incremental_vacuum(200);");
            Self::prune_unreferenced_session_segments(conn, payload_store, session_id);
        } else if let Some(store) = payload_store {
            let _ = store.enforce_persistent_budget();
        }

        Ok(())
    }

    pub(crate) fn prune_unreferenced_session_segments(
        conn: &rusqlite::Connection,
        payload_store: Option<&PayloadStore>,
        session_id: &str,
    ) {
        let Some(store) = payload_store else {
            return;
        };
        let mut min_seg_id: Option<u64> = None;
        if let Ok(mut stmt) = conn.prepare_cached(
            "SELECT req_payload_ref, res_payload_ref FROM http_logs WHERE session_id = ?1",
        ) {
            if let Ok(mut rows) = stmt.query(params![session_id]) {
                while let Ok(Some(row)) = rows.next() {
                    for col in [0, 1] {
                        if let Ok(Some(r)) = row.get::<_, Option<String>>(col) {
                            if let Some(seg_id) = parse_seg_id_from_ref(&r) {
                                min_seg_id = Some(match min_seg_id {
                                    Some(cur) => cur.min(seg_id),
                                    None => seg_id,
                                });
                            }
                        }
                    }
                }
            }
        }
        if let Some(min_id) = min_seg_id {
            store.prune_session_segments_before(session_id, min_id);
        } else {
            store.prune_session_segments_before(session_id, u64::MAX);
        }
    }

    pub fn prune_ephemeral_http_logs(
        conn: &rusqlite::Connection,
        payload_store: Option<&PayloadStore>,
        session_id: &str,
    ) -> SqlResult<()> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM http_logs WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )?;

        // Only prune when reaching or exceeding high watermark (150)
        if count < Self::EPHEMERAL_HIGH_WATERMARK_ROWS as i64 {
            return Ok(());
        }

        // Collect slab refs to remove for rows beyond the baseline 100
        let mut stmt = conn.prepare_cached(
            "SELECT req_payload_ref, res_payload_ref FROM http_logs \
             WHERE session_id = ?1 \
             ORDER BY timestamp DESC, id DESC \
             LIMIT -1 OFFSET ?2",
        )?;

        let mut refs_to_remove = Vec::new();
        let mut rows = stmt.query(params![session_id, Self::EPHEMERAL_BASELINE_ROWS as i64])?;
        while let Some(row) = rows.next()? {
            let req_ref: Option<String> = row.get(0)?;
            let res_ref: Option<String> = row.get(1)?;
            if let Some(r) = req_ref {
                if !r.is_empty() {
                    refs_to_remove.push(r);
                }
            }
            if let Some(r) = res_ref {
                if !r.is_empty() {
                    refs_to_remove.push(r);
                }
            }
        }
        drop(rows);
        drop(stmt);

        // Bulk single-statement DELETE trimming back to baseline 100
        conn.execute(
            "DELETE FROM http_logs \
             WHERE session_id = ?1 AND id IN ( \
                 SELECT id FROM http_logs \
                 WHERE session_id = ?1 \
                 ORDER BY timestamp DESC, id DESC \
                 LIMIT -1 OFFSET ?2 \
             )",
            params![session_id, Self::EPHEMERAL_BASELINE_ROWS as i64],
        )?;

        if let Some(store) = payload_store {
            for r in refs_to_remove {
                let _ = store.remove_body(&r);
            }
        }

        Ok(())
    }

    pub fn insert_logs_batch(
        &self,
        records: &[(ProxyRecord, Option<String>)],
        payload_store: Option<&PayloadStore>,
    ) -> SqlResult<()> {
        if records.is_empty() {
            return Ok(());
        }

        // Group records by (target_session_id, storage_mode)
        let mut persistent_records = Vec::new();
        let mut ephemeral_records = Vec::new();

        {
            let conn = self.conn.lock();
            let default_session = Self::ensure_default_http_session(&conn)?;
            drop(conn);

            // Cache the per-session storage mode so a batch of 50 records
            // doesn't re-lock the disk DB up to 50 times for the same lookup.
            let mut mode_cache: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();

            for (record, session_id) in records {
                let sid = match session_id.as_deref() {
                    Some(s) if !s.trim().is_empty() => s.trim().to_string(),
                    _ => default_session.clone(),
                };
                let mode = match mode_cache.get(&sid) {
                    Some(m) => m.clone(),
                    None => {
                        let m = self.get_session_storage_mode(Some(&sid));
                        mode_cache.insert(sid.clone(), m.clone());
                        m
                    }
                };
                if mode == "ephemeral" {
                    ephemeral_records.push((record, sid));
                } else {
                    persistent_records.push((record, sid));
                }
            }
        }

        // Write persistent batch
        if !persistent_records.is_empty() {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare_cached(
                    r#"INSERT INTO http_logs (
                        id, session_id, timestamp, method, url,
                        request_headers, request_body, req_payload_ref, req_body_size, req_truncated,
                        response_status, response_status_text,
                        response_headers, response_body, res_payload_ref, res_body_size, res_truncated,
                        client_addr, server_addr, duration_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, ?11, ?12, NULL, ?13, ?14, ?15, ?16, ?17, ?18)"#,
                )?;
                for (record, sid) in &persistent_records {
                    let (req_ref, req_size, req_trunc) = if let Some(ps) = payload_store {
                        ps.store_body(sid, &record.request.body, "persistent")
                    } else {
                        (String::new(), record.request.body.len(), false)
                    };

                    let (res_ref, res_size, res_trunc) = if let Some(ps) = payload_store {
                        if let Some(ref resp) = record.response {
                            ps.store_body(sid, &resp.body, "persistent")
                        } else {
                            (String::new(), 0, false)
                        }
                    } else {
                        (
                            String::new(),
                            record.response.as_ref().map(|r| r.body.len()).unwrap_or(0),
                            false,
                        )
                    };

                    let request_headers =
                        serde_json::to_string(&record.request.headers).unwrap_or_default();
                    let response_headers = record
                        .response
                        .as_ref()
                        .map(|r| serde_json::to_string(&r.headers).unwrap_or_default())
                        .unwrap_or_default();

                    stmt.execute(params![
                        record.id.to_string(),
                        sid,
                        record.timestamp.to_rfc3339(),
                        record.request.method,
                        record.request.uri,
                        request_headers,
                        req_ref,
                        req_size as i64,
                        if req_trunc { 1 } else { 0 },
                        record.response.as_ref().map(|r| r.status_code as i64),
                        record.response.as_ref().map(|r| r.status_text.clone()),
                        response_headers,
                        res_ref,
                        res_size as i64,
                        if res_trunc { 1 } else { 0 },
                        record.client_addr,
                        record.server_addr,
                        0i64,
                    ])?;
                }
            }
            tx.commit()?;

            let mut session_ids: Vec<String> = persistent_records
                .iter()
                .map(|(_, sid)| sid.clone())
                .collect();
            session_ids.sort();
            session_ids.dedup();
            for sid in session_ids {
                let _ = Self::prune_persistent_http_logs(&conn, payload_store, &sid);
            }
        }

        // Write ephemeral batch
        if !ephemeral_records.is_empty() {
            let mut conn = self.ephemeral_conn.lock();
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare_cached(
                    r#"INSERT INTO http_logs (
                        id, session_id, timestamp, method, url,
                        request_headers, request_body, req_payload_ref, req_body_size, req_truncated,
                        response_status, response_status_text,
                        response_headers, response_body, res_payload_ref, res_body_size, res_truncated,
                        client_addr, server_addr, duration_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, ?11, ?12, NULL, ?13, ?14, ?15, ?16, ?17, ?18)"#,
                )?;
                for (record, sid) in &ephemeral_records {
                    let (req_ref, req_size, req_trunc) = if let Some(ps) = payload_store {
                        ps.store_body(sid, &record.request.body, "ephemeral")
                    } else {
                        (String::new(), record.request.body.len(), false)
                    };

                    let (res_ref, res_size, res_trunc) = if let Some(ps) = payload_store {
                        if let Some(ref resp) = record.response {
                            ps.store_body(sid, &resp.body, "ephemeral")
                        } else {
                            (String::new(), 0, false)
                        }
                    } else {
                        (
                            String::new(),
                            record.response.as_ref().map(|r| r.body.len()).unwrap_or(0),
                            false,
                        )
                    };

                    let request_headers =
                        serde_json::to_string(&record.request.headers).unwrap_or_default();
                    let response_headers = record
                        .response
                        .as_ref()
                        .map(|r| serde_json::to_string(&r.headers).unwrap_or_default())
                        .unwrap_or_default();

                    stmt.execute(params![
                        record.id.to_string(),
                        sid,
                        record.timestamp.to_rfc3339(),
                        record.request.method,
                        record.request.uri,
                        request_headers,
                        req_ref,
                        req_size as i64,
                        if req_trunc { 1 } else { 0 },
                        record.response.as_ref().map(|r| r.status_code as i64),
                        record.response.as_ref().map(|r| r.status_text.clone()),
                        response_headers,
                        res_ref,
                        res_size as i64,
                        if res_trunc { 1 } else { 0 },
                        record.client_addr,
                        record.server_addr,
                        0i64,
                    ])?;
                }
            }
            tx.commit()?;

            // Prune ephemeral sessions after batch write
            let mut session_ids: Vec<String> = ephemeral_records
                .iter()
                .map(|(_, sid)| sid.clone())
                .collect();
            session_ids.sort();
            session_ids.dedup();
            for sid in session_ids {
                let _ = Self::prune_ephemeral_http_logs(&conn, payload_store, &sid);
            }
        }

        Ok(())
    }
}

//! Proxy log queries: listing, filtering, delete, and clear operations.

use super::*;

use super::mappers::*;
impl Database {
    pub fn get_all(&self, payload_store: Option<&PayloadStore>) -> SqlResult<Vec<ProxyRecord>> {
        let mut results = Vec::new();
        {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare(&format!(
                "SELECT {} FROM http_logs ORDER BY timestamp DESC",
                SELECT_PROXY_RECORD_COLS
            ))?;
            let rows = stmt.query_map([], |row| row_to_proxy_record(row, payload_store))?;
            results.extend(collect_records(rows));
        }
        {
            let eph = self.ephemeral_conn.lock();
            let mut stmt = eph.prepare(&format!(
                "SELECT {} FROM http_logs ORDER BY timestamp DESC",
                SELECT_PROXY_RECORD_COLS
            ))?;
            let rows = stmt.query_map([], |row| row_to_proxy_record(row, payload_store))?;
            results.extend(collect_records(rows));
        }
        // Newest first. `Reverse` keeps this a key sort without cloning the timestamps.
        results.sort_by_key(|record| Reverse(record.timestamp));
        Ok(results)
    }

    pub fn get_filtered(
        &self,
        filter: &ProxyFilter,
        payload_store: Option<&PayloadStore>,
    ) -> SqlResult<Vec<ProxyRecord>> {
        let storage_mode = self.get_session_storage_mode(filter.session_id.as_deref());
        let conn = self.traffic_conn(&storage_mode).lock();
        let mut sql = format!(
            "SELECT {} FROM http_logs WHERE 1=1",
            SELECT_PROXY_RECORD_COLS
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref session_id) = filter.session_id {
            if !session_id.is_empty() {
                sql.push_str(" AND session_id = ?");
                params_vec.push(Box::new(session_id.clone()));
            }
        }

        if let Some(ref search) = filter.search {
            if !search.is_empty() {
                let search_pattern = format!("%{}%", search);
                sql.push_str(
                    " AND (url LIKE ? OR method LIKE ? OR server_addr LIKE ? OR request_headers LIKE ?)",
                );
                params_vec.push(Box::new(search_pattern.clone()));
                params_vec.push(Box::new(search_pattern.clone()));
                params_vec.push(Box::new(search_pattern.clone()));
                params_vec.push(Box::new(search_pattern));
            }
        }

        if let Some(ref path) = filter.path {
            if !path.is_empty() {
                sql.push_str(" AND url LIKE ?");
                params_vec.push(Box::new(format!("%{}%", path)));
            }
        }

        if let Some(ref methods) = filter.methods {
            if !methods.is_empty() {
                sql.push_str(" AND method IN (");
                for (i, m) in methods.iter().enumerate() {
                    if i > 0 {
                        sql.push_str(", ");
                    }
                    sql.push('?');
                    params_vec.push(Box::new(m.clone()));
                }
                sql.push(')');
            }
        }

        if let Some(ref status_codes) = filter.status_codes {
            if !status_codes.is_empty() {
                sql.push_str(" AND response_status IN (");
                for (i, s) in status_codes.iter().enumerate() {
                    if i > 0 {
                        sql.push_str(", ");
                    }
                    sql.push('?');
                    params_vec.push(Box::new(*s as i64));
                }
                sql.push(')');
            }
        }

        sql.push_str(" ORDER BY timestamp DESC");

        let all_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(all_params.as_slice(), |row| {
            row_to_proxy_record(row, payload_store)
        })?;

        Ok(collect_records(rows))
    }

    pub fn delete_log(&self, id: &str, payload_store: Option<&PayloadStore>) -> SqlResult<()> {
        // Collect payload refs first so the stored bodies can be reclaimed.
        let mut payload_refs: Vec<String> = Vec::new();
        {
            let conn = self.conn.lock();
            if let Ok(mut stmt) =
                conn.prepare("SELECT req_payload_ref, res_payload_ref FROM http_logs WHERE id = ?1")
            {
                if let Ok(rows) = stmt.query_map(params![id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                }) {
                    for pair in rows.flatten() {
                        payload_refs.extend([pair.0, pair.1]);
                    }
                }
            }
            if let Err(e) = conn.execute("DELETE FROM http_logs WHERE id = ?1", params![id]) {
                eprintln!("[db] delete_log: failed to delete from disk DB: {}", e);
            }
        }
        {
            let eph = self.ephemeral_conn.lock();
            if let Ok(mut stmt) =
                eph.prepare("SELECT req_payload_ref, res_payload_ref FROM http_logs WHERE id = ?1")
            {
                if let Ok(rows) = stmt.query_map(params![id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                }) {
                    for pair in rows.flatten() {
                        payload_refs.extend([pair.0, pair.1]);
                    }
                }
            }
            if let Err(e) = eph.execute("DELETE FROM http_logs WHERE id = ?1", params![id]) {
                eprintln!("[db] delete_log: failed to delete from ephemeral DB: {}", e);
            }
        }
        Self::release_payload_refs(payload_refs, payload_store);
        Ok(())
    }

    /// Releases payload bodies (e.g. RAM slab entries) referenced by deleted
    /// log rows. Disk segment refs are append-only and ignored.
    pub(crate) fn release_payload_refs(refs: Vec<String>, payload_store: Option<&PayloadStore>) {
        if let Some(store) = payload_store {
            for r in refs {
                if !r.is_empty() && r.starts_with("slab:") {
                    if let Err(e) = store.remove_body(&r) {
                        eprintln!("[db] failed to release payload {}: {}", r, e);
                    }
                }
            }
        }
    }

    pub fn get_by_id(
        &self,
        id: &str,
        payload_store: Option<&PayloadStore>,
    ) -> SqlResult<Option<ProxyRecord>> {
        // Try persistent DB first
        {
            let conn = self.conn.lock();
            let mut stmt = conn.prepare(&format!(
                "SELECT {} FROM http_logs WHERE id = ?1 LIMIT 1",
                SELECT_PROXY_RECORD_COLS
            ))?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                return row_to_proxy_record(row, payload_store).map(Some);
            }
        }

        // Try ephemeral DB
        {
            let eph = self.ephemeral_conn.lock();
            let mut stmt = eph.prepare(&format!(
                "SELECT {} FROM http_logs WHERE id = ?1 LIMIT 1",
                SELECT_PROXY_RECORD_COLS
            ))?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                return row_to_proxy_record(row, payload_store).map(Some);
            }
        }

        Ok(None)
    }

    pub fn clear_logs(&self) -> SqlResult<()> {
        {
            let conn = self.conn.lock();
            conn.execute("DELETE FROM http_logs", [])?;
            drop(conn);
            let _ = self.vacuum();
        }
        {
            let eph = self.ephemeral_conn.lock();
            let _ = eph.execute("DELETE FROM http_logs", []);
        }
        Ok(())
    }

    pub fn clear_logs_before(&self, cutoff_rfc3339: &str) -> SqlResult<usize> {
        self.clear_logs_before_with_store(cutoff_rfc3339, None)
    }

    pub fn clear_logs_before_with_store(
        &self,
        cutoff_rfc3339: &str,
        payload_store: Option<&PayloadStore>,
    ) -> SqlResult<usize> {
        let mut total = 0;
        {
            let mut conn = self.conn.lock();
            let tx = conn.transaction()?;
            let rows = tx.execute(
                "DELETE FROM http_logs WHERE timestamp < ?1",
                params![cutoff_rfc3339],
            )?;
            if let Err(e) = tx.execute(
                "DELETE FROM websocket_messages WHERE timestamp < ?1",
                params![cutoff_rfc3339],
            ) {
                eprintln!(
                    "[db] clear_logs_before: failed to delete websocket_messages: {}",
                    e
                );
            }
            if let Err(e) = tx.execute(
                "DELETE FROM websocket_connections WHERE timestamp < ?1",
                params![cutoff_rfc3339],
            ) {
                eprintln!(
                    "[db] clear_logs_before: failed to delete websocket_connections: {}",
                    e
                );
            }
            tx.commit()?;
            let mut session_ids: Vec<String> = Vec::new();
            if let Ok(mut stmt) = conn.prepare("SELECT id FROM http_sessions") {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                    session_ids.extend(rows.flatten());
                }
            }
            for sid in &session_ids {
                Self::prune_unreferenced_session_segments(&conn, payload_store, sid);
            }
            drop(conn);
            let _ = self.vacuum();
            total += rows;
        }
        {
            let mut eph = self.ephemeral_conn.lock();
            let mut payload_refs: Vec<String> = Vec::new();
            if let Ok(mut stmt) = eph.prepare(
                "SELECT req_payload_ref, res_payload_ref FROM http_logs WHERE timestamp < ?1",
            ) {
                if let Ok(rows) = stmt.query_map(params![cutoff_rfc3339], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                }) {
                    for pair in rows.flatten() {
                        payload_refs.extend([pair.0, pair.1]);
                    }
                }
            }
            match eph.transaction() {
                Ok(tx) => {
                    let rows = tx
                        .execute(
                            "DELETE FROM http_logs WHERE timestamp < ?1",
                            params![cutoff_rfc3339],
                        )
                        .unwrap_or(0);
                    if let Err(e) = tx.execute(
                        "DELETE FROM websocket_messages WHERE timestamp < ?1",
                        params![cutoff_rfc3339],
                    ) {
                        eprintln!(
                            "[db] clear_logs_before: failed to delete ephemeral websocket_messages: {}",
                            e
                        );
                    }
                    if let Err(e) = tx.execute(
                        "DELETE FROM websocket_connections WHERE timestamp < ?1",
                        params![cutoff_rfc3339],
                    ) {
                        eprintln!(
                            "[db] clear_logs_before: failed to delete ephemeral websocket_connections: {}",
                            e
                        );
                    }
                    let _ = tx.commit();
                    total += rows;
                }
                Err(e) => {
                    eprintln!(
                        "[db] clear_logs_before: failed to begin ephemeral transaction: {}",
                        e
                    );
                }
            }
            drop(eph);
            Self::release_payload_refs(payload_refs, payload_store);
        }
        Ok(total)
    }
}

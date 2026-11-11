use super::Database;
use super::types::{
    ChronicleLogRecord, CollectionRecord, ContextRecord, StashEndpointRecord, StashRecord,
};
use rusqlite::{Result as SqlResult, params};

impl Database {
    // --- Collections ---
    pub fn get_collections(&self) -> SqlResult<Vec<CollectionRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, sort_order, created_at, updated_at FROM collections ORDER BY sort_order ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CollectionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_collection(&self, record: &CollectionRecord) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO collections (id, name, sort_order, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   sort_order = excluded.sort_order,
                   updated_at = excluded.updated_at"#,
            params![
                record.id,
                record.name,
                record.sort_order,
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_collection(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        // Endpoints cascade from stashes; stashes cascade from collection_id FK
        // when present. Explicitly delete stashes under this collection first so
        // pre-FK rows (null collection_id after soft assignment) still clean up.
        let stash_ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM stashes WHERE collection_id = ?1")?;
            let rows = stmt.query_map(params![id], |row| row.get(0))?;
            rows.collect::<SqlResult<Vec<_>>>()?
        };
        for stash_id in &stash_ids {
            conn.execute(
                "DELETE FROM stash_endpoints WHERE stash_id = ?1",
                params![stash_id],
            )?;
            conn.execute("DELETE FROM stashes WHERE id = ?1", params![stash_id])?;
        }
        conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Stashes (folders) ---
    pub fn get_stashes(&self) -> SqlResult<Vec<StashRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, collection_id, sort_order, created_at, updated_at FROM stashes ORDER BY sort_order ASC, name ASC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(StashRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                collection_id: row.get(3)?,
                sort_order: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_stash(&self, record: &StashRecord) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO stashes (id, name, parent_id, collection_id, sort_order, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   parent_id = excluded.parent_id,
                   collection_id = excluded.collection_id,
                   sort_order = excluded.sort_order,
                   updated_at = excluded.updated_at"#,
            params![
                record.id,
                record.name,
                record.parent_id,
                record.collection_id,
                record.sort_order,
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_stash(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM stashes WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Stash Endpoints ---
    pub fn get_stash_endpoints(&self) -> SqlResult<Vec<StashEndpointRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, stash_id, name, method, url, headers, body, body_type, pre_script, test_script, sort_order, created_at, updated_at 
             FROM stash_endpoints ORDER BY sort_order ASC, created_at ASC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(StashEndpointRecord {
                id: row.get(0)?,
                stash_id: row.get(1)?,
                name: row.get(2)?,
                method: row.get(3)?,
                url: row.get(4)?,
                headers: row.get(5)?,
                body: row.get(6)?,
                body_type: row.get(7)?,
                pre_script: row.get(8)?,
                test_script: row.get(9)?,
                sort_order: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_stash_endpoint(&self, record: &StashEndpointRecord) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO stash_endpoints (id, stash_id, name, method, url, headers, body, body_type, pre_script, test_script, sort_order, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
               ON CONFLICT(id) DO UPDATE SET
                   stash_id = excluded.stash_id,
                   name = excluded.name,
                   method = excluded.method,
                   url = excluded.url,
                   headers = excluded.headers,
                   body = excluded.body,
                   body_type = excluded.body_type,
                   pre_script = excluded.pre_script,
                   test_script = excluded.test_script,
                   sort_order = excluded.sort_order,
                   updated_at = excluded.updated_at"#,
            params![
                record.id, record.stash_id, record.name, record.method, record.url,
                record.headers, record.body, record.body_type, record.pre_script,
                record.test_script, record.sort_order, record.created_at, record.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_stash_endpoint(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM stash_endpoints WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Contexts (environments) ---
    pub fn get_contexts(&self) -> SqlResult<Vec<ContextRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, variables, created_at, updated_at FROM contexts ORDER BY name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ContextRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                variables: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_context(&self, record: &ContextRecord) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO contexts (id, name, variables, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   variables = excluded.variables,
                   updated_at = excluded.updated_at"#,
            params![
                record.id,
                record.name,
                record.variables,
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_context(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM contexts WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Chronicle (request history) ---
    pub fn get_chronicle_logs(&self, limit: u32) -> SqlResult<Vec<ChronicleLogRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, method, url, request_headers, request_body, response_status, response_status_text, response_headers, response_body, duration_ms
             FROM chronicle_logs ORDER BY timestamp DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(ChronicleLogRecord {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                method: row.get(2)?,
                url: row.get(3)?,
                request_headers: row.get(4)?,
                request_body: row.get(5)?,
                response_status: row.get(6)?,
                response_status_text: row.get(7)?,
                response_headers: row.get(8)?,
                response_body: row.get(9)?,
                duration_ms: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    pub fn add_chronicle_log(&self, record: &ChronicleLogRecord) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO chronicle_logs (id, timestamp, method, url, request_headers, request_body, response_status, response_status_text, response_headers, response_body, duration_ms)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            params![
                record.id, record.timestamp, record.method, record.url,
                record.request_headers, record.request_body, record.response_status,
                record.response_status_text, record.response_headers, record.response_body,
                record.duration_ms
            ],
        )?;
        // Cap chronicle at 500 entries (drop oldest).
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM chronicle_logs", [], |row| row.get(0))?;
        if count > 500 {
            let to_delete = count - 500;
            conn.execute(
                "DELETE FROM chronicle_logs WHERE id IN (
                     SELECT id FROM chronicle_logs ORDER BY timestamp ASC LIMIT ?1
                 )",
                params![to_delete],
            )?;
        }
        Ok(())
    }

    pub fn clear_chronicle_logs(&self) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM chronicle_logs", [])?;
        Ok(())
    }
}

pub mod api_stash;
pub mod collaborator;
pub mod crawler;
pub mod documents;
pub mod halo_agent;
pub mod http_sessions;
pub mod mock_forge;
pub mod notes;
pub mod proxy_logs;
pub mod regression;
pub mod types;
pub mod websocket;

pub use http_sessions::*;
pub use types::*;

use parking_lot::Mutex;
use rusqlite::{Connection, Result as SqlResult};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    ephemeral_conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Database {
    pub fn new(path: PathBuf) -> SqlResult<Self> {
        let conn = Connection::open(&path)?;
        let ephemeral_conn = Connection::open_in_memory()?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            ephemeral_conn: Arc::new(Mutex::new(ephemeral_conn)),
            path,
        })
    }

    pub fn disk_conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    pub fn ephemeral_conn(&self) -> &Arc<Mutex<Connection>> {
        &self.ephemeral_conn
    }

    pub fn traffic_conn(&self, storage_mode: &str) -> &Arc<Mutex<Connection>> {
        if storage_mode == "ephemeral" {
            &self.ephemeral_conn
        } else {
            &self.conn
        }
    }

    pub fn reset_ephemeral(&self) -> SqlResult<()> {
        let eph = self.ephemeral_conn.lock();
        eph.execute("DELETE FROM websocket_messages", [])?;
        eph.execute("DELETE FROM websocket_connections", [])?;
        eph.execute("DELETE FROM http_logs", [])?;
        Ok(())
    }

    // ponytail: allow closing the sqlite connection and reopening it to reset the database file safely
    pub fn close_connection(&self) -> SqlResult<()> {
        let mut conn = self.conn.lock();
        let dummy_conn = Connection::open_in_memory()?;
        let old_conn = std::mem::replace(&mut *conn, dummy_conn);
        drop(old_conn);
        Ok(())
    }

    pub fn reopen_and_init(&self) -> SqlResult<()> {
        let mut conn = self.conn.lock();
        let new_conn = Connection::open(&self.path)?;
        *conn = new_conn;
        drop(conn);
        self.init()?;
        Ok(())
    }

    pub fn init(&self) -> SqlResult<()> {
        {
            let conn = self.conn.lock();
            Self::apply_disk_pragmas(&conn)?;
            Self::create_all_tables(&conn)?;
            Self::init_traffic_schema(&conn)?;

            Self::ensure_column(
                &conn,
                "http_sessions",
                "capture_mode",
                "TEXT NOT NULL DEFAULT 'all'",
            )?;
            Self::ensure_column(
                &conn,
                "http_sessions",
                "capture_filter",
                "TEXT NOT NULL DEFAULT '[]'",
            )?;
            Self::ensure_column(
                &conn,
                "http_sessions",
                "exclude_filter",
                "TEXT NOT NULL DEFAULT '[]'",
            )?;
            Self::ensure_column(
                &conn,
                "http_sessions",
                "storage_mode",
                "TEXT NOT NULL DEFAULT 'persistent'",
            )?;

            let _ = Self::ensure_default_http_session(&conn)?;

            Self::ensure_column(&conn, "crawler_pages", "ai_used_for_analysis", "INTEGER")?;
            Self::ensure_column(&conn, "crawler_pages", "screenshot_path", "TEXT")?;
            Self::ensure_column(&conn, "crawler_pages", "rendered_html_path", "TEXT")?;
            Self::ensure_column(&conn, "crawler_insights", "ai_used_for_analysis", "INTEGER")?;
            Self::ensure_column(&conn, "crawler_insights", "analysis_source", "TEXT")?;
            Self::ensure_column(&conn, "crawler_insights", "analysis_tool_id", "TEXT")?;
            Self::ensure_column(&conn, "crawler_insights", "analysis_tool_name", "TEXT")?;
            Self::ensure_column(&conn, "crawler_logs", "ai_used_for_analysis", "INTEGER")?;
            Self::ensure_column(&conn, "crawler_logs", "extra_json", "TEXT")?;
            Self::ensure_column(
                &conn,
                "documents",
                "custom_sections",
                "TEXT NOT NULL DEFAULT '[]'",
            )?;
            Self::ensure_column(
                &conn,
                "documents",
                "removed_built_in_sections",
                "TEXT NOT NULL DEFAULT '[]'",
            )?;
            Self::ensure_column(&conn, "stashes", "sort_order", "INTEGER NOT NULL DEFAULT 0")?;
            Self::ensure_column(&conn, "stashes", "collection_id", "TEXT")?;
            Self::ensure_column(
                &conn,
                "stash_endpoints",
                "sort_order",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            Self::ensure_column(
                &conn,
                "mock_routes",
                "matcher_enabled",
                "INTEGER NOT NULL DEFAULT 1",
            )?;
            Self::ensure_column(
                &conn,
                "halo_agent_sessions",
                "approval_mode",
                "TEXT NOT NULL DEFAULT 'ask'",
            )?;
            Self::ensure_column(
                &conn,
                "halo_agent_sessions",
                "subagents_json",
                "TEXT NOT NULL DEFAULT '[]'",
            )?;
            Self::ensure_column(
                &conn,
                "halo_agent_sessions",
                "skills_json",
                "TEXT NOT NULL DEFAULT '[]'",
            )?;
            Self::ensure_column(
                &conn,
                "halo_agent_sessions",
                "mcp_servers_json",
                "TEXT NOT NULL DEFAULT '[]'",
            )?;
            Self::ensure_column(
                &conn,
                "halo_agent_messages",
                "usage_json",
                "TEXT NOT NULL DEFAULT ''",
            )?;
            let _ = conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_stashes_collection ON stashes(collection_id)",
                [],
            );
        }

        // Initialize in-memory ephemeral database
        {
            let eph = self.ephemeral_conn.lock();
            eph.execute_batch("PRAGMA foreign_keys = OFF;")?;
            eph.execute_batch("PRAGMA journal_mode = OFF;")?;
            eph.execute_batch("PRAGMA synchronous = OFF;")?;
            eph.execute_batch("PRAGMA temp_store = MEMORY;")?;
            Self::init_traffic_schema(&eph)?;
        }
        Ok(())
    }

    fn apply_disk_pragmas(conn: &Connection) -> SqlResult<()> {
        let auto_vacuum: i64 = conn
            .query_row("PRAGMA auto_vacuum;", [], |r| r.get(0))
            .unwrap_or(0);
        if auto_vacuum == 0 {
            let _ = conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;");
        }
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA journal_size_limit = 33554432;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;
             PRAGMA mmap_size = 67108864;
             PRAGMA cache_size = -16384;
             PRAGMA temp_store = MEMORY;
             PRAGMA wal_autocheckpoint = 1000;
             PRAGMA incremental_vacuum(500);",
        )?;
        Ok(())
    }

    fn create_all_tables(conn: &Connection) -> SqlResult<()> {
        for (old_table, new_table) in [
            ("ai_browser_sessions", "crawler_sessions"),
            ("ai_browser_pages", "crawler_pages"),
            ("ai_browser_edges", "crawler_edges"),
            ("ai_browser_insights", "crawler_insights"),
            ("ai_browser_logs", "crawler_logs"),
            ("coder_agent_sessions", "halo_agent_sessions"),
            ("coder_agent_messages", "halo_agent_messages"),
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1",
                    [old_table],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            let new_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1",
                    [new_table],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if exists && !new_exists {
                let _ = conn.execute(
                    &format!("ALTER TABLE {old_table} RENAME TO {new_table}"),
                    [],
                );
            }
        }

        conn.execute_batch(crate::db::schema::CREATE_DOCUMENTS_TABLE)?;
        conn.execute_batch(crate::db::schema::CREATE_CRAWLER_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_COLLABORATOR_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_REGRESSION_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_COLLECTIONS_TABLES)?;
        // Legacy `stashes` tables predate `collection_id`, but the batch below
        // creates an index on that column. SQLite aborts the whole batch there,
        // so migrate the column first when the table already exists.
        let stashes_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='stashes'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if stashes_exists {
            Self::ensure_column(conn, "stashes", "collection_id", "TEXT")?;
        }
        conn.execute_batch(crate::db::schema::CREATE_STASHES_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_CONTEXTS_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_CHRONICLE_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_MOCK_FORGE_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_NOTES_TABLES)?;
        conn.execute_batch(crate::db::schema::CREATE_HALO_AGENT_TABLES)?;
        Ok(())
    }

    /// Schema shared by the disk and ephemeral connections: traffic tables,
    /// their runtime-added columns, and the query indexes.
    fn init_traffic_schema(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(crate::db::schema::CREATE_HTTP_SESSIONS_TABLE)?;
        conn.execute_batch(crate::db::schema::CREATE_HTTP_LOGS_TABLE)?;
        conn.execute_batch(crate::db::schema::CREATE_WEBSOCKET_TABLES)?;

        for (table_name, column_name, column_type) in [
            ("http_logs", "session_id", "TEXT NOT NULL DEFAULT ''"),
            ("http_logs", "req_payload_ref", "TEXT DEFAULT ''"),
            ("http_logs", "req_body_size", "INTEGER DEFAULT 0"),
            ("http_logs", "req_truncated", "INTEGER DEFAULT 0"),
            ("http_logs", "res_payload_ref", "TEXT DEFAULT ''"),
            ("http_logs", "res_body_size", "INTEGER DEFAULT 0"),
            ("http_logs", "res_truncated", "INTEGER DEFAULT 0"),
            (
                "websocket_connections",
                "session_id",
                "TEXT NOT NULL DEFAULT ''",
            ),
        ] {
            Self::ensure_column(conn, table_name, column_name, column_type)?;
        }

        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_http_logs_session_ts ON http_logs(session_id, timestamp DESC)", []);
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_http_logs_host_status_ts ON http_logs(server_addr, response_status, timestamp DESC)", []);
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_websocket_connections_session ON websocket_connections(session_id)", []);
        Ok(())
    }

    fn ensure_column(
        conn: &Connection,
        table_name: &str,
        column_name: &str,
        column_type: &str,
    ) -> SqlResult<()> {
        // SQLite cannot bind identifiers as parameters, so these are formatted
        // into the statement. Every caller passes a literal today; assert that
        // stays true so a future caller cannot smuggle SQL through a name.
        if !is_plain_identifier(table_name) {
            return Err(rusqlite::Error::InvalidParameterName(
                table_name.to_string(),
            ));
        }
        if !is_plain_identifier(column_name) {
            return Err(rusqlite::Error::InvalidParameterName(
                column_name.to_string(),
            ));
        }
        if !is_safe_type_declaration(column_type) {
            return Err(rusqlite::Error::InvalidParameterName(
                column_type.to_string(),
            ));
        }

        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let exists = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<SqlResult<Vec<_>>>()?
            .iter()
            .any(|name| name == column_name);

        if !exists {
            conn.execute(
                &format!(
                    "ALTER TABLE {} ADD COLUMN {} {}",
                    table_name, column_name, column_type
                ),
                [],
            )?;
        }

        Ok(())
    }
}

/// `[A-Za-z0-9_]{1,64}` — safe to interpolate into DDL as an identifier.
fn is_plain_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A column type declaration like `TEXT NOT NULL DEFAULT '[]'`. Allows the
/// characters SQLite needs for a type and a simple literal default, nothing
/// that could terminate the statement or start a new one.
fn is_safe_type_declaration(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '\'' | '(' | ')' | ',' | '[' | ']')
        })
}

use super::Database;
use halo_core::types::{ChaosConfig, MockDomain, MockRoute};
use rusqlite::{Result as SqlResult, params};

impl Database {
    pub fn get_mock_domains(&self) -> SqlResult<Vec<MockDomain>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, hostname, ssl, status, created_at FROM mock_domains ORDER BY created_at ASC"
        )?;
        let rows = stmt.query_map([], |row| {
            let ssl_int: i32 = row.get(2)?;
            Ok(MockDomain {
                id: row.get(0)?,
                hostname: row.get(1)?,
                ssl: ssl_int != 0,
                status: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn insert_mock_domain(&self, domain: &MockDomain) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO mock_domains (id, hostname, ssl, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                hostname = excluded.hostname,
                ssl = excluded.ssl,
                status = excluded.status,
                created_at = excluded.created_at",
            params![
                domain.id,
                domain.hostname,
                if domain.ssl { 1 } else { 0 },
                domain.status,
                domain.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_mock_domain(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM mock_domains WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn toggle_mock_domain(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE mock_domains
             SET status = CASE WHEN status = 'active' THEN 'inactive' ELSE 'active' END
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn get_mock_routes(&self) -> SqlResult<Vec<MockRoute>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, domain_id, method, path, status_code, response_body, response_headers, matchers, chaos, enabled, matcher_enabled, request_query_params, request_body FROM mock_routes"
        )?;
        let rows = stmt.query_map([], |row| {
            let headers_str: String = row.get(6)?;
            let matchers_str: String = row.get(7)?;
            let chaos_str: String = row.get(8)?;
            let enabled_int: i32 = row.get(9)?;
            let matcher_enabled_int: i32 = row.get(10)?;
            let query_params_str: Option<String> = row.get(11)?;

            let response_headers = serde_json::from_str(&headers_str).unwrap_or_default();
            let matchers = serde_json::from_str(&matchers_str).unwrap_or_default();
            let chaos = serde_json::from_str(&chaos_str).unwrap_or_else(|_| ChaosConfig {
                latency_mode: "none".to_string(),
                latency_fixed: None,
                latency_min: None,
                latency_max: None,
                error_rate: None,
                error_status: None,
            });
            let request_query_params = query_params_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(MockRoute {
                id: row.get(0)?,
                domain_id: row.get(1)?,
                method: row.get(2)?,
                path: row.get(3)?,
                status_code: row.get::<_, i32>(4)? as u16,
                response_body: row.get(5)?,
                response_headers,
                matchers,
                chaos,
                enabled: enabled_int != 0,
                matcher_enabled: matcher_enabled_int != 0,
                request_query_params,
                request_body: row.get(12)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_mock_route(&self, route: &MockRoute) -> SqlResult<()> {
        let conn = self.conn.lock();
        // Ensure domain_id exists in mock_domains to satisfy foreign key constraint
        let default_hostname = if route.domain_id == "local_mock_server" {
            "localhost"
        } else if route.domain_id == "all-hosts" {
            "*"
        } else {
            &route.domain_id
        };
        conn.execute(
            "INSERT OR IGNORE INTO mock_domains (id, hostname, ssl, status, created_at)
             VALUES (?1, ?2, 0, 'active', datetime('now'))",
            params![route.domain_id, default_hostname],
        )?;

        let headers_str =
            serde_json::to_string(&route.response_headers).unwrap_or_else(|_| "{}".to_string());
        let matchers_str =
            serde_json::to_string(&route.matchers).unwrap_or_else(|_| "[]".to_string());
        let chaos_str = serde_json::to_string(&route.chaos).unwrap_or_else(|_| "{}".to_string());
        let query_params_str = route
            .request_query_params
            .as_ref()
            .and_then(|q| serde_json::to_string(q).ok());

        conn.execute(
            "INSERT INTO mock_routes (id, domain_id, method, path, status_code, response_body, response_headers, matchers, chaos, enabled, matcher_enabled, request_query_params, request_body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                domain_id = excluded.domain_id,
                method = excluded.method,
                path = excluded.path,
                status_code = excluded.status_code,
                response_body = excluded.response_body,
                response_headers = excluded.response_headers,
                matchers = excluded.matchers,
                chaos = excluded.chaos,
                enabled = excluded.enabled,
                matcher_enabled = excluded.matcher_enabled,
                request_query_params = excluded.request_query_params,
                request_body = excluded.request_body",
            params![
                route.id,
                route.domain_id,
                route.method,
                route.path,
                route.status_code as i32,
                route.response_body,
                headers_str,
                matchers_str,
                chaos_str,
                if route.enabled { 1 } else { 0 },
                if route.matcher_enabled { 1 } else { 0 },
                query_params_str,
                route.request_body,
            ],
        )?;
        Ok(())
    }

    pub fn delete_mock_route(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM mock_routes WHERE id = ?1", params![id])?;
        Ok(())
    }
}

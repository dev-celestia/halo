//! Proxy log summaries and the request/response tree view.

use super::*;

use super::mappers::*;
impl Database {
    /// Returns the latest log summaries, skipping request/response BLOBs and headers.
    pub fn get_summary_recent(
        &self,
        session_id: Option<&str>,
        limit: u32,
        sort_order: &str,
    ) -> Result<Vec<ProxySummaryRow>, String> {
        // Unscoped listing always reads the disk DB so history survives restarts.
        // Session-scoped queries follow that session's storage_mode.
        let storage_mode = match session_id {
            Some(sid) if !sid.is_empty() => self.get_session_storage_mode(Some(sid)),
            _ => "persistent".to_string(),
        };
        let conn = self.traffic_conn(&storage_mode).lock();

        let (where_clause, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = match session_id {
            Some(sid) if !sid.is_empty() => (
                " WHERE session_id = ?".to_string(),
                vec![Box::new(sid.to_string())],
            ),
            _ => (String::new(), Vec::new()),
        };

        let sql = format!(
            "SELECT id, session_id, timestamp, method, url, response_status, response_status_text,
                    COALESCE(req_body_size, LENGTH(COALESCE(request_body, X''))),
                    COALESCE(res_body_size, LENGTH(COALESCE(response_body, X''))),
                    COALESCE(server_addr, ''),
                    request_headers,
                    response_headers
             FROM http_logs{}
             ORDER BY timestamp {} LIMIT ?",
            where_clause, sort_order
        );

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let limit_i64 = limit as i64;
        let mut all_params: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();
        all_params.push(&limit_i64 as &dyn rusqlite::ToSql);

        let mut rows = stmt
            .query(all_params.as_slice())
            .map_err(|e| e.to_string())?;
        let mut records: Vec<ProxySummaryRow> = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            records.push(row_to_proxy_summary(row).map_err(|e| e.to_string())?);
        }

        Ok(records)
    }

    /// Returns the latest filtered log summaries, skipping request/response BLOBs.
    pub fn get_filtered_summary_recent(
        &self,
        filter: &ProxyFilter,
        limit: u32,
        sort_order: &str,
    ) -> Result<Vec<ProxySummaryRow>, String> {
        let storage_mode = self.get_session_storage_mode(filter.session_id.as_deref());
        let conn = self.traffic_conn(&storage_mode).lock();

        let mut where_sql = String::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref sid) = filter.session_id {
            if !sid.is_empty() {
                where_sql.push_str(" AND session_id = ?");
                params_vec.push(Box::new(sid.clone()));
            }
        }

        if let Some(ref search) = filter.search {
            if !search.is_empty() {
                let search_pattern = format!("%{}%", search);
                where_sql.push_str(
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
                where_sql.push_str(" AND url LIKE ?");
                params_vec.push(Box::new(format!("%{}%", path)));
            }
        }

        if let Some(ref methods) = filter.methods {
            if !methods.is_empty() {
                where_sql.push_str(" AND method IN (");
                for (i, m) in methods.iter().enumerate() {
                    if i > 0 {
                        where_sql.push_str(", ");
                    }
                    where_sql.push('?');
                    params_vec.push(Box::new(m.clone()));
                }
                where_sql.push(')');
            }
        }

        if let Some(ref status_codes) = filter.status_codes {
            if !status_codes.is_empty() {
                where_sql.push_str(" AND response_status IN (");
                for (i, s) in status_codes.iter().enumerate() {
                    if i > 0 {
                        where_sql.push_str(", ");
                    }
                    where_sql.push('?');
                    params_vec.push(Box::new(*s as i64));
                }
                where_sql.push(')');
            }
        }

        if let Some(ref scope) = filter.scope {
            if let Some(clause) = build_scope_sql_clause(scope, &mut params_vec) {
                where_sql.push_str(&clause);
            }
        }

        let limit_i64 = limit as i64;

        let data_sql = format!(
            "SELECT id, session_id, timestamp, method, url, response_status, response_status_text,
                    COALESCE(req_body_size, LENGTH(COALESCE(request_body, X''))),
                    COALESCE(res_body_size, LENGTH(COALESCE(response_body, X''))),
                    COALESCE(server_addr, ''),
                    request_headers,
                    response_headers
             FROM http_logs WHERE 1=1{}
             ORDER BY timestamp {} LIMIT ?",
            where_sql, sort_order
        );

        let mut stmt = conn.prepare(&data_sql).map_err(|e| e.to_string())?;
        let mut all_params: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();
        all_params.push(&limit_i64 as &dyn rusqlite::ToSql);

        let mut rows = stmt
            .query(all_params.as_slice())
            .map_err(|e| e.to_string())?;
        let mut records: Vec<ProxySummaryRow> = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            records.push(row_to_proxy_summary(row).map_err(|e| e.to_string())?);
        }

        Ok(records)
    }

    pub fn count(&self, session_id: Option<&str>) -> Result<usize, String> {
        let storage_mode = self.get_session_storage_mode(session_id);
        let conn = self.traffic_conn(&storage_mode).lock();
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = match session_id {
            Some(sid) if !sid.is_empty() => (
                "SELECT COUNT(*) FROM http_logs WHERE session_id = ?".to_string(),
                vec![Box::new(sid.to_string())],
            ),
            _ => ("SELECT COUNT(*) FROM http_logs".to_string(), Vec::new()),
        };
        let params_ref: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        let total: i64 = conn
            .query_row(&sql, params_ref.as_slice(), |row| row.get(0))
            .map_err(|e| e.to_string())?;
        Ok(total as usize)
    }

    pub fn get_tree(&self, filter: &ProxyFilter) -> Result<Vec<TreeNode>, String> {
        let storage_mode = self.get_session_storage_mode(filter.session_id.as_deref());
        let conn = self.traffic_conn(&storage_mode).lock();

        let mut sql = String::from("SELECT url, method FROM http_logs WHERE 1=1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref sid) = filter.session_id {
            if !sid.is_empty() {
                sql.push_str(" AND session_id = ?");
                params_vec.push(Box::new(sid.clone()));
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
                where_sql_push_close(&mut sql);
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
                where_sql_push_close(&mut sql);
            }
        }

        if let Some(ref scope) = filter.scope {
            if let Some(clause) = build_scope_sql_clause(scope, &mut params_vec) {
                sql.push_str(&clause);
            }
        }

        let all_params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(all_params.as_slice(), |row| {
                let url: String = row.get(0)?;
                let method: String = row.get(1)?;
                Ok((url, method))
            })
            .map_err(|e| e.to_string())?;

        #[derive(Default)]
        struct PathInfo {
            url: String,
            count: u32,
            methods: std::collections::HashSet<String>,
        }

        let mut host_paths: std::collections::HashMap<
            String,
            std::collections::HashMap<String, PathInfo>,
        > = Default::default();

        for row in rows {
            let (url, method) = row.map_err(|e| e.to_string())?;
            let uri = if url.contains("://") {
                match url.split("://").nth(1) {
                    Some(u) => u,
                    _ => &url,
                }
            } else {
                &url
            };
            let host = uri.split('/').next().unwrap_or("");

            let host_entry = host_paths.entry(host.to_string()).or_default();
            let path_entry = host_entry
                .entry(url.to_string())
                .or_insert_with(|| PathInfo {
                    url,
                    ..Default::default()
                });
            path_entry.count += 1;
            path_entry.methods.insert(method);
        }

        let mut tree: Vec<TreeNode> = Vec::new();
        let mut hosts: Vec<_> = host_paths.into_iter().collect();
        hosts.sort_by(|a, b| a.0.cmp(&b.0));

        for (host, paths_map) in hosts {
            let mut paths_vec: Vec<TreePath> = Vec::new();
            let mut paths: Vec<_> = paths_map.into_iter().collect();
            paths.sort_by_key(|path| Reverse(path.1.count));

            for (_url, info) in paths {
                let mut methods: Vec<String> = info.methods.into_iter().collect();
                methods.sort();
                let uri = if info.url.contains("://") {
                    match info.url.split("://").nth(1) {
                        Some(u) => u,
                        _ => &info.url,
                    }
                } else {
                    &info.url
                };
                let path = uri.strip_prefix(&host).unwrap_or("/");
                let path = if path.is_empty() { "/" } else { path };
                paths_vec.push(TreePath {
                    path: path.to_string(),
                    url: info.url,
                    count: info.count,
                    methods,
                });
            }

            tree.push(TreeNode {
                host,
                paths: paths_vec,
            });
        }

        Ok(tree)
    }
}

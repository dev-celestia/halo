use halo_core::types::{AIInsight, ActivityLog, CrawlPage, CrawlSession};
use rusqlite::{OptionalExtension, Result as SqlResult, params};

use super::Database;

impl Database {
    pub fn upsert_crawler_session(&self, session: &CrawlSession) -> SqlResult<()> {
        let conn = self.conn.lock();
        let updated_at = chrono::Utc::now().to_rfc3339();
        let created_at = session.started_at.as_deref().unwrap_or(&updated_at);

        conn.execute(
            r#"INSERT INTO crawler_sessions (
                id, target_url, strategy, status, max_depth, max_pages,
                started_at, finished_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                target_url = excluded.target_url,
                strategy = excluded.strategy,
                status = excluded.status,
                max_depth = excluded.max_depth,
                max_pages = excluded.max_pages,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                updated_at = excluded.updated_at"#,
            params![
                session.id,
                session.target_url,
                session.strategy,
                session.status,
                session.max_depth as i64,
                session.max_pages as i64,
                session.started_at,
                session.finished_at,
                created_at,
                updated_at,
            ],
        )?;

        Ok(())
    }

    pub fn get_crawler_session(&self, session_id: &str) -> SqlResult<Option<CrawlSession>> {
        let conn = self.conn.lock();

        conn.query_row(
            r#"SELECT id, target_url, status, strategy, max_depth, max_pages, started_at, finished_at
               FROM crawler_sessions WHERE id = ?1"#,
            params![session_id],
            |row| {
                Ok(CrawlSession {
                    id: row.get(0)?,
                    target_url: row.get(1)?,
                    status: row.get(2)?,
                    strategy: row.get(3)?,
                    max_depth: row.get::<_, i64>(4)? as u32,
                    max_pages: row.get::<_, i64>(5)? as u32,
                    started_at: row.get(6)?,
                    finished_at: row.get(7)?,
                })
            },
        )
        .optional()
    }

    pub fn list_recent_crawler_sessions(&self, limit: u32) -> SqlResult<Vec<CrawlSession>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT id, target_url, status, strategy, max_depth, max_pages, started_at, finished_at
               FROM crawler_sessions
               ORDER BY COALESCE(started_at, created_at) DESC
               LIMIT ?1"#,
        )?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(CrawlSession {
                    id: row.get(0)?,
                    target_url: row.get(1)?,
                    status: row.get(2)?,
                    strategy: row.get(3)?,
                    max_depth: row.get::<_, i64>(4)? as u32,
                    max_pages: row.get::<_, i64>(5)? as u32,
                    started_at: row.get(6)?,
                    finished_at: row.get(7)?,
                })
            })?
            .collect();

        rows
    }

    pub fn delete_crawler_session(&self, session_id: &str) -> SqlResult<usize> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        tx.execute(
            "DELETE FROM crawler_edges WHERE session_id = ?1",
            params![session_id],
        )?;
        tx.execute(
            "DELETE FROM crawler_insights WHERE session_id = ?1",
            params![session_id],
        )?;
        tx.execute(
            "DELETE FROM crawler_logs WHERE session_id = ?1",
            params![session_id],
        )?;
        tx.execute(
            "DELETE FROM crawler_pages WHERE session_id = ?1",
            params![session_id],
        )?;
        let deleted = tx.execute(
            "DELETE FROM crawler_sessions WHERE id = ?1",
            params![session_id],
        )?;

        tx.commit()?;
        Ok(deleted)
    }

    pub fn insert_crawler_edge(
        &self,
        session_id: &str,
        from_url: &str,
        to_url: &str,
        source: &str,
    ) -> SqlResult<()> {
        let conn = self.conn.lock();

        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM crawler_edges WHERE session_id = ?1 AND from_url = ?2 AND to_url = ?3 LIMIT 1",
                params![session_id, from_url, to_url],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Ok(());
        }

        conn.execute(
            "INSERT INTO crawler_edges (id, session_id, from_url, to_url, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                uuid::Uuid::new_v4().to_string(),
                session_id,
                from_url,
                to_url,
                source,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    pub fn upsert_crawler_page(&self, page: &CrawlPage) -> SqlResult<()> {
        let conn = self.conn.lock();
        let updated_at = chrono::Utc::now().to_rfc3339();
        let created_at = &page.discovered_at;

        conn.execute(
            r#"INSERT INTO crawler_pages (
                id, session_id, url, title, status, depth, parent_url, http_status,
                links_found, forms_found, ai_summary, ai_used_for_analysis, interesting,
                screenshot_path, rendered_html_path, discovered_at, visited_at, created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(id) DO UPDATE SET
                url = excluded.url,
                title = excluded.title,
                status = excluded.status,
                depth = excluded.depth,
                parent_url = excluded.parent_url,
                http_status = excluded.http_status,
                links_found = excluded.links_found,
                forms_found = excluded.forms_found,
                ai_summary = excluded.ai_summary,
                ai_used_for_analysis = excluded.ai_used_for_analysis,
                interesting = excluded.interesting,
                screenshot_path = excluded.screenshot_path,
                rendered_html_path = excluded.rendered_html_path,
                discovered_at = excluded.discovered_at,
                visited_at = excluded.visited_at,
                updated_at = excluded.updated_at"#,
            params![
                page.id,
                page.session_id,
                page.url,
                page.title,
                page.status,
                page.depth as i64,
                page.parent_url,
                page.http_status.map(|status| status as i64),
                page.links_found as i64,
                page.forms_found as i64,
                page.ai_summary,
                page.ai_used_for_analysis.map(|used| if used {
                    1i64
                } else {
                    0i64
                }),
                if page.interesting.unwrap_or(false) {
                    1i64
                } else {
                    0i64
                },
                page.screenshot_path,
                page.rendered_html_path,
                page.discovered_at,
                page.visited_at,
                created_at,
                updated_at,
            ],
        )?;

        Ok(())
    }

    pub fn list_crawler_pages(&self, session_id: &str) -> SqlResult<Vec<CrawlPage>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, url, title, status, depth, parent_url, http_status,
               links_found, forms_found, ai_summary, ai_used_for_analysis, interesting,
               screenshot_path, rendered_html_path, discovered_at, visited_at
               FROM crawler_pages WHERE session_id = ?1 ORDER BY depth ASC, discovered_at ASC"#,
        )?;

        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(CrawlPage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    url: row.get(2)?,
                    title: row.get(3)?,
                    status: row.get(4)?,
                    depth: row.get::<_, i64>(5)? as u32,
                    parent_url: row.get(6)?,
                    http_status: row.get::<_, Option<i64>>(7)?.map(|value| value as u16),
                    links_found: row.get::<_, i64>(8)? as u32,
                    forms_found: row.get::<_, i64>(9)? as u32,
                    ai_summary: row.get(10)?,
                    ai_used_for_analysis: row.get::<_, Option<i64>>(11)?.map(|value| value != 0),
                    interesting: Some(row.get::<_, i64>(12)? != 0),
                    screenshot_path: row.get(13)?,
                    rendered_html_path: row.get(14)?,
                    discovered_at: row.get(15)?,
                    visited_at: row.get(16)?,
                })
            })?
            .collect();

        rows
    }

    pub fn clear_crawler_artifact_paths(&self) -> SqlResult<usize> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE crawler_pages SET screenshot_path = NULL, rendered_html_path = NULL WHERE screenshot_path IS NOT NULL OR rendered_html_path IS NOT NULL",
            [],
        )
    }

    pub fn insert_crawler_insight(&self, insight: &AIInsight) -> SqlResult<()> {
        let conn = self.conn.lock();

        conn.execute(
            r#"INSERT OR REPLACE INTO crawler_insights (
                id, session_id, page_id, url, severity, type, title, description, ai_used_for_analysis,
                analysis_source, analysis_tool_id, analysis_tool_name, reviewed, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
            params![
                &insight.id,
                &insight.session_id,
                &insight.page_id,
                &insight.url,
                &insight.severity,
                &insight.r#type,
                &insight.title,
                &insight.description,
                insight
                    .ai_used_for_analysis
                    .map(|used| if used { 1i64 } else { 0i64 }),
                &insight.analysis_source,
                &insight.analysis_tool_id,
                &insight.analysis_tool_name,
                if insight.reviewed { 1i64 } else { 0i64 },
                &insight.created_at,
            ],
        )?;

        Ok(())
    }

    pub fn list_crawler_insights(&self, session_id: &str) -> SqlResult<Vec<AIInsight>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, page_id, severity, type, title, description, url, ai_used_for_analysis,
                      analysis_source, analysis_tool_id, analysis_tool_name, reviewed, created_at
               FROM crawler_insights WHERE session_id = ?1 ORDER BY created_at DESC"#,
        )?;

        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(AIInsight {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    page_id: row.get(2)?,
                    severity: row.get(3)?,
                    r#type: row.get(4)?,
                    title: row.get(5)?,
                    description: row.get(6)?,
                    url: row.get(7)?,
                    ai_used_for_analysis: row.get::<_, Option<i64>>(8)?.map(|value| value != 0),
                    analysis_source: row.get(9)?,
                    analysis_tool_id: row.get(10)?,
                    analysis_tool_name: row.get(11)?,
                    reviewed: row.get::<_, i64>(12)? != 0,
                    created_at: row.get(13)?,
                })
            })?
            .collect();

        rows
    }

    pub fn insert_crawler_log(&self, log: &ActivityLog) -> SqlResult<()> {
        let conn = self.conn.lock();

        conn.execute(
            r#"INSERT OR IGNORE INTO crawler_logs (
                id, session_id, level, type, message, url, ai_used_for_analysis, extra_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                log.id,
                log.session_id,
                log.level,
                log.r#type,
                log.message,
                log.url,
                log.ai_used_for_analysis
                    .map(|used| if used { 1i64 } else { 0i64 }),
                log.extra
                    .as_ref()
                    .and_then(|extra| serde_json::to_string(extra).ok()),
                log.created_at,
            ],
        )?;

        let log_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM crawler_logs WHERE session_id = ?1",
                params![log.session_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if log_count >= 1_200 {
            let _ = conn.execute(
                "DELETE FROM crawler_logs \
                 WHERE session_id = ?1 AND id IN ( \
                     SELECT id FROM crawler_logs \
                     WHERE session_id = ?1 \
                     ORDER BY created_at DESC, id DESC \
                     LIMIT -1 OFFSET 1000 \
                 )",
                params![log.session_id],
            );
        }

        Ok(())
    }

    pub fn list_crawler_logs(&self, session_id: &str) -> SqlResult<Vec<ActivityLog>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, level, type, message, url, ai_used_for_analysis, extra_json, created_at
               FROM crawler_logs WHERE session_id = ?1 ORDER BY created_at ASC"#,
        )?;

        let rows = stmt
            .query_map(params![session_id], |row| {
                let extra_json: Option<String> = row.get(7)?;
                Ok(ActivityLog {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    level: row.get(2)?,
                    r#type: row.get(3)?,
                    message: row.get(4)?,
                    url: row.get(5)?,
                    ai_used_for_analysis: row.get::<_, Option<i64>>(6)?.map(|value| value != 0),
                    extra: extra_json
                        .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok()),
                    created_at: row.get(8)?,
                    human_input_request: None,
                })
            })?
            .collect();

        rows
    }
}

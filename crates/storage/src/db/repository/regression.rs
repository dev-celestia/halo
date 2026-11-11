use rusqlite::{Result as SqlResult, params};
use serde::{Deserialize, Serialize};

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegressionScriptRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub target_url: String,
    pub yaml: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegressionScriptRunRecord {
    pub id: String,
    pub script_id: String,
    pub status: String,
    pub conditions_json: String,
    pub findings_json: String,
    pub logs_json: String,
    pub total_templates: i64,
    pub total_targets: i64,
    pub passed_conditions: i64,
    pub failed_conditions: i64,
    pub elapsed_millis: Option<i64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
}

impl Database {
    // ── Scripts ───────────────────────────────────────────────────────────

    pub fn list_regression_scripts(&self) -> SqlResult<Vec<RegressionScriptRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, target_url, yaml, enabled, created_at, updated_at \
             FROM regression_scripts ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RegressionScriptRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                target_url: row.get(3)?,
                yaml: row.get(4)?,
                enabled: row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_regression_script(&self, id: &str) -> SqlResult<Option<RegressionScriptRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, target_url, yaml, enabled, created_at, updated_at \
             FROM regression_scripts WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(RegressionScriptRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                target_url: row.get(3)?,
                yaml: row.get(4)?,
                enabled: row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn save_regression_script(
        &self,
        id: &str,
        name: &str,
        description: &str,
        target_url: &str,
        yaml: &str,
        enabled: bool,
    ) -> SqlResult<RegressionScriptRecord> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().to_rfc3339();
        let enabled_int = if enabled { 1 } else { 0 };

        let exists: bool = conn
            .prepare("SELECT COUNT(*) FROM regression_scripts WHERE id = ?1")?
            .query_row(params![id], |row| row.get::<_, i32>(0))
            .map(|count| count > 0)
            .unwrap_or(false);

        if exists {
            conn.execute(
                "UPDATE regression_scripts SET name = ?1, description = ?2, target_url = ?3, \
                 yaml = ?4, enabled = ?5, updated_at = ?6 WHERE id = ?7",
                params![name, description, target_url, yaml, enabled_int, now, id],
            )?;
        } else {
            conn.execute(
                "INSERT INTO regression_scripts (id, name, description, target_url, yaml, \
                 enabled, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    name,
                    description,
                    target_url,
                    yaml,
                    enabled_int,
                    now,
                    now
                ],
            )?;
        }

        Ok(RegressionScriptRecord {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            target_url: target_url.to_string(),
            yaml: yaml.to_string(),
            enabled,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn delete_regression_script(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM regression_scripts WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Runs ──────────────────────────────────────────────────────────────

    pub fn list_regression_script_runs(
        &self,
        script_id: &str,
    ) -> SqlResult<Vec<RegressionScriptRunRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, script_id, status, conditions_json, findings_json, logs_json, \
             total_templates, total_targets, passed_conditions, failed_conditions, \
             elapsed_millis, started_at, finished_at, error, created_at \
             FROM regression_script_runs WHERE script_id = ?1 ORDER BY created_at DESC LIMIT 50",
        )?;
        let rows = stmt.query_map(params![script_id], |row| {
            Ok(RegressionScriptRunRecord {
                id: row.get(0)?,
                script_id: row.get(1)?,
                status: row.get(2)?,
                conditions_json: row.get(3)?,
                findings_json: row.get(4)?,
                logs_json: row.get(5)?,
                total_templates: row.get(6)?,
                total_targets: row.get(7)?,
                passed_conditions: row.get(8)?,
                failed_conditions: row.get(9)?,
                elapsed_millis: row.get(10)?,
                started_at: row.get(11)?,
                finished_at: row.get(12)?,
                error: row.get(13)?,
                created_at: row.get(14)?,
            })
        })?;
        rows.collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_regression_script_run(
        &self,
        id: &str,
        script_id: &str,
        status: &str,
        conditions_json: &str,
        total_templates: i64,
    ) -> SqlResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO regression_script_runs (id, script_id, status, conditions_json, \
             total_templates, total_targets, started_at, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            params![id, script_id, status, conditions_json, total_templates, now],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_regression_script_run(
        &self,
        id: &str,
        status: &str,
        conditions_json: &str,
        findings_json: &str,
        logs_json: &str,
        passed_conditions: i64,
        failed_conditions: i64,
        elapsed_millis: Option<i64>,
        error: Option<&str>,
    ) -> SqlResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE regression_script_runs SET status = ?1, conditions_json = ?2, findings_json = ?3, \
             logs_json = ?4, passed_conditions = ?5, failed_conditions = ?6, elapsed_millis = ?7, \
             error = ?8, finished_at = ?9 WHERE id = ?10",
            params![
                status,
                conditions_json,
                findings_json,
                logs_json,
                passed_conditions,
                failed_conditions,
                elapsed_millis,
                error,
                now,
                id
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Database;

    #[test]
    fn test_regression_script_crud() {
        let db = Database::new(std::path::PathBuf::from(":memory:"))
            .expect("failed to create in-memory db");
        db.init().expect("db init failed");

        let saved = db
            .save_regression_script(
                "script-1",
                "Login reachable",
                "Checks the login page",
                "https://example.com",
                "id: login-reachable\ninfo:\n  name: Login reachable\n",
                true,
            )
            .expect("save failed");

        assert_eq!(saved.id, "script-1");

        let list = db.list_regression_scripts().expect("list failed");
        assert_eq!(list.len(), 1);

        db.save_regression_script(
            "script-1",
            "Login reachable v2",
            "Updated",
            "https://example.com",
            "id: login-reachable\n",
            false,
        )
        .expect("update failed");

        let updated = db
            .get_regression_script("script-1")
            .expect("get failed")
            .expect("script missing");
        assert_eq!(updated.name, "Login reachable v2");
        assert!(!updated.enabled);

        db.create_regression_script_run("run-1", "script-1", "running", "[]", 1)
            .expect("create run failed");
        db.finish_regression_script_run(
            "run-1",
            "completed",
            "[]",
            "[]",
            "[]",
            1,
            0,
            Some(120),
            None,
        )
        .expect("finish run failed");

        let runs = db
            .list_regression_script_runs("script-1")
            .expect("list runs failed");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "completed");
        assert_eq!(runs[0].passed_conditions, 1);

        db.delete_regression_script("script-1")
            .expect("delete failed");
        let after = db.list_regression_scripts().expect("list failed");
        assert!(after.is_empty());
    }
}

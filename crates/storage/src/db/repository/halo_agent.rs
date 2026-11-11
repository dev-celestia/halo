use rusqlite::{Result as SqlResult, params};

use super::Database;
use super::types::{HaloAgentMessageRecord, HaloAgentSessionRecord};

const SESSION_COLUMNS: &str = "id, title, preview, updated_at, archived, root_path, \
    harness_session_id, model, plan_mode, approval_mode, draft, attachments_json, todos_json, \
    file_changes_json, queue_json, todo_expanded, todo_dismissed, subagents_json, skills_json, \
    mcp_servers_json";

fn row_to_session(row: &rusqlite::Row<'_>) -> SqlResult<HaloAgentSessionRecord> {
    Ok(HaloAgentSessionRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        preview: row.get(2)?,
        updated_at: row.get::<_, i64>(3)? as u64,
        archived: row.get::<_, i64>(4)? != 0,
        root_path: row.get(5)?,
        harness_session_id: row.get(6)?,
        model: row.get(7)?,
        plan_mode: row.get::<_, i64>(8)? != 0,
        approval_mode: row.get(9)?,
        draft: row.get(10)?,
        attachments_json: row.get(11)?,
        todos_json: row.get(12)?,
        file_changes_json: row.get(13)?,
        queue_json: row.get(14)?,
        todo_expanded: row.get::<_, i64>(15)? != 0,
        todo_dismissed: row.get::<_, i64>(16)? != 0,
        subagents_json: row.get(17)?,
        skills_json: row.get(18)?,
        mcp_servers_json: row.get(19)?,
        messages: Vec::new(),
    })
}

fn load_messages(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> SqlResult<Vec<HaloAgentMessageRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, parts_json, usage_json FROM halo_agent_messages \
         WHERE session_id = ?1 ORDER BY ord ASC",
    )?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok(HaloAgentMessageRecord {
                id: row.get(0)?,
                role: row.get(1)?,
                parts_json: row.get(2)?,
                usage_json: row.get(3)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
    Ok(rows)
}

impl Database {
    pub fn list_halo_agent_sessions(&self) -> SqlResult<Vec<HaloAgentSessionRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM halo_agent_sessions ORDER BY updated_at DESC"
        ))?;
        let mut sessions = stmt
            .query_map([], row_to_session)?
            .collect::<SqlResult<Vec<_>>>()?;
        for session in &mut sessions {
            session.messages = load_messages(&conn, &session.id)?;
        }
        Ok(sessions)
    }

    pub fn get_halo_agent_session(&self, id: &str) -> SqlResult<Option<HaloAgentSessionRecord>> {
        let conn = self.conn.lock();
        let mut session = match conn.query_row(
            &format!("SELECT {SESSION_COLUMNS} FROM halo_agent_sessions WHERE id = ?1"),
            params![id],
            row_to_session,
        ) {
            Ok(session) => session,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(err) => return Err(err),
        };
        session.messages = load_messages(&conn, &session.id)?;
        Ok(Some(session))
    }

    /// Replace the session row and its message list atomically.
    pub fn upsert_halo_agent_session(&self, record: &HaloAgentSessionRecord) -> SqlResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute(
            r#"INSERT INTO halo_agent_sessions (
                   id, title, preview, updated_at, archived, root_path,
                   harness_session_id, model, plan_mode, approval_mode, draft,
                   attachments_json, todos_json, file_changes_json, queue_json,
                   todo_expanded, todo_dismissed, subagents_json, skills_json, mcp_servers_json
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
               ON CONFLICT(id) DO UPDATE SET
                   title = excluded.title,
                   preview = excluded.preview,
                   updated_at = excluded.updated_at,
                   archived = excluded.archived,
                   root_path = excluded.root_path,
                   harness_session_id = excluded.harness_session_id,
                   model = excluded.model,
                   plan_mode = excluded.plan_mode,
                   approval_mode = excluded.approval_mode,
                   draft = excluded.draft,
                   attachments_json = excluded.attachments_json,
                   todos_json = excluded.todos_json,
                   file_changes_json = excluded.file_changes_json,
                   queue_json = excluded.queue_json,
                   todo_expanded = excluded.todo_expanded,
                   todo_dismissed = excluded.todo_dismissed,
                   subagents_json = excluded.subagents_json,
                   skills_json = excluded.skills_json,
                   mcp_servers_json = excluded.mcp_servers_json"#,
            params![
                record.id,
                record.title,
                record.preview,
                record.updated_at as i64,
                record.archived as i64,
                record.root_path,
                record.harness_session_id,
                record.model,
                record.plan_mode as i64,
                record.approval_mode,
                record.draft,
                record.attachments_json,
                record.todos_json,
                record.file_changes_json,
                record.queue_json,
                record.todo_expanded as i64,
                record.todo_dismissed as i64,
                record.subagents_json,
                record.skills_json,
                record.mcp_servers_json,
            ],
        )?;
        tx.execute(
            "DELETE FROM halo_agent_messages WHERE session_id = ?1",
            params![record.id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO halo_agent_messages \
                 (session_id, ord, id, role, parts_json, usage_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (ord, message) in record.messages.iter().enumerate() {
                stmt.execute(params![
                    record.id,
                    ord as i64,
                    message.id,
                    message.role,
                    message.parts_json,
                    message.usage_json,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_halo_agent_session(&self, id: &str) -> SqlResult<usize> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM halo_agent_sessions WHERE id = ?1", params![id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repository::Database;
    use tempfile::tempdir;

    #[test]
    fn upsert_list_delete_round_trip() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().join("test.db")).unwrap();
        db.init().unwrap();

        let record = HaloAgentSessionRecord {
            id: "chat-1".into(),
            title: "Hello".into(),
            preview: "world".into(),
            updated_at: 1_700_000_000_000,
            archived: false,
            root_path: Some("/tmp/proj".into()),
            harness_session_id: Some("/tmp/pi-session.jsonl".into()),
            model: Some("anthropic/claude-sonnet-5-5".into()),
            plan_mode: false,
            approval_mode: "ask".into(),
            draft: "next".into(),
            attachments_json: "[]".into(),
            todos_json: "[]".into(),
            file_changes_json: "[]".into(),
            queue_json: "[]".into(),
            todo_expanded: true,
            todo_dismissed: false,
            subagents_json: "[]".into(),
            skills_json: r#"["demo"]"#.into(),
            mcp_servers_json: r#"["files"]"#.into(),
            messages: vec![HaloAgentMessageRecord {
                id: "m1".into(),
                role: "user".into(),
                parts_json: r#"[{"type":"text","text":"hi"}]"#.into(),
                usage_json: String::new(),
            }],
        };
        db.upsert_halo_agent_session(&record).unwrap();

        let listed = db.list_halo_agent_sessions().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Hello");
        assert_eq!(listed[0].messages.len(), 1);
        assert_eq!(listed[0].messages[0].role, "user");

        let got = db.get_halo_agent_session("chat-1").unwrap().unwrap();
        assert_eq!(
            got.harness_session_id.as_deref(),
            Some("/tmp/pi-session.jsonl")
        );
        assert_eq!(got.skills_json, r#"["demo"]"#);
        assert_eq!(got.mcp_servers_json, r#"["files"]"#);

        assert_eq!(db.delete_halo_agent_session("chat-1").unwrap(), 1);
        assert!(db.list_halo_agent_sessions().unwrap().is_empty());
    }
}

use rusqlite::{Result as SqlResult, params};

use super::Database;
use super::types::DocumentRecord;

impl Database {
    pub fn get_documents(&self) -> SqlResult<Vec<DocumentRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"SELECT id, name, title, sections, custom_sections, removed_built_in_sections, api_entries, created_at, updated_at
               FROM documents
               ORDER BY created_at ASC"#,
        )?;
        let rows = stmt.query_map([], row_to_document_record)?;

        rows.collect()
    }

    pub fn upsert_document(&self, document: &DocumentRecord) -> SqlResult<()> {
        let conn = self.conn.lock();
        let sections = serde_json::to_string(&document.sections).unwrap_or_else(|_| "{}".into());
        let custom_sections =
            serde_json::to_string(&document.custom_sections).unwrap_or_else(|_| "[]".into());
        let removed_built_in_sections = serde_json::to_string(&document.removed_built_in_sections)
            .unwrap_or_else(|_| "[]".into());
        let api_entries =
            serde_json::to_string(&document.api_entries).unwrap_or_else(|_| "[]".into());

        conn.execute(
            r#"INSERT INTO documents (
                id, name, title, sections, custom_sections, removed_built_in_sections, api_entries, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                title = excluded.title,
                sections = excluded.sections,
                custom_sections = excluded.custom_sections,
                removed_built_in_sections = excluded.removed_built_in_sections,
                api_entries = excluded.api_entries,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at"#,
            params![
                document.id,
                document.name,
                document.title,
                sections,
                custom_sections,
                removed_built_in_sections,
                api_entries,
                document.created_at,
                document.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_document(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn row_to_document_record(row: &rusqlite::Row) -> SqlResult<DocumentRecord> {
    let sections: String = row.get("sections")?;
    let custom_sections: String = row.get("custom_sections")?;
    let removed_built_in_sections: String = row.get("removed_built_in_sections")?;
    let api_entries: String = row.get("api_entries")?;

    Ok(DocumentRecord {
        id: row.get("id")?,
        name: row.get("name")?,
        title: row.get("title")?,
        sections: serde_json::from_str(&sections).unwrap_or_else(|_| serde_json::json!({})),
        custom_sections: serde_json::from_str(&custom_sections)
            .unwrap_or_else(|_| serde_json::json!([])),
        removed_built_in_sections: serde_json::from_str(&removed_built_in_sections)
            .unwrap_or_else(|_| serde_json::json!([])),
        api_entries: serde_json::from_str(&api_entries).unwrap_or_else(|_| serde_json::json!([])),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

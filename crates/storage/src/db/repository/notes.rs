use rusqlite::{Result as SqlResult, params};

use super::Database;
use super::types::NoteRecord;

const NOTE_COLUMNS: &str = "id, name, note, created_at, updated_at";

fn row_to_note(row: &rusqlite::Row<'_>) -> SqlResult<NoteRecord> {
    Ok(NoteRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        note: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

impl Database {
    pub fn list_notes(&self) -> SqlResult<Vec<NoteRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {NOTE_COLUMNS} FROM notes ORDER BY updated_at DESC"
        ))?;
        let rows = stmt
            .query_map([], row_to_note)?
            .collect::<SqlResult<Vec<_>>>()?;
        Ok(rows)
    }

    /// Inserts or updates a note and returns the stored row. `created_at` is absent from
    /// the update set on purpose: a save must not reset the original creation time.
    pub fn upsert_note(&self, record: &NoteRecord) -> SqlResult<NoteRecord> {
        let conn = self.conn.lock();
        conn.execute(
            r#"INSERT INTO notes (id, name, note, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   note = excluded.note,
                   updated_at = excluded.updated_at"#,
            params![
                record.id,
                record.name,
                record.note,
                record.created_at,
                record.updated_at
            ],
        )?;

        conn.query_row(
            &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
            params![record.id],
            row_to_note,
        )
    }

    /// One-time import of notes recovered from localStorage. `INSERT OR IGNORE` keeps it
    /// idempotent per id, so a partially-failed import can be retried without duplicating.
    pub fn import_notes(&self, records: &[NoteRecord]) -> SqlResult<usize> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                r#"INSERT OR IGNORE INTO notes (id, name, note, created_at, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5)"#,
            )?;
            for record in records {
                inserted += stmt.execute(params![
                    record.id,
                    record.name,
                    record.note,
                    record.created_at,
                    record.updated_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn delete_note(&self, id: &str) -> SqlResult<usize> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
    }

    pub fn delete_notes(&self, ids: &[String]) -> SqlResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let conn = self.conn.lock();
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        conn.execute(
            &format!("DELETE FROM notes WHERE id IN ({placeholders})"),
            rusqlite::params_from_iter(ids.iter()),
        )
    }
}

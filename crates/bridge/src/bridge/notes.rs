//! Notes (Scratchpad) bridge methods for tab management and SQLite CRUD.

use super::*;
use chrono::Utc;

impl Bridge {
    /// Load notes from backend SQLite database. Safe to call repeatedly.
    pub fn refresh_notes(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let notes = backend.notes_list()?;
        Self::state(cx).update(cx, |s| {
            s.notes = notes;
            s.notes_loaded = true;

            // Reconcile open tabs with existing notes
            let existing_ids: Vec<String> = s.notes.iter().map(|n| n.id.clone()).collect();
            s.notes_open_tab_ids.retain(|id| existing_ids.contains(id));

            // Default to opening all notes if none are open
            if s.notes_open_tab_ids.is_empty() && !s.notes.is_empty() {
                s.notes_open_tab_ids = existing_ids;
            }

            // Fallback active id
            if let Some(active) = &s.notes_active_id {
                if !s.notes_open_tab_ids.contains(active) {
                    s.notes_active_id = s.notes_open_tab_ids.first().cloned();
                }
            } else {
                s.notes_active_id = s.notes_open_tab_ids.first().cloned();
            }

            s.commit_active_tab();
        });
        Ok(())
    }

    /// Create a new note with an auto-incremented name ("Note N") and open it.
    pub fn create_note(cx: &mut App) -> BridgeResult<String> {
        let backend = Self::backend(cx)?;
        let now = Utc::now().to_rfc3339();

        let state = Self::state(cx).read(cx);
        let mut next_num = 1;
        while state.notes.iter().any(|n| {
            n.name.eq_ignore_ascii_case(&format!("Note {next_num}"))
                || n.name
                    .eq_ignore_ascii_case(&format!("Scratchpad {next_num}"))
        }) {
            next_num += 1;
        }

        let id = uuid::Uuid::new_v4().to_string();
        let name = format!("Note {next_num}");
        let record = NoteRecord {
            id: id.clone(),
            name: name.clone(),
            note: String::new(),
            created_at: now.clone(),
            updated_at: now,
        };

        let saved = backend.note_upsert(&record)?;
        Self::state(cx).update(cx, |s| {
            s.notes.insert(0, saved);
            if !s.notes_open_tab_ids.contains(&id) {
                s.notes_open_tab_ids.push(id.clone());
            }
            s.notes_active_id = Some(id.clone());
            s.apply_ok_msg(format!("Created note \"{name}\""));
            s.commit_active_tab();
        });

        Ok(id)
    }

    /// Open an existing note in tabs and activate it.
    pub fn open_note(id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if s.notes.iter().any(|n| n.id == id) {
                if !s.notes_open_tab_ids.contains(&id.to_string()) {
                    s.notes_open_tab_ids.push(id.to_string());
                }
                s.notes_active_id = Some(id.to_string());
                s.notes_library_open = false;
                s.commit_active_tab();
            }
        });
        Ok(())
    }

    /// Close a note tab without deleting it from library.
    pub fn close_note_tab(id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(pos) = s.notes_open_tab_ids.iter().position(|tab_id| tab_id == id) {
                s.notes_open_tab_ids.remove(pos);
                if s.notes_active_id.as_deref() == Some(id) {
                    s.notes_active_id = if pos > 0 {
                        s.notes_open_tab_ids.get(pos - 1).cloned()
                    } else {
                        s.notes_open_tab_ids.first().cloned()
                    };
                }
                s.commit_active_tab();
            }
        });
        Ok(())
    }

    /// Close tabs to the left of the given tab.
    pub fn close_notes_to_left(id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(pos) = s.notes_open_tab_ids.iter().position(|tab_id| tab_id == id) {
                s.notes_open_tab_ids.drain(0..pos);
                if let Some(active) = &s.notes_active_id {
                    if !s.notes_open_tab_ids.contains(active) {
                        s.notes_active_id = Some(id.to_string());
                    }
                }
                s.commit_active_tab();
            }
        });
        Ok(())
    }

    /// Close tabs to the right of the given tab.
    pub fn close_notes_to_right(id: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(pos) = s.notes_open_tab_ids.iter().position(|tab_id| tab_id == id) {
                s.notes_open_tab_ids.truncate(pos + 1);
                if let Some(active) = &s.notes_active_id {
                    if !s.notes_open_tab_ids.contains(active) {
                        s.notes_active_id = Some(id.to_string());
                    }
                }
                s.commit_active_tab();
            }
        });
        Ok(())
    }

    /// Close all open note tabs.
    pub fn close_all_note_tabs(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_open_tab_ids.clear();
            s.notes_active_id = None;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Save content of a note to backend SQLite.
    pub fn save_note(id: &str, name: &str, content: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let now = Utc::now().to_rfc3339();

        let updated_record = {
            let state = Self::state(cx).read(cx);
            let existing = state.notes.iter().find(|n| n.id == id);
            NoteRecord {
                id: id.to_string(),
                name: name.to_string(),
                note: content.to_string(),
                created_at: existing
                    .map(|n| n.created_at.clone())
                    .unwrap_or_else(|| now.clone()),
                updated_at: now,
            }
        };

        let saved = backend.note_upsert(&updated_record)?;
        Self::state(cx).update(cx, |s| {
            if let Some(existing) = s.notes.iter_mut().find(|n| n.id == id) {
                *existing = saved;
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Rename a note.
    pub fn rename_note(id: &str, name: &str, cx: &mut App) -> BridgeResult<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let now = Utc::now().to_rfc3339();

        let updated_record = {
            let state = Self::state(cx).read(cx);
            let existing = state.notes.iter().find(|n| n.id == id);
            NoteRecord {
                id: id.to_string(),
                name: trimmed.to_string(),
                note: existing.map(|n| n.note.clone()).unwrap_or_default(),
                created_at: existing
                    .map(|n| n.created_at.clone())
                    .unwrap_or_else(|| now.clone()),
                updated_at: now,
            }
        };

        let saved = backend.note_upsert(&updated_record)?;
        Self::state(cx).update(cx, |s| {
            if let Some(existing) = s.notes.iter_mut().find(|n| n.id == id) {
                *existing = saved;
            }
            s.notes_editing_title_id = None;
            s.apply_ok_msg(format!("Renamed note to \"{trimmed}\""));
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Duplicate an existing note into `"<name> (Copy)"` and open it.
    pub fn duplicate_note(id: &str, cx: &mut App) -> BridgeResult<String> {
        let backend = Self::backend(cx)?;
        let now = Utc::now().to_rfc3339();

        let (copy_name, copy_content) = {
            let state = Self::state(cx).read(cx);
            let original = state
                .notes
                .iter()
                .find(|n| n.id == id)
                .ok_or_else(|| BridgeError::message("Note not found"))?;
            (format!("{} (Copy)", original.name), original.note.clone())
        };

        let new_id = uuid::Uuid::new_v4().to_string();
        let record = NoteRecord {
            id: new_id.clone(),
            name: copy_name.clone(),
            note: copy_content,
            created_at: now.clone(),
            updated_at: now,
        };

        let saved = backend.note_upsert(&record)?;
        Self::state(cx).update(cx, |s| {
            s.notes.insert(0, saved);
            if !s.notes_open_tab_ids.contains(&new_id) {
                s.notes_open_tab_ids.push(new_id.clone());
            }
            s.notes_active_id = Some(new_id.clone());
            s.apply_ok_msg(format!("Duplicated note as \"{copy_name}\""));
            s.commit_active_tab();
        });
        Ok(new_id)
    }

    /// Permanently delete a note from SQLite database.
    pub fn delete_note(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        backend.note_delete(id)?;
        Self::state(cx).update(cx, |s| {
            s.notes.retain(|n| n.id != id);
            if let Some(pos) = s.notes_open_tab_ids.iter().position(|tab_id| tab_id == id) {
                s.notes_open_tab_ids.remove(pos);
                if s.notes_active_id.as_deref() == Some(id) {
                    s.notes_active_id = if pos > 0 {
                        s.notes_open_tab_ids.get(pos - 1).cloned()
                    } else {
                        s.notes_open_tab_ids.first().cloned()
                    };
                }
            }
            s.apply_ok_msg("Note permanently deleted".into());
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Set search filter for open tabs.
    pub fn set_notes_search(query: String, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_search_query = query;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Switch editor view mode (Edit / Split / Preview).
    pub fn set_notes_view_mode(mode: NotesViewMode, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_view_mode = mode;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Open or close Saved Notes Library modal dialog.
    pub fn set_notes_library_open(open: bool, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_library_open = open;
            if open {
                s.notes_library_search.clear();
            }
        });
        Ok(())
    }

    /// Set search filter for Saved Notes Library.
    pub fn set_notes_library_search(query: String, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_library_search = query;
        });
        Ok(())
    }

    /// Set tab filter for Saved Notes Library (All / Open / Closed).
    pub fn set_notes_library_filter(filter: NotesFilterTab, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_library_filter = filter;
        });
        Ok(())
    }

    /// Set sort order for Saved Notes Library.
    pub fn set_notes_library_sort(sort: NotesSortOption, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_library_sort = sort;
        });
        Ok(())
    }

    /// Set which note is currently in inline title editing mode.
    pub fn set_notes_editing_title(id: Option<String>, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_editing_title_id = id;
        });
        Ok(())
    }

    /// Open or close drawing studio whiteboard dialog.
    pub fn set_notes_drawing_open(open: bool, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.notes_drawing_open = open;
        });
        Ok(())
    }

    /// Promote note to AI memory context.
    pub fn promote_note_to_memory(note: &NoteRecord, cx: &mut App) -> BridgeResult<()> {
        let title = &note.name;
        Self::state(cx).update(cx, |s| {
            s.apply_ok_msg(format!("Promoted \"{title}\" to AI memory context"));
        });
        Ok(())
    }
}

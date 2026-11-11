//! Notes scratchpad helpers.

use super::*;

impl BridgeState {
    // ── Notes helpers ────────────────────────────────────────────────────────

    pub fn active_note(&self) -> Option<&NoteRecord> {
        let id = self.notes_active_id.as_deref()?;
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn active_note_mut(&mut self) -> Option<&mut NoteRecord> {
        let id = self.notes_active_id.clone()?;
        self.notes.iter_mut().find(|n| n.id == id)
    }

    pub fn filtered_open_notes(&self) -> Vec<&NoteRecord> {
        let q = self.notes_search_query.trim().to_ascii_lowercase();
        self.notes_open_tab_ids
            .iter()
            .filter_map(|id| self.notes.iter().find(|n| &n.id == id))
            .filter(|note| {
                if q.is_empty() {
                    return true;
                }
                note.name.to_ascii_lowercase().contains(&q)
                    || note.note.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn filtered_and_sorted_library_notes(&self) -> Vec<&NoteRecord> {
        let q = self.notes_library_search.trim().to_ascii_lowercase();
        let mut list: Vec<&NoteRecord> = self
            .notes
            .iter()
            .filter(|note| match self.notes_library_filter {
                NotesFilterTab::All => true,
                NotesFilterTab::Open => self.notes_open_tab_ids.contains(&note.id),
                NotesFilterTab::Closed => !self.notes_open_tab_ids.contains(&note.id),
            })
            .filter(|note| {
                if q.is_empty() {
                    return true;
                }
                note.name.to_ascii_lowercase().contains(&q)
                    || note.note.to_ascii_lowercase().contains(&q)
            })
            .collect();

        match self.notes_library_sort {
            NotesSortOption::UpdatedDesc => {
                list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            }
            NotesSortOption::UpdatedAsc => {
                list.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
            }
            NotesSortOption::TitleAsc => {
                list.sort_by(|a, b| {
                    a.name
                        .to_ascii_lowercase()
                        .cmp(&b.name.to_ascii_lowercase())
                });
            }
            NotesSortOption::TitleDesc => {
                list.sort_by(|a, b| {
                    b.name
                        .to_ascii_lowercase()
                        .cmp(&a.name.to_ascii_lowercase())
                });
            }
            NotesSortOption::CreatedDesc => {
                list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
        }

        list
    }

    pub fn library_counts(&self) -> (usize, usize, usize) {
        let all = self.notes.len();
        let open = self
            .notes
            .iter()
            .filter(|n| self.notes_open_tab_ids.contains(&n.id))
            .count();
        let closed = all.saturating_sub(open);
        (all, open, closed)
    }
}

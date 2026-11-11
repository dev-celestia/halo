//! Quick Open (Go to File, ⌘P / Ctrl+P) in Coder.
//!
//! Searches files across the open workspace using fuzzy path matching,
//! allows keyboard navigation (Up, Down, Enter, Esc), and opens the selected
//! file into the active Coder editor pane.

use std::collections::HashSet;
use std::path::Path;

use halo_backend::commands::halo_agent::FileSearchMatch;

use super::*;

impl Bridge {
    /// Toggle Quick Open modal.
    pub fn coder_quick_open_toggle(cx: &mut App) -> BridgeResult<bool> {
        let root = Self::state(cx).read(cx).coder.root_path.clone();
        let Some(root) = root else {
            return Ok(false);
        };

        let now_open = Self::state(cx).update(cx, |s| {
            s.coder.quick_open_open = !s.coder.quick_open_open;
            s.coder.quick_open_query.clear();
            s.coder.quick_open_selected = 0;
            s.coder.quick_open_open
        });

        if now_open {
            Self::coder_quick_open_refresh(&root, "", cx)?;
        }

        Ok(now_open)
    }

    /// Close Quick Open modal.
    pub fn coder_quick_open_close(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.coder.quick_open_open = false;
        });
    }

    /// Update search query and re-run search.
    pub fn coder_quick_open_query(query: &str, cx: &mut App) -> BridgeResult<()> {
        let root = Self::state(cx).read(cx).coder.root_path.clone();
        let Some(root) = root else {
            return Ok(());
        };

        Self::state(cx).update(cx, |s| {
            s.coder.quick_open_query = query.to_string();
            s.coder.quick_open_selected = 0;
        });
        Self::coder_quick_open_refresh(&root, query, cx)
    }

    fn coder_quick_open_refresh(root: &str, query: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let raw_matches = backend.halo_agent_search_files(root, query)?;
        let mut files: Vec<FileSearchMatch> =
            raw_matches.into_iter().filter(|m| !m.is_dir).collect();

        // If query is empty, prioritize open files at the top
        if query.trim().is_empty() {
            let state = Self::state(cx).read(cx);
            let root_path = Path::new(root);
            let mut prioritized = Vec::new();
            for of in &state.coder.open_files {
                let p = Path::new(&of.path);
                if let Ok(rel) = p.strip_prefix(root_path) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if !rel_str.is_empty() {
                        prioritized.push(FileSearchMatch {
                            path: rel_str,
                            is_dir: false,
                        });
                    }
                }
            }
            // Prepend open files (deduped)
            let mut seen = HashSet::new();
            let mut combined = Vec::new();
            for m in prioritized {
                if seen.insert(m.path.clone()) {
                    combined.push(m);
                }
            }
            for m in files {
                if seen.insert(m.path.clone()) {
                    combined.push(m);
                }
            }
            files = combined;
        }

        Self::state(cx).update(cx, |s| {
            s.coder.quick_open_matches = files;
            if s.coder.quick_open_matches.is_empty() {
                s.coder.quick_open_selected = 0;
            } else if s.coder.quick_open_selected >= s.coder.quick_open_matches.len() {
                s.coder.quick_open_selected = s.coder.quick_open_matches.len() - 1;
            }
        });

        Ok(())
    }

    /// Move selection in Quick Open.
    pub fn coder_quick_open_select(delta: isize, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let total = s.coder.quick_open_matches.len();
            if total == 0 {
                s.coder.quick_open_selected = 0;
                return;
            }
            let cur = s.coder.quick_open_selected as isize;
            let next = (cur + delta).rem_euclid(total as isize) as usize;
            s.coder.quick_open_selected = next;
        });
    }

    /// Confirm and open the selected file.
    pub fn coder_quick_open_confirm(
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let (root, selected_path) = Self::state(cx).update(cx, |s| {
            s.coder.quick_open_open = false;
            let root = s.coder.root_path.clone();
            let path = s
                .coder
                .quick_open_matches
                .get(s.coder.quick_open_selected)
                .map(|m| m.path.clone());
            (root, path)
        });

        let Some(root) = root else {
            return Ok(());
        };
        let Some(rel_path) = selected_path else {
            return Ok(());
        };

        let abs_path = Path::new(&root)
            .join(&rel_path)
            .to_string_lossy()
            .to_string();
        Self::coder_open_file(&abs_path, editors, window, cx)
    }

    /// Open file at a specific match index.
    pub fn coder_quick_open_open_index(
        index: usize,
        editors: &[Entity<CodeEditor>],
        window: &mut Window,
        cx: &mut App,
    ) -> BridgeResult<()> {
        let (root, selected_path) = Self::state(cx).update(cx, |s| {
            s.coder.quick_open_open = false;
            let root = s.coder.root_path.clone();
            let path = s
                .coder
                .quick_open_matches
                .get(index)
                .map(|m| m.path.clone());
            (root, path)
        });

        let Some(root) = root else {
            return Ok(());
        };
        let Some(rel_path) = selected_path else {
            return Ok(());
        };

        let abs_path = Path::new(&root)
            .join(&rel_path)
            .to_string_lossy()
            .to_string();
        Self::coder_open_file(&abs_path, editors, window, cx)
    }
}

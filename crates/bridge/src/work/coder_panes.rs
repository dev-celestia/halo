//! Editor-pane and terminal accessors on [`CoderWork`].

use super::*;

impl CoderWork {
    pub fn active_terminal(&self) -> Option<&CoderTerminal> {
        let id = self.active_terminal_id.as_deref()?;
        self.terminals.iter().find(|t| t.id == id)
    }

    pub fn active_terminal_mut(&mut self) -> Option<&mut CoderTerminal> {
        let id = self.active_terminal_id.clone()?;
        self.terminals.iter_mut().find(|t| t.id == id)
    }

    /// Active path for the focused editor pane.
    pub fn focused_active_path(&self) -> Option<&str> {
        self.pane_active(self.focused_pane)
    }

    pub fn set_focused_active_path(&mut self, path: Option<String>) {
        let id = self.focused_pane;
        if let Some(pane) = self.pane_mut(id) {
            pane.active = path;
        }
    }

    /// Tab paths for a pane id.
    pub fn pane_tabs(&self, pane_id: u8) -> &[String] {
        self.pane(pane_id).map(|p| p.tabs.as_slice()).unwrap_or(&[])
    }

    /// Compatibility alias — same as [`Self::pane_tabs`].
    pub fn group_tabs(&self, group: u8) -> &[String] {
        self.pane_tabs(group)
    }

    pub fn pane_active(&self, pane_id: u8) -> Option<&str> {
        self.pane(pane_id).and_then(|p| p.active.as_deref())
    }

    /// Compatibility alias — same as [`Self::pane_active`].
    pub fn group_active(&self, group: u8) -> Option<&str> {
        self.pane_active(group)
    }

    pub fn pane(&self, id: u8) -> Option<&CoderPane> {
        self.panes.iter().find(|p| p.id == id)
    }

    pub fn pane_mut(&mut self, id: u8) -> Option<&mut CoderPane> {
        self.panes.iter_mut().find(|p| p.id == id)
    }

    /// Ensure the root leaf has tabs when migrating from a bare `open_files` list.
    pub fn seed_primary_tabs_if_empty(&mut self) {
        if self.pane(0).is_some_and(|p| !p.tabs.is_empty()) || self.open_files.is_empty() {
            return;
        }
        let tabs: Vec<String> = self.open_files.iter().map(|f| f.path.clone()).collect();
        let active = tabs.last().cloned();
        if let Some(pane) = self.pane_mut(0) {
            pane.tabs = tabs;
            if pane.active.is_none() {
                pane.active = active;
            }
        }
    }

    /// Depth of the leaf `pane_id` in the layout tree (`0` = root leaf).
    pub fn pane_depth(&self, pane_id: u8) -> u8 {
        self.layout.depth_of(pane_id).unwrap_or(0)
    }

    pub fn can_split_pane(&self, pane_id: u8) -> bool {
        self.panes.len() < CODER_MAX_PANES
            && self.pane_depth(pane_id) < CODER_MAX_SPLIT_DEPTH
            && self.layout.contains_leaf(pane_id)
    }

    pub fn is_split(&self) -> bool {
        !matches!(self.layout, CoderLayout::Leaf(_))
    }

    /// Axis of the root split, if the layout is currently split.
    pub fn split_axis(&self) -> Option<CoderSplitAxis> {
        match &self.layout {
            CoderLayout::Split { axis, .. } => Some(*axis),
            CoderLayout::Leaf(_) => None,
        }
    }

    /// Allocate the next free pane id, or `None` if at capacity.
    pub fn alloc_pane_id(&self) -> Option<u8> {
        (0..CODER_MAX_PANES as u8).find(|id| !self.panes.iter().any(|p| p.id == *id))
    }

    /// Split `pane_id` along `axis`. Returns the new pane id.
    pub fn split_pane(&mut self, pane_id: u8, axis: CoderSplitAxis) -> Result<u8, &'static str> {
        if !self.can_split_pane(pane_id) {
            return Err("Cannot split further");
        }
        let new_id = self.alloc_pane_id().ok_or("Maximum editor panes reached")?;
        let source_active = self.pane_active(pane_id).map(str::to_string);
        let mut new_pane = CoderPane::new(new_id);
        if let Some(path) = source_active.clone() {
            new_pane.tabs.push(path.clone());
            new_pane.active = Some(path);
        }
        self.panes.push(new_pane);
        if !self.layout.split_leaf(pane_id, axis, new_id) {
            self.panes.retain(|p| p.id != new_id);
            return Err("Pane not found in layout");
        }
        self.focused_pane = new_id;
        Ok(new_id)
    }

    /// Close `pane_id`. Returns the pane that should keep focus, or `None` if the
    /// last pane was cleared instead of removed.
    pub fn close_pane(&mut self, pane_id: u8) -> Option<u8> {
        if matches!(self.layout, CoderLayout::Leaf(id) if id == pane_id) {
            // Last pane — clear tabs, keep the leaf.
            if let Some(pane) = self.pane_mut(pane_id) {
                pane.tabs.clear();
                pane.active = None;
            }
            self.focused_pane = pane_id;
            self.prune_orphan_open_files();
            return None;
        }
        let sibling = self.layout.remove_leaf(pane_id)?;
        self.panes.retain(|p| p.id != pane_id);
        self.focused_pane = sibling;
        self.prune_orphan_open_files();
        Some(sibling)
    }

    /// Drop `open_files` entries that no longer appear in any pane.
    pub fn prune_orphan_open_files(&mut self) {
        self.open_files.retain(|f| {
            self.panes
                .iter()
                .any(|p| p.tabs.iter().any(|t| t == &f.path))
        });
    }

    pub fn all_tab_paths(&self) -> impl Iterator<Item = &String> {
        self.panes.iter().flat_map(|p| p.tabs.iter())
    }
}

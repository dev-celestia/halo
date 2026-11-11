//! Recursive editor split-tree layout and open-file state.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderTreeEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    /// `None` until the directory is expanded at least once.
    pub children: Option<Vec<CoderTreeEntry>>,
}

/// How an open Coder tab is shown — text editor vs image preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoderFileKind {
    Text,
    Image,
}

/// Maximum nesting depth for editor splits (`0` = single leaf).
pub const CODER_MAX_SPLIT_DEPTH: u8 = 3;
/// Hard cap on leaf panes (`2^CODER_MAX_SPLIT_DEPTH`).
pub const CODER_MAX_PANES: usize = 8;

/// Axis for an editor split.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoderSplitAxis {
    /// Side-by-side panes.
    #[default]
    Horizontal,
    /// Stacked panes.
    Vertical,
}

/// One editor leaf — tab strip + active path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderPane {
    pub id: u8,
    pub tabs: Vec<String>,
    pub active: Option<String>,
}

impl CoderPane {
    pub fn new(id: u8) -> Self {
        Self {
            id,
            tabs: Vec::new(),
            active: None,
        }
    }
}

/// Recursive editor layout tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoderLayout {
    Leaf(u8),
    Split {
        axis: CoderSplitAxis,
        first: Box<CoderLayout>,
        second: Box<CoderLayout>,
    },
}

impl Default for CoderLayout {
    fn default() -> Self {
        Self::Leaf(0)
    }
}

impl CoderLayout {
    pub fn contains_leaf(&self, id: u8) -> bool {
        match self {
            Self::Leaf(leaf) => *leaf == id,
            Self::Split { first, second, .. } => {
                first.contains_leaf(id) || second.contains_leaf(id)
            }
        }
    }

    pub fn depth_of(&self, id: u8) -> Option<u8> {
        self.depth_of_inner(id, 0)
    }

    fn depth_of_inner(&self, id: u8, depth: u8) -> Option<u8> {
        match self {
            Self::Leaf(leaf) => (*leaf == id).then_some(depth),
            Self::Split { first, second, .. } => first
                .depth_of_inner(id, depth + 1)
                .or_else(|| second.depth_of_inner(id, depth + 1)),
        }
    }

    pub fn leaf_ids(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<u8>) {
        match self {
            Self::Leaf(id) => out.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    /// Replace `Leaf(old)` with a split whose first child is the old leaf and
    /// second is `Leaf(new_id)`.
    pub fn split_leaf(&mut self, old: u8, axis: CoderSplitAxis, new_id: u8) -> bool {
        match self {
            Self::Leaf(id) if *id == old => {
                *self = Self::Split {
                    axis,
                    first: Box::new(Self::Leaf(old)),
                    second: Box::new(Self::Leaf(new_id)),
                };
                true
            }
            Self::Split { first, second, .. } => {
                first.split_leaf(old, axis, new_id) || second.split_leaf(old, axis, new_id)
            }
            _ => false,
        }
    }

    /// Remove `Leaf(id)` and collapse its parent split into the sibling.
    /// Returns the sibling leaf/root id that should receive focus.
    pub fn remove_leaf(&mut self, id: u8) -> Option<u8> {
        match self {
            Self::Leaf(_) => None,
            Self::Split { first, second, .. } => {
                if matches!(first.as_ref(), Self::Leaf(leaf) if *leaf == id) {
                    let sibling = second.first_leaf();
                    *self = *second.clone();
                    return Some(sibling);
                }
                if matches!(second.as_ref(), Self::Leaf(leaf) if *leaf == id) {
                    let sibling = first.first_leaf();
                    *self = *first.clone();
                    return Some(sibling);
                }
                if let Some(focus) = first.remove_leaf(id) {
                    return Some(focus);
                }
                second.remove_leaf(id)
            }
        }
    }

    pub fn first_leaf(&self) -> u8 {
        match self {
            Self::Leaf(id) => *id,
            Self::Split { first, .. } => first.first_leaf(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderOpenFile {
    pub path: String,
    pub kind: CoderFileKind,
    /// Last loaded or saved text — used to detect dirty buffers.
    /// Empty for [`CoderFileKind::Image`] tabs.
    pub baseline: String,
    /// Current buffer (may include unsaved edits).
    /// Empty for [`CoderFileKind::Image`] tabs.
    pub content: String,
    pub dirty: bool,
    /// Image preview zoom percent (`100` = fit). Ignored for text tabs.
    pub zoom_pct: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoderDirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

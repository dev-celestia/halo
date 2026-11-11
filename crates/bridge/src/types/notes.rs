//! Notes view-state enums.

use serde::{Deserialize, Serialize};

pub use halo_backend::NoteRecord;

/// View layout mode for the notes markdown editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NotesViewMode {
    #[default]
    Edit,
    Split,
    Preview,
}

impl NotesViewMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Edit => "Editor",
            Self::Split => "Split",
            Self::Preview => "Preview",
        }
    }
}

/// Filter tab for the Saved Notes Library dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NotesFilterTab {
    #[default]
    All,
    Open,
    Closed,
}

impl NotesFilterTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Open => "Open in Tabs",
            Self::Closed => "Closed",
        }
    }

    pub const ALL: [Self; 3] = [Self::All, Self::Open, Self::Closed];
}

/// Sort options for the Saved Notes Library dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NotesSortOption {
    #[default]
    UpdatedDesc,
    UpdatedAsc,
    TitleAsc,
    TitleDesc,
    CreatedDesc,
}

impl NotesSortOption {
    pub fn label(self) -> &'static str {
        match self {
            Self::UpdatedDesc => "Recently Updated",
            Self::UpdatedAsc => "Oldest Updated",
            Self::TitleAsc => "Title (A-Z)",
            Self::TitleDesc => "Title (Z-A)",
            Self::CreatedDesc => "Recently Created",
        }
    }

    pub const ALL: [Self; 5] = [
        Self::UpdatedDesc,
        Self::UpdatedAsc,
        Self::TitleAsc,
        Self::TitleDesc,
        Self::CreatedDesc,
    ];
}

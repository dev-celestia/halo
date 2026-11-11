//! Settings, storage-info, and local-data-reset DTOs.

use serde::{Deserialize, Serialize};

/// Settings sidebar category (matches reference SettingsCategory).
///
/// The `Coder*` and `Agent` tabs are the IDE settings that used to live in a
/// separate Coder Settings window. They still read and write the shared
/// `CoderSettings` values persisted to `coder.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SettingsTab {
    #[default]
    General,
    Appearance,
    Keyboard,
    CoderEditor,
    CoderTerminal,
    CoderLanguageServers,
    CoderSourceControl,
    CoderWorkspace,
    Agent,
    CaCert,
    R2,
    Automation,
}

impl SettingsTab {
    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Appearance => "appearance",
            Self::Keyboard => "keyboard",
            Self::CoderEditor => "coder-editor",
            Self::CoderTerminal => "coder-terminal",
            Self::CoderLanguageServers => "coder-language-servers",
            Self::CoderSourceControl => "coder-source-control",
            Self::CoderWorkspace => "coder-workspace",
            Self::Agent => "agent",
            Self::CaCert => "ca-cert",
            Self::R2 => "r2",
            Self::Automation => "automation",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Keyboard => "Keyboard",
            Self::CoderEditor => "Editor",
            Self::CoderTerminal => "Terminal",
            Self::CoderLanguageServers => "Language Servers",
            Self::CoderSourceControl => "Source Control",
            Self::CoderWorkspace => "Workspace",
            Self::Agent => "Agent",
            Self::CaCert => "CA Certificate",
            Self::R2 => "R2 Storage",
            Self::Automation => "Runtime",
        }
    }

    /// True for the IDE tabs that edit the shared `CoderSettings` values.
    ///
    /// They share the "Reset to defaults" toolbar because a single reset clears
    /// all of them at once.
    pub fn is_coder(self) -> bool {
        matches!(
            self,
            Self::CoderEditor
                | Self::CoderTerminal
                | Self::CoderLanguageServers
                | Self::CoderSourceControl
                | Self::CoderWorkspace
                | Self::Agent
        )
    }
}

/// On-disk footprint snapshot for Settings → Storage.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StorageInfo {
    pub app_data_dir: String,
    pub database_path: String,
    pub crawler_artifacts_path: String,
    pub database_size_bytes: u64,
    pub crawler_artifacts_size_bytes: u64,
    pub regression_artifacts_size_bytes: u64,
    pub log_file_size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteArtifactResult {
    pub bytes_deleted: u64,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetLocalDataResult {
    pub artifact_dir: String,
    pub files_deleted: u64,
    pub bytes_deleted: u64,
    pub pages_updated: usize,
    pub intercept_browser_profile_removed: bool,
    pub ca_file_removed: bool,
}

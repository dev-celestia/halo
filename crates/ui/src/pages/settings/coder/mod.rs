//! IDE (Coder) sections of the global Settings page.
//!
//! Migrated from the standalone Coder Settings window: one module per section
//! — `editor`, `terminal`, `agent`, `language_servers`, `workspace`.
//! Source Control settings live in [`crate::pages::settings::source_control`].
//! Row and control helpers live one level up in
//! [`crate::pages::settings::widgets`].
//!
//! Everything reads and writes [`CoderSettings`](halo_bridge::CoderSettings) on
//! the shared bridge state and persists to `coder.json` on change, so every
//! window agrees. Tab selection is
//! [`SettingsTab`](halo_bridge::SettingsTab), owned by the settings page.

mod agent;
mod editor;
mod language_servers;
mod terminal;
mod workspace;

pub(super) use agent::render as render_agent_tab;
pub(super) use editor::render as render_editor_tab;
pub(super) use language_servers::render as render_language_servers_tab;
pub(super) use terminal::render as render_terminal_tab;
pub(super) use workspace::render as render_workspace_tab;

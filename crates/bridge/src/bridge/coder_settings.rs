//! Shared Coder (IDE) settings — one copy for every Coder tab.
//!
//! Edited from Settings → Coder / Agent, which may live in a satellite window
//! beside the editor. Every value sits on
//! [`BridgeState`](crate::BridgeState) so all windows agree.
//!
//! Values are written to `coder.json` on every change so they survive relaunch.

use super::util::*;
use super::*;
use crate::McpServerConfig;

impl Bridge {
    /// Mutate the IDE settings in place and persist them.
    ///
    /// No toast: these fire once per switch / stepper click and a toast per
    /// keystroke would bury the editor.
    pub fn update_coder_settings(cx: &mut App, f: impl FnOnce(&mut CoderSettings)) {
        let opencode_path_changed = Self::state(cx).update(cx, |s| {
            let before = s.coder_settings.opencode_executable_path.clone();
            f(&mut s.coder_settings);
            before != s.coder_settings.opencode_executable_path
        });
        Self::coder_persist_settings(cx);
        let _ = Self::halo_agent_refresh_catalogs(cx);
        // A different OpenCode binary may expose a different skill catalog.
        if opencode_path_changed {
            let _ = Self::halo_agent_refresh_skills(cx);
        }
    }

    /// Replace the MCP server list (Settings → Agent) and persist it.
    ///
    /// Removes chat selections for servers that no longer exist or are
    /// globally disabled so a stale name can never keep a server alive.
    pub fn set_mcp_servers(servers: Vec<McpServerConfig>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.coder_settings.mcp_servers = servers;
            let usable: Vec<String> = s
                .coder_settings
                .mcp_servers
                .iter()
                .filter(|server| server.enabled && server.is_configured())
                .map(|server| server.name.clone())
                .collect();
            s.coder
                .agent_enabled_mcp
                .retain(|name| usable.iter().any(|usable| usable == name));
            let snapshot = s.coder.agent_enabled_mcp.clone();
            let id = s.coder.agent_active_session_id.clone();
            if let Some(session) = s.coder.agent_sessions.iter_mut().find(|s| s.id == id) {
                session.mcp_servers = snapshot;
            }
            s.commit_active_tab();
        });
        Self::coder_persist_settings(cx);
    }

    /// Write the shell path typed into the Terminal section.
    ///
    /// Blank falls back to `$SHELL` so "clear the field" means "inherit".
    pub fn set_coder_shell(shell: &str, cx: &mut App) {
        let shell = shell.trim();
        let shell = if shell.is_empty() {
            CoderSettings::default().shell
        } else {
            shell.to_string()
        };
        Self::state(cx).update(cx, |s| {
            s.coder_settings.shell = shell.clone();
            s.apply_ok_msg(format!("Terminal shell set to {shell}"));
        });
        Self::coder_persist_settings(cx);
    }

    /// Write the directory new shells start in (Terminal section).
    ///
    /// Blank means "fall back to the open Coder folder, then `$HOME`", which is
    /// also the behaviour before this setting existed.
    pub fn set_terminal_default_cwd(dir: &str, cx: &mut App) {
        let dir = dir.trim().to_string();
        Self::state(cx).update(cx, |s| {
            s.coder_settings.default_cwd = dir.clone();
            s.apply_ok_msg(if dir.is_empty() {
                "New terminals start in the project folder or home".into()
            } else {
                format!("New terminals start in {dir}")
            });
        });
        Self::coder_persist_settings(cx);
    }

    /// Write the ignore globs typed into the Workspace section.
    ///
    /// The field is comma separated; state keeps one glob per line so the
    /// explorer can match them one at a time.
    pub fn set_coder_ignore_globs(globs: &str, cx: &mut App) {
        let normalized = globs
            .split([',', '\n'])
            .map(str::trim)
            .filter(|glob| !glob.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Self::state(cx).update(cx, |s| {
            s.coder_settings.ignore_globs = normalized;
            s.apply_ok_msg("Workspace ignore rules updated".into());
        });
        Self::coder_persist_settings(cx);
    }

    /// Turn the "ask before changing the system" gate on or off.
    ///
    /// On by default. Doctor's actions reach outside the app's own data
    /// directory (installs, ownership repairs, cache reclaims), so a click
    /// should say what it is about to do before it does it.
    pub fn set_confirm_system_changes(enabled: bool, cx: &mut App) {
        Self::update_coder_settings(cx, |settings| {
            settings.confirm_system_changes = enabled;
        });
    }

    /// Restore every IDE setting to its default and persist the reset.
    pub fn reset_coder_settings(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.coder_settings = CoderSettings::default();
            s.apply_ok_msg("Coder settings reset to defaults".into());
        });
        Self::coder_persist_settings(cx);
    }

    /// Write the current IDE settings into `coder.json`.
    pub(super) fn coder_persist_settings(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let snapshot = Self::state(cx).read(cx).coder_settings.clone();
        let persisted = coder_settings_to_persisted(&snapshot);
        let _ = backend.coder_save_settings(&persisted);
    }
}

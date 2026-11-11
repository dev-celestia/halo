//! Coder IDE: project Run / Build / Stop via the terminal PTY.

use super::util::*;
use super::*;
use crate::work::{CoderTerminal, CoderTerminalKind};
use uuid::Uuid;

const RUN_TAB_TITLE: &str = "Run";
const BUILD_TAB_TITLE: &str = "Build";

impl Bridge {
    /// Shell command the Run button will execute for the current project / focused file.
    pub fn coder_run_command_preview(cx: &App) -> Option<String> {
        let s = Self::state(cx).read(cx);
        if let Some(cmd) = override_cmd(s.coder.run_command_override.as_deref()) {
            return Some(cmd);
        }
        let root = s.coder.root_path.as_deref()?;
        let kind = s.coder.project_kind?;
        let focused = s.coder.focused_active_path();
        coder_run_shell_command(root, kind, focused)
    }

    /// Shell command the Build button will execute for the current project / focused file.
    pub fn coder_build_command_preview(cx: &App) -> Option<String> {
        let s = Self::state(cx).read(cx);
        if let Some(cmd) = override_cmd(s.coder.build_command_override.as_deref()) {
            return Some(cmd);
        }
        let root = s.coder.root_path.as_deref()?;
        let kind = s.coder.project_kind?;
        let focused = s.coder.focused_active_path();
        coder_build_shell_command(root, kind, focused)
    }

    /// `package.json` script names for the open project (empty when not Node / missing).
    pub fn coder_list_node_scripts(cx: &App) -> Vec<String> {
        let s = Self::state(cx).read(cx);
        let Some(root) = s.coder.root_path.as_deref() else {
            return Vec::new();
        };
        if !s.coder.project_kind.is_some_and(|k| k.has_node()) {
            return Vec::new();
        }
        coder_list_node_scripts(root)
    }

    /// Toggle the activity-rail Run/Build panel.
    pub fn coder_toggle_run_panel(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.run_panel_open = !s.coder.run_panel_open;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Open or close the Run/Build panel.
    pub fn coder_set_run_panel_open(open: bool, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.run_panel_open = open;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Set or clear the Run command override (`None` / blank → auto-detect).
    pub fn coder_set_run_command_override(
        command: Option<String>,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.run_command_override = normalize_override(command);
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Set or clear the Build command override (`None` / blank → auto-detect).
    pub fn coder_set_build_command_override(
        command: Option<String>,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.build_command_override = normalize_override(command);
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Pick a Node script for Run and store the full package-manager command.
    pub fn coder_pick_run_script(script: &str, cx: &mut App) -> BridgeResult<()> {
        let root = Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .clone()
            .ok_or_else(|| BridgeError::message("No project open"))?;
        let cmd = coder_node_script_command(&root, script);
        Self::coder_set_run_command_override(Some(cmd), cx)
    }

    /// Pick a Node script for Build and store the full package-manager command.
    pub fn coder_pick_build_script(script: &str, cx: &mut App) -> BridgeResult<()> {
        let root = Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .clone()
            .ok_or_else(|| BridgeError::message("No project open"))?;
        let cmd = coder_node_script_command(&root, script);
        Self::coder_set_build_command_override(Some(cmd), cx)
    }

    /// Restore auto-detected Run command (Rust `cargo run` or Node default script).
    pub fn coder_reset_run_command(cx: &mut App) -> BridgeResult<()> {
        Self::coder_set_run_command_override(None, cx)
    }

    /// Restore auto-detected Build command (Rust `cargo build` or Node `build`).
    pub fn coder_reset_build_command(cx: &mut App) -> BridgeResult<()> {
        Self::coder_set_build_command_override(None, cx)
    }

    /// Run the open project (`cargo run` or npm/pnpm/yarn script). Opens a dedicated Run tab.
    pub fn coder_run_project(cx: &mut App) -> BridgeResult<()> {
        let Some(cmd) = Self::coder_run_command_preview(cx) else {
            return Ok(());
        };
        if Self::state(cx).read(cx).coder.root_path.is_none() {
            return Ok(());
        }

        Self::state(cx).update(cx, |s| {
            s.coder.terminal_open = true;
            s.commit_active_tab();
        });

        let run_id = Self::coder_ensure_named_terminal(RUN_TAB_TITLE, cx)?;
        let line = format!("{cmd}\r");
        Self::coder_pty_write_bytes(&run_id, line.as_bytes(), cx)?;

        Self::state(cx).update(cx, |s| {
            s.coder.run_active = true;
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Build the open project (`cargo build` or npm/pnpm/yarn build). Opens a Build tab.
    ///
    /// Does not set `run_active` — rail Stop only tracks Play-started runs.
    pub fn coder_build_project(cx: &mut App) -> BridgeResult<()> {
        let Some(cmd) = Self::coder_build_command_preview(cx) else {
            return Ok(());
        };
        if Self::state(cx).read(cx).coder.root_path.is_none() {
            return Ok(());
        }

        Self::state(cx).update(cx, |s| {
            s.coder.terminal_open = true;
            s.commit_active_tab();
        });

        let build_id = Self::coder_ensure_named_terminal(BUILD_TAB_TITLE, cx)?;
        let line = format!("{cmd}\r");
        Self::coder_pty_write_bytes(&build_id, line.as_bytes(), cx)?;
        Ok(())
    }

    /// Stop the Play-started run (Ctrl+C on the Run tab) and clear `run_active`.
    pub fn coder_stop_run(cx: &mut App) -> BridgeResult<()> {
        if let Some(id) = Self::coder_find_terminal_id(RUN_TAB_TITLE, cx) {
            Self::state(cx).update(cx, |s| {
                s.coder.active_terminal_id = Some(id);
                s.coder.terminal_open = true;
                s.commit_active_tab();
            });
        }
        Self::coder_pty_interrupt(cx)?;
        Self::state(cx).update(cx, |s| {
            s.coder.run_active = false;
            s.commit_active_tab();
        });
        Ok(())
    }

    fn coder_find_terminal_id(title: &str, cx: &App) -> Option<String> {
        Self::state(cx)
            .read(cx)
            .coder
            .terminals
            .iter()
            .find(|t| t.title == title)
            .map(|t| t.id.clone())
    }

    /// Select an existing named tab or create one, then ensure its PTY is alive.
    fn coder_ensure_named_terminal(title: &str, cx: &mut App) -> BridgeResult<String> {
        let existing = Self::coder_find_terminal_id(title, cx);
        let id = if let Some(id) = existing {
            Self::state(cx).update(cx, |s| {
                s.coder.active_terminal_id = Some(id.clone());
                s.commit_active_tab();
            });
            id
        } else {
            Self::coder_terminal_push_named_tab(title, cx)
        };

        let alive = Self::state(cx).read(cx).terminal_sessions.alive(&id);
        if !alive {
            Self::coder_pty_start_session(&id, cx)?;
        }
        Ok(id)
    }

    fn coder_terminal_push_named_tab(title: &str, cx: &mut App) -> String {
        let id = Uuid::new_v4().to_string();
        let title = title.to_string();
        Self::state(cx).update(cx, |s| {
            s.coder.terminals.push(CoderTerminal {
                id: id.clone(),
                title,
                kind: CoderTerminalKind::Shell,
            });
            s.coder.active_terminal_id = Some(id.clone());
            s.commit_active_tab();
        });
        id
    }
}

fn override_cmd(raw: Option<&str>) -> Option<String> {
    let cmd = raw?.trim();
    if cmd.is_empty() {
        None
    } else {
        Some(cmd.to_string())
    }
}

fn normalize_override(command: Option<String>) -> Option<String> {
    command.and_then(|c| {
        let t = c.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

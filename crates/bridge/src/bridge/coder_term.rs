//! Coder IDE: multi-tab terminal dock and PTY lifecycle.
//!
//! The dock is a thin layout layer over the shared terminal session API in
//! [`super::terminal`]: runtime bytes live in `BridgeState::terminal_sessions`,
//! not in the work-tab projection.

use super::*;
use crate::work::{CoderTerminal, CoderTerminalKind};
use uuid::Uuid;

/// Fixed id for the read-only history tab — reopening it focuses the existing
/// tab instead of stacking duplicates.
const GIT_LOG_TAB_ID: &str = "coder-git-log";

impl Bridge {
    /// Toggle the bottom terminal dock. Opening ensures at least one tab + PTY.
    pub fn coder_toggle_terminal(cx: &mut App) -> BridgeResult<()> {
        let opening = Self::state(cx).update(cx, |s| {
            s.coder.terminal_open = !s.coder.terminal_open;
            let open = s.coder.terminal_open;
            s.commit_active_tab();
            open
        });
        if opening {
            Self::coder_terminal_ensure_one(cx)?;
        }
        Ok(())
    }

    /// Create a new terminal tab and start its PTY.
    pub fn coder_terminal_new(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.terminal_open = true;
            s.commit_active_tab();
        });
        let id = Self::coder_terminal_push_tab(cx);
        Self::coder_pty_start_session(&id, cx)?;
        Ok(())
    }

    /// Select an existing terminal tab.
    ///
    /// Selecting the Git Log tab reloads its text, so coming back to the tab
    /// never shows a history that predates commits made elsewhere.
    pub fn coder_terminal_select(id: &str, cx: &mut App) -> BridgeResult<()> {
        let is_log = Self::state(cx)
            .read(cx)
            .coder
            .terminals
            .iter()
            .any(|t| t.id == id && t.kind == CoderTerminalKind::GitLog);
        Self::state(cx).update(cx, |s| {
            if s.coder.terminals.iter().any(|t| t.id == id) {
                s.coder.active_terminal_id = Some(id.to_string());
                s.coder.terminal_open = true;
                s.commit_active_tab();
            }
        });
        if is_log {
            Self::git_log_refresh(cx)?;
        }
        Ok(())
    }

    /// Close a terminal tab and kill its PTY. Closing the last tab leaves the dock open.
    pub fn coder_terminal_close(id: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let _ = backend.coder_pty_kill(id);

        Self::state(cx).update(cx, |s| {
            s.terminal_sessions.remove(id);
            let was_active = s.coder.active_terminal_id.as_deref() == Some(id);
            let idx = s.coder.terminals.iter().position(|t| t.id == id);
            if let Some(idx) = idx {
                s.coder.terminals.remove(idx);
                if was_active {
                    s.coder.active_terminal_id = if idx > 0 {
                        s.coder.terminals.get(idx - 1).map(|t| t.id.clone())
                    } else {
                        s.coder.terminals.first().map(|t| t.id.clone())
                    };
                }
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Ensure the dock has an active terminal (creates one if needed).
    pub fn coder_terminal_ensure_one(cx: &mut App) -> BridgeResult<()> {
        let needs_new = Self::state(cx).read(cx).coder.terminals.is_empty();
        if needs_new {
            let id = Self::coder_terminal_push_tab(cx);
            Self::coder_pty_start_session(&id, cx)?;
        } else {
            let id = Self::state(cx)
                .read(cx)
                .coder
                .active_terminal_id
                .clone()
                .or_else(|| {
                    Self::state(cx)
                        .read(cx)
                        .coder
                        .terminals
                        .first()
                        .map(|t| t.id.clone())
                });
            if let Some(id) = id {
                Self::state(cx).update(cx, |s| {
                    s.coder.active_terminal_id = Some(id.clone());
                    s.commit_active_tab();
                });
                let alive = Self::state(cx).read(cx).terminal_sessions.alive(&id);
                if !alive {
                    Self::coder_pty_start_session(&id, cx)?;
                }
            }
        }
        Ok(())
    }

    /// Open (or focus) the read-only Git Log tab and load its history.
    ///
    /// The tab has no PTY behind it: it is a static view that the toolbar can
    /// reload, which is why it is safe to open before a shell exists.
    pub fn coder_terminal_open_git_log(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.terminal_open = true;
            if !s
                .coder
                .terminals
                .iter()
                .any(|t| t.kind == CoderTerminalKind::GitLog)
            {
                s.coder.terminals.push(CoderTerminal {
                    id: GIT_LOG_TAB_ID.into(),
                    title: "Git Log".into(),
                    kind: CoderTerminalKind::GitLog,
                });
            }
            s.coder.active_terminal_id = Some(GIT_LOG_TAB_ID.into());
            s.commit_active_tab();
        });
        Self::git_log_refresh(cx)
    }

    /// Reload the Git Log tab's text from the workspace root.
    ///
    /// Failures are written into the pane instead of the toast strip: the view
    /// is read-only, so the message is the only useful place to explain why it
    /// is empty.
    pub fn git_log_refresh(cx: &mut App) -> BridgeResult<()> {
        let root = Self::state(cx).read(cx).coder.root_path.clone();
        let text = match root {
            Some(root) => match Self::backend(cx) {
                Ok(backend) => backend
                    .git_log(&root)
                    .unwrap_or_else(|err| format!("Could not read git log:\n{err}")),
                Err(err) => format!("Could not read git log:\n{err}"),
            },
            None => "Open a folder to see its git history.".to_string(),
        };
        Self::state(cx).update(cx, |s| {
            s.git_log = Some(text);
            s.commit_active_tab();
        });
        Ok(())
    }

    fn coder_terminal_push_tab(cx: &mut App) -> String {
        let id = Uuid::new_v4().to_string();
        Self::state(cx).update(cx, |s| {
            s.coder.terminal_seq = s.coder.terminal_seq.saturating_add(1);
            let title = format!("Terminal {}", s.coder.terminal_seq);
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

    pub(super) fn coder_pty_start_session(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        let root = Self::state(cx).read(cx).coder.root_path.clone();
        let Some(root) = root else {
            // No folder → no shell. The hint goes through the store like any
            // other shell output so the surface paints and reports "stopped".
            Self::state(cx).update(cx, |s| {
                if !s.terminal_sessions.has(session_id) {
                    s.terminal_sessions
                        .push(session_id, "Open a folder to use the terminal.\r\n");
                    s.terminal_sessions.mark_dead(session_id);
                }
                s.note_shell();
            });
            return Ok(());
        };
        Self::terminal_start(session_id, Some(&root), cx)
    }

    /// Restart the active terminal's PTY.
    pub fn coder_pty_restart(cx: &mut App) -> BridgeResult<()> {
        let Some(id) = Self::state(cx).read(cx).coder.active_terminal_id.clone() else {
            return Self::coder_terminal_ensure_one(cx);
        };
        // The dock has no cwd fallback: without an open folder, restarting keeps
        // showing the "open a folder" hint instead of spawning in `$HOME`.
        if Self::state(cx).read(cx).coder.root_path.is_none() {
            if let Ok(backend) = Self::backend(cx) {
                let _ = backend.coder_pty_kill(&id);
            }
            Self::state(cx).update(cx, |s| {
                s.terminal_sessions.reset(&id);
                s.note_shell();
            });
            return Self::coder_pty_start_session(&id, cx);
        }
        Self::terminal_restart(&id, cx)
    }

    /// Kill the active terminal's PTY without closing the tab.
    pub fn coder_pty_kill(cx: &mut App) -> BridgeResult<()> {
        let Some(id) = Self::state(cx).read(cx).coder.active_terminal_id.clone() else {
            return Ok(());
        };
        Self::terminal_kill(&id, cx)
    }

    /// Kill every active terminal PTY process on app shutdown.
    pub fn coder_pty_kill_all(cx: &mut App) {
        Self::terminal_kill_all(cx);
    }

    /// Clear the active terminal's screen.
    ///
    /// A live shell is asked to clear itself (Ctrl+L) rather than having the UI
    /// throw its `Term` away: resetting the local grid leaves the shell with its
    /// own idea of where the cursor and the prompt are, so the next relative
    /// redraw lands in the wrong place (and TUIs are garbage immediately).
    /// Only a dead session falls back to the local epoch reset.
    pub fn coder_pty_clear(cx: &mut App) -> BridgeResult<()> {
        let Some(id) = Self::state(cx).read(cx).coder.active_terminal_id.clone() else {
            return Ok(());
        };
        Self::terminal_clear(&id, cx)
    }

    /// Write raw bytes to one PTY (character-mode input).
    ///
    /// The session is passed explicitly rather than resolved from
    /// `active_terminal_id`: the UI already knows which tab it is drawing, and
    /// input must never create a tab or spawn a shell as a side effect.
    pub fn coder_pty_write_bytes(session_id: &str, data: &[u8], cx: &mut App) -> BridgeResult<()> {
        Self::terminal_write(session_id, data, cx)
    }

    /// Resize the active PTY (and any matching session by id).
    pub fn coder_pty_resize(
        session_id: &str,
        rows: u16,
        cols: u16,
        cx: &mut App,
    ) -> BridgeResult<()> {
        Self::terminal_resize(session_id, rows, cols, cx)
    }

    /// Send Ctrl+C (`\\x03`) to the active PTY.
    pub fn coder_pty_interrupt(cx: &mut App) -> BridgeResult<()> {
        let Some(id) = Self::state(cx).read(cx).coder.active_terminal_id.clone() else {
            return Ok(());
        };
        Self::terminal_interrupt(&id, cx)
    }
}

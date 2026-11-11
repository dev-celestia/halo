//! Terminal sessions: PTY lifecycle behind a stable, work-tab-independent id.
//!
//! Every shell — Coder dock tabs, Terminal app panes — is a *session* in
//! [`crate::TerminalStore`], keyed by an opaque string. Layout lives in the work
//! tab; runtime bytes live here. That split is what lets output keep flowing
//! while a tab is inactive and lets two surfaces read the same session without
//! racing over a destructive drain.

use super::*;

/// Default PTY size until the surface measures itself and calls resize.
const DEFAULT_PTY_ROWS: u16 = 24;
const DEFAULT_PTY_COLS: u16 = 120;

impl Bridge {
    /// Spawn a shell for `session_id`.
    ///
    /// Working directory: explicit `cwd` → open Coder folder → `$HOME`.
    pub fn terminal_start(session_id: &str, cwd: Option<&str>, cx: &mut App) -> BridgeResult<()> {
        if session_id.trim().is_empty() {
            return Ok(());
        }
        let (dir, shell) = {
            let state = Self::state(cx).read(cx);
            (terminal_cwd(state, cwd), state.coder_settings.shell.clone())
        };
        let shell = shell.trim();
        let shell = if shell.is_empty() { None } else { Some(shell) };
        let backend = Self::backend(cx)?;
        let generation =
            backend.coder_pty_start(session_id, &dir, shell, DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS)?;
        Self::state(cx).update(cx, |s| {
            s.terminal_sessions.create(session_id, &dir, generation);
            // Runtime-only change: nothing in the work tab moved, but the
            // surface still has to repaint (and re-read from cursor 0).
            s.note_shell();
        });
        Ok(())
    }

    /// Write raw bytes (key input, paste) — never spawns anything as a side
    /// effect.
    pub fn terminal_write(session_id: &str, data: &[u8], cx: &mut App) -> BridgeResult<()> {
        if session_id.is_empty() || data.is_empty() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        backend.coder_pty_write(session_id, data)?;
        Ok(())
    }

    /// Resize the PTY. `rows`/`cols` follow the surface's cell grid.
    pub fn terminal_resize(
        session_id: &str,
        rows: u16,
        cols: u16,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if session_id.is_empty() || rows == 0 || cols == 0 {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        // A not-yet-spawned session has no PTY to resize; that is not an error.
        let _ = backend.coder_pty_resize(session_id, rows, cols);
        Ok(())
    }

    /// Send Ctrl+C to a live session.
    pub fn terminal_interrupt(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        let alive = Self::state(cx).read(cx).terminal_sessions.alive(session_id);
        if !alive {
            return Ok(());
        }
        Self::terminal_write(session_id, &[0x03], cx)
    }

    /// Kill the process and start a fresh shell in the same directory.
    pub fn terminal_restart(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        if session_id.is_empty() {
            return Ok(());
        }
        let cwd = Self::state(cx)
            .read(cx)
            .terminal_sessions
            .meta(session_id)
            .map(|meta| meta.cwd)
            .filter(|cwd| !cwd.is_empty());
        if let Ok(backend) = Self::backend(cx) {
            let _ = backend.coder_pty_kill(session_id);
        }
        Self::state(cx).update(cx, |s| {
            s.terminal_sessions.reset(session_id);
            s.note_shell();
        });
        Self::terminal_start(session_id, cwd.as_deref(), cx)
    }

    /// Clear the screen.
    ///
    /// A live shell is asked to clear itself (Ctrl+L): throwing away the local
    /// grid leaves the shell's own cursor/prompt idea stale, and the next
    /// relative redraw lands in the wrong place. A dead session falls back to a
    /// local epoch reset.
    pub fn terminal_clear(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        if session_id.is_empty() {
            return Ok(());
        }
        let alive = Self::state(cx).read(cx).terminal_sessions.alive(session_id);
        if alive {
            return Self::terminal_write(session_id, &[0x0c], cx);
        }
        Self::state(cx).update(cx, |s| {
            s.terminal_sessions.reset(session_id);
            s.note_shell();
        });
        Ok(())
    }

    /// Kill the process but keep the session (and its output) around.
    pub fn terminal_kill(session_id: &str, cx: &mut App) -> BridgeResult<()> {
        if session_id.is_empty() {
            return Ok(());
        }
        if let Ok(backend) = Self::backend(cx) {
            let _ = backend.coder_pty_kill(session_id);
        }
        Self::state(cx).update(cx, |s| {
            s.terminal_sessions.mark_dead(session_id);
            s.note_shell();
        });
        Ok(())
    }

    /// Kill every PTY this process owns — store sessions plus any legacy Coder
    /// tab that predates the store.
    pub fn terminal_kill_all(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let ids: Vec<String> = {
            let state = Self::state(cx);
            let state = state.read(cx);
            let mut ids = state.terminal_sessions.ids();
            for term in &state.coder.terminals {
                if !ids.contains(&term.id) {
                    ids.push(term.id.clone());
                }
            }
            ids
        };
        for id in ids {
            let _ = backend.coder_pty_kill(&id);
        }
    }

    /// Non-consuming read of a session from `cursor`.
    pub fn terminal_read(session_id: &str, cursor: u64, cx: &App) -> Option<crate::TerminalRead> {
        Self::state(cx)
            .read(cx)
            .terminal_sessions
            .read_from(session_id, cursor)
    }

    /// Header-only view (alive / epoch / cwd / backlog).
    pub fn terminal_meta(session_id: &str, cx: &App) -> Option<crate::TerminalMeta> {
        Self::state(cx).read(cx).terminal_sessions.meta(session_id)
    }

    /// Every session id the store knows about.
    pub fn terminal_ids(cx: &App) -> Vec<String> {
        Self::state(cx).read(cx).terminal_sessions.ids()
    }
}

/// Working directory for a new session: explicit → open Coder folder →
/// Settings default → home.
fn terminal_cwd(state: &BridgeState, explicit: Option<&str>) -> String {
    if let Some(dir) = explicit.map(str::trim).filter(|dir| !dir.is_empty()) {
        return dir.to_string();
    }
    if let Some(root) = state
        .coder
        .root_path
        .as_deref()
        .map(str::trim)
        .filter(|root| !root.is_empty())
    {
        return root.to_string();
    }
    if let Some(dir) = state.coder_settings.effective_cwd() {
        return dir.to_string();
    }
    dirs::home_dir()
        .map(|home| home.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string())
}

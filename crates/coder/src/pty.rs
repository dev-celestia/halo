//! Interactive PTY sessions for the Coder IDE terminal panel (multi-tab).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::thread;

use parking_lot::Mutex;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use serde::Serialize;

use halo_core::tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize)]
struct PtyOutPayload {
    session_id: String,
    data: String,
}

#[derive(Serialize)]
struct PtyExitPayload {
    session_id: String,
    generation: u64,
}

struct PtySession {
    generation: u64,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send>,
    writer: Box<dyn Write + Send>,
}

/// Managed on [`AppHandle`] — keyed by UI terminal tab id.
#[derive(Default)]
pub struct CoderPtyState {
    next_generation: Mutex<u64>,
    sessions: Mutex<HashMap<String, PtySession>>,
}

/// Take the decodable prefix out of `buf`, leaving any incomplete UTF-8 tail
/// behind for the next read.
///
/// PTY reads are byte chunks, so a multi-byte codepoint can straddle two reads.
/// Decoding each chunk on its own turns that codepoint into two replacement
/// characters — which is what CJK / emoji build output used to look like. The
/// leftover fragment is kept here instead.
///
/// Returns `None` while the buffer holds nothing but an incomplete sequence.
fn take_utf8(buf: &mut Vec<u8>) -> Option<String> {
    if buf.is_empty() {
        return None;
    }
    let end = match std::str::from_utf8(buf) {
        Ok(_) => buf.len(),
        // `error_len()` is `None` only for a truncated tail: stop before it and
        // wait for the rest. A `Some(len)` is a genuinely invalid sequence —
        // consume it so a corrupt stream cannot stall the reader.
        Err(err) => err.valid_up_to() + err.error_len().unwrap_or(0),
    };
    if end == 0 {
        return None;
    }
    let data = String::from_utf8_lossy(&buf[..end]).into_owned();
    buf.drain(0..end);
    Some(data)
}

fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())
    }
}

/// Ensure managed state exists (idempotent).
pub fn ensure_managed(app: &AppHandle) {
    if app.try_state::<CoderPtyState>().is_none() {
        app.manage(CoderPtyState::default());
    }
}

/// Kill any existing session for `session_id`, then spawn a shell in `cwd`.
///
/// `shell` — when `Some` and non-empty, used as the program path; otherwise
/// [`default_shell`]. `rows`/`cols` — initial PTY size (clamped to at least 1).
///
/// Returns the new session `generation`. Readers stamp their exit event with
/// it so a restart cannot be marked dead by the previous process's exit.
pub fn start(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    shell: Option<&str>,
    rows: u16,
    cols: u16,
) -> Result<u64, String> {
    ensure_managed(app);
    let path = Path::new(cwd);
    if !path.is_dir() {
        return Err(format!("Not a directory: {cwd}"));
    }
    if session_id.trim().is_empty() {
        return Err("Terminal session id is required".into());
    }

    kill(app, session_id);

    let rows = rows.max(1);
    let cols = cols.max(1);

    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {e}"))?;

    let shell = shell
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(default_shell);
    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(cwd);
    #[cfg(not(windows))]
    {
        // Advertise a full-color terminal so shells / CLIs emit ANSI + truecolor.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("CLICOLOR", "1");
        cmd.env("CLICOLOR_FORCE", "1");

        // Apps launched from Finder/Dock inherit a bare environment: no UTF-8
        // locale (oh-my-zsh prompts then fail with "character not in range")
        // and no login-shell PATH (Homebrew / /usr/local/bin, so tools like
        // pnpm look missing). Match what Terminal.app gives the user.
        for (key, value) in [("LANG", "en_US.UTF-8"), ("LC_CTYPE", "en_US.UTF-8")] {
            let current = std::env::var(key).unwrap_or_default();
            if current.is_empty() || !current.to_ascii_lowercase().contains("utf-8") {
                cmd.env(key, value);
            }
        }
        if let Ok(lc_all) = std::env::var("LC_ALL") {
            if !lc_all.is_empty() && !lc_all.to_ascii_lowercase().contains("utf-8") {
                cmd.env("LC_ALL", "en_US.UTF-8");
            }
        }

        if shell.ends_with("zsh") || shell.ends_with("bash") || shell.ends_with("sh") {
            // Login + interactive: read /etc/zprofile and ~/.zprofile, which is
            // where the system PATH (path_helper, Homebrew) is assembled.
            cmd.arg("-l");
            cmd.arg("-i");
        }
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn shell ({shell}): {e}"))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to take PTY writer: {e}"))?;

    let state = app.state::<CoderPtyState>();
    let generation = {
        let mut next = state.next_generation.lock();
        *next = next.wrapping_add(1);
        *next
    };
    state.sessions.lock().insert(
        session_id.to_string(),
        PtySession {
            generation,
            master: pair.master,
            child,
            writer,
        },
    );

    let emit_app = app.clone();
    let sid = session_id.to_string();
    thread::Builder::new()
        .name(format!("coder-pty-reader-{sid}"))
        .spawn(move || {
            let mut buf = [0u8; 4096];
            // Holds at most the 3 bytes of a codepoint split across two reads.
            let mut carry: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        // Keep raw bytes (as lossless UTF-8) so CSI / CR survive
                        // for the VT parser; only the split tail is deferred.
                        carry.extend_from_slice(&buf[..n]);
                        while let Some(data) = take_utf8(&mut carry) {
                            let _ = emit_app.emit(
                                "coder-pty-out",
                                &PtyOutPayload {
                                    session_id: sid.clone(),
                                    data,
                                },
                            );
                        }
                    }
                    Err(_) => break,
                }
            }
            if let Some(data) = take_utf8(&mut carry) {
                let _ = emit_app.emit(
                    "coder-pty-out",
                    &PtyOutPayload {
                        session_id: sid.clone(),
                        data,
                    },
                );
            }
            let _ = emit_app.emit(
                "coder-pty-exit",
                &PtyExitPayload {
                    session_id: sid.clone(),
                    generation,
                },
            );
            if let Some(state) = emit_app.try_state::<CoderPtyState>() {
                let mut guard = state.sessions.lock();
                if guard.get(&sid).map(|s| s.generation) == Some(generation) {
                    guard.remove(&sid);
                }
            }
        })
        .map_err(|e| format!("Failed to spawn PTY reader: {e}"))?;

    Ok(generation)
}

/// Write raw bytes to a session's PTY.
pub fn write(app: &AppHandle, session_id: &str, data: &[u8]) -> Result<(), String> {
    ensure_managed(app);
    let state = app.state::<CoderPtyState>();
    let mut guard = state.sessions.lock();
    let Some(session) = guard.get_mut(session_id) else {
        return Err("No terminal session".into());
    };
    session
        .writer
        .write_all(data)
        .map_err(|e| format!("Failed to write to PTY: {e}"))?;
    session
        .writer
        .flush()
        .map_err(|e| format!("Failed to flush PTY: {e}"))?;
    Ok(())
}

/// Resize a session's PTY window (rows × cols). No-op if the session is gone.
pub fn resize(app: &AppHandle, session_id: &str, rows: u16, cols: u16) -> Result<(), String> {
    ensure_managed(app);
    let state = app.state::<CoderPtyState>();
    let mut guard = state.sessions.lock();
    let Some(session) = guard.get_mut(session_id) else {
        return Err("No terminal session".into());
    };
    let rows = rows.max(1);
    let cols = cols.max(1);
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {e}"))?;
    Ok(())
}

/// Kill one session if present.
pub fn kill(app: &AppHandle, session_id: &str) {
    ensure_managed(app);
    let Some(state) = app.try_state::<CoderPtyState>() else {
        return;
    };
    if let Some(mut session) = state.sessions.lock().remove(session_id) {
        let _ = session.child.kill();
    }
}

/// Kill every active session.
pub fn kill_all(app: &AppHandle) {
    ensure_managed(app);
    let Some(state) = app.try_state::<CoderPtyState>() else {
        return;
    };
    let mut guard = state.sessions.lock();
    for (_, mut session) in guard.drain() {
        let _ = session.child.kill();
    }
}

/// Whether a specific session is currently held.
pub fn is_alive(app: &AppHandle, session_id: &str) -> bool {
    app.try_state::<CoderPtyState>()
        .map(|s| s.sessions.lock().contains_key(session_id))
        .unwrap_or(false)
}

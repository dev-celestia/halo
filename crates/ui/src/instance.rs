//! Single-instance guard.
//!
//! A dev run is a bare executable: no `CFBundleIdentifier`, no
//! `LSMultipleInstancesProhibited`, so macOS hands every process its own Dock
//! tile. Two `cargo run`s — or one watcher whose rebuild orphaned the previous
//! app — therefore show the Halo ring twice, and the second window is usually
//! invisible behind the first.
//!
//! The first process to start claims a Unix socket in the user cache dir;
//! later ones find the claim, wait briefly (a rebuild kills the old process and
//! the socket is released a moment later), then give up. The socket is keyed on
//! the canonical executable path, so a dev build, a bundled `.app`, and a second
//! checkout never block each other.
//!
//! `HALO_MOCKUP` opts out: the design-review surface is meant to sit next to the
//! running app.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long a second instance waits for the previous one to release the claim.
const HANDOFF: Duration = Duration::from_secs(2);

/// Holds the listener (and the bound socket) for the lifetime of the process.
/// Statics are never dropped, which is exactly the semantics we want.
static GUARD: OnceLock<UnixListener> = OnceLock::new();

/// `true` when this process owns the instance lock and should open its window,
/// `false` when another instance of the same executable is already running.
pub fn ensure_single() -> bool {
    if std::env::var_os("HALO_MOCKUP").is_some() {
        return true;
    }
    let Some(path) = socket_path() else {
        // No HOME / no cache dir: fail open rather than refusing to launch.
        return true;
    };

    let is_hot_reload =
        std::env::var_os("HALO_HOT_RELOAD").is_some_and(|v| v != "0" && !v.is_empty());
    let deadline = Instant::now() + HANDOFF;
    loop {
        if let Ok(listener) = UnixListener::bind(&path) {
            let _ = GUARD.set(listener);
            return true;
        }
        // A refused connection means nobody is listening: the file is a
        // leftover from a process that died without unlinking it.
        match UnixStream::connect(&path) {
            Ok(mut stream) => {
                // If this is a normal launch (e.g. user clicked Halo in Applications/Spotlight
                // while an instance is already running), ask the running instance to activate
                // and open/focus its window, then exit immediately.
                if !is_hot_reload {
                    let _ = stream.write_all(b"activate\n");
                    let _ = stream.flush();
                    return false;
                }
            }
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Listen for activation requests from secondary launches.
pub fn listen_for_activations(cx: &mut gpui::App) {
    let Some(listener) = GUARD.get() else {
        return;
    };
    let Ok(listener_clone) = listener.try_clone() else {
        return;
    };
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("halo-instance-listener".into())
            .spawn(move || {
                for stream in listener_clone.incoming() {
                    let Ok(stream) = stream else { continue };
                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_ok() && line.trim() == "activate" {
                        let _ = tx.send(());
                    }
                }
            })
            .ok();

        loop {
            executor.timer(Duration::from_millis(100)).await;
            if rx.try_recv().is_ok() {
                let _ = cx.update(|cx| {
                    crate::windows::ensure_or_focus_main_window(cx);
                });
            }
        }
    })
    .detach();
}

/// Clean up the single-instance socket file on shutdown.
pub fn cleanup() {
    if let Some(path) = socket_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// One socket per executable, named by an FNV-1a hash of its canonical path.
fn socket_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in exe.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    let is_dir_writable = |d: &std::path::Path| -> bool {
        if std::fs::create_dir_all(d).is_err() {
            return false;
        }
        let probe = d.join(format!(".write-probe-{}", std::process::id()));
        if std::fs::write(&probe, b"1").is_err() {
            return false;
        }
        let _ = std::fs::remove_file(&probe);
        true
    };

    // 1. Preferred user cache directory
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        #[cfg(target_os = "macos")]
        let primary = home.join("Library/Caches/Halo/run");
        #[cfg(not(target_os = "macos"))]
        let primary = home.join(".cache/halo/run");

        if is_dir_writable(&primary) {
            return Some(primary.join(format!("{hash:016x}.sock")));
        }
    }

    // 2. Fallback user temp directory (resilient against permission issues in user cache)
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".into());
    let fallback = std::env::temp_dir().join(format!("halo-run-{user}"));
    if is_dir_writable(&fallback) {
        return Some(fallback.join(format!("{hash:016x}.sock")));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path_computes() {
        let p = socket_path();
        assert!(p.is_some());
        let path = p.unwrap();
        assert!(path.to_string_lossy().ends_with(".sock"));
    }
}

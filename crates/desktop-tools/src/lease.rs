//! One chat owns the desktop at a time.
//!
//! Not a correctness lock: the engine already gives each session its own
//! refstore and liveness directory, so two sessions cannot corrupt each other's
//! refs. What they *can* do is fight over the user's attention — stealing focus,
//! moving the pointer, scrolling a window out from under someone. The lease
//! makes that an explicit refusal instead of a race.
//!
//! The lease records the `session_id` because a session-owned ref only resolves
//! under the same `--session` scope: losing the id loses every ref the agent
//! holds.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ToolResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub chat_id: String,
    pub session_id: String,
    pub pid: u32,
    pub since: u64,
    /// Last app this chat snapshotted. Absent on leases written before the field.
    #[serde(default)]
    pub app: Option<String>,
}

/// Take the lease, or explain who has it.
///
/// Re-entrant for the same chat: a second command in one run must not evict its
/// own run. A holder whose process is gone is reclaimed, but only after checking
/// the pid — never on a timer alone, because a slow turn is not a dead one.
/// `pid` is the agent run, not this CLI process. A `halo-desktop` command
/// exits immediately, so recording its own pid would free the desktop before
/// the turn's next command.
pub fn acquire(path: &Path, chat_id: &str, session_id: &str, pid: u32) -> ToolResult<Lease> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }

    for _ in 0..2 {
        match create_exclusive(path, chat_id, session_id, pid) {
            Ok(lease) => return Ok(lease),
            Err(Acquire::Held(holder)) => {
                if holder.chat_id == chat_id {
                    return Ok(holder);
                }
                // A holder with no identity is an unreadable lease (a crashed
                // writer); a holder whose pid is gone is a dead run. Both are
                // reclaimed, and only after the check.
                let unreadable = holder.chat_id.is_empty() && holder.pid == 0;
                if unreadable || (holder.pid != 0 && !pid_alive(holder.pid)) {
                    let _ = std::fs::remove_file(path);
                    continue;
                }
                return Err(format!(
                    "DESKTOP_BUSY: the desktop is held by chat {} (pid {}, since {}). \
                     Stop that run, or wait for it to finish.",
                    holder.chat_id, holder.pid, holder.since
                ));
            }
            Err(Acquire::Failed(error)) => return Err(error),
        }
    }

    Err("DESKTOP_BUSY: the lease kept changing hands; try again".to_string())
}

/// Remember the app this chat is driving, when it already holds the lease.
pub fn note_app(path: &Path, chat_id: &str, app: &str) {
    let app = app.trim();
    if app.is_empty() {
        return;
    }
    let Some(mut holder) = current(path) else {
        return;
    };
    if holder.chat_id != chat_id || holder.app.as_deref() == Some(app) {
        return;
    }
    holder.app = Some(app.to_string());
    if let Ok(encoded) = serde_json::to_string(&holder) {
        let _ = std::fs::write(path, encoded);
    }
}

/// Release only if this chat owns it. Returns whether anything was released.
pub fn release(path: &Path, chat_id: &str) -> ToolResult<bool> {
    match current(path) {
        Some(holder) if holder.chat_id == chat_id => {
            std::fs::remove_file(path)
                .map_err(|error| format!("could not release the lease: {error}"))?;
            Ok(true)
        }
        Some(_) => Ok(false),
        None => Ok(false),
    }
}

pub fn current(path: &Path) -> Option<Lease> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn path_for(cache_dir: &Path) -> PathBuf {
    cache_dir.join("lease.json")
}

enum Acquire {
    Held(Lease),
    Failed(String),
}

fn create_exclusive(
    path: &Path,
    chat_id: &str,
    session_id: &str,
    pid: u32,
) -> Result<Lease, Acquire> {
    let lease = Lease {
        chat_id: chat_id.to_string(),
        session_id: session_id.to_string(),
        pid,
        since: now_secs(),
        app: None,
    };

    // `create_new` is the whole point: two processes cannot both win, even
    // without an advisory lock.
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            let encoded = serde_json::to_string(&lease)
                .map_err(|error| Acquire::Failed(format!("could not encode the lease: {error}")))?;
            file.write_all(encoded.as_bytes())
                .map_err(|error| Acquire::Failed(format!("could not write the lease: {error}")))?;
            Ok(lease)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // An unreadable lease is a crashed writer, not a live holder.
            match current(path) {
                Some(holder) => Err(Acquire::Held(holder)),
                None => {
                    let _ = std::fs::remove_file(path);
                    Err(Acquire::Held(Lease {
                        chat_id: String::new(),
                        session_id: String::new(),
                        pid: 0,
                        since: 0,
                        app: None,
                    }))
                }
            }
        }
        Err(error) => Err(Acquire::Failed(format!(
            "could not create {}: {error}",
            path.display()
        ))),
    }
}

/// `kill -0` answers "does this pid exist and may I signal it" without a signal.
/// A failure to run it at all is treated as alive: fail closed.
fn pid_alive(pid: u32) -> bool {
    match std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => true,
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_chat_is_refused_with_the_holder_named() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_for(dir.path());

        acquire(&path, "chat-a", "run-1", std::process::id()).expect("first lease");
        let error = acquire(&path, "chat-b", "run-2", std::process::id())
            .expect_err("second must be refused");
        assert!(error.starts_with("DESKTOP_BUSY"), "{error}");
        assert!(error.contains("chat-a"), "{error}");
    }

    #[test]
    fn the_same_chat_is_re_entrant() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_for(dir.path());

        let first = acquire(&path, "chat-a", "run-1", std::process::id()).unwrap();
        let again = acquire(&path, "chat-a", "run-1", std::process::id()).expect("same chat, same run");
        assert_eq!(first, again);
    }

    #[test]
    fn a_dead_holder_is_reclaimed() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_for(dir.path());

        // A pid that cannot exist: pid 1 is launchd, so use a huge number.
        let dead = Lease {
            chat_id: "ghost".into(),
            session_id: "run-dead".into(),
            pid: 4_000_000,
            since: 0,
            app: None,
        };
        std::fs::write(&path, serde_json::to_string(&dead).unwrap()).unwrap();

        let lease = acquire(&path, "chat-a", "run-1", std::process::id())
            .expect("stale lease must be reclaimed");
        assert_eq!(lease.chat_id, "chat-a");
        assert_eq!(current(&path).unwrap().chat_id, "chat-a");
    }

    #[test]
    fn an_unreadable_lease_does_not_block_forever() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_for(dir.path());
        std::fs::write(&path, b"{not json").unwrap();

        let lease = acquire(&path, "chat-a", "run-1", std::process::id())
            .expect("garbage must be reclaimed");
        assert_eq!(lease.chat_id, "chat-a");
    }

    #[test]
    fn only_the_owner_releases() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_for(dir.path());

        acquire(&path, "chat-a", "run-1", std::process::id()).unwrap();
        assert!(!release(&path, "chat-b").expect("non-owner is not an error"));
        assert!(current(&path).is_some(), "the lease must survive");

        assert!(release(&path, "chat-a").expect("owner releases"));
        assert!(current(&path).is_none());
        assert!(!release(&path, "chat-a").expect("releasing nothing is fine"));
    }
}

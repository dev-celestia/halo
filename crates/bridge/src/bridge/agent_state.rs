//! Agent state-directory ownership repair.
//!
//! Every agent CLI keeps writable state under the user's home — `~/.pi` for Pi,
//! `~/.local/share/opencode` for OpenCode. A single
//! root-run `sudo make run` leaves those trees owned by **root**, and from then
//! on the CLI cannot write its own files:
//!
//! ```text
//! EACCES: permission denied, open '/Users/me/.pi/agent/auth.json.92827.tmp'
//! PlatformError: PermissionDenied: FileSystem.open (~/.local/share/opencode/log/opencode.log)
//! ```
//!
//! The app surfaced those as "not installed" / "AUTH REQUIRED" and offered a
//! retry, which cannot help: the binary is healthy, the *directory* is not.
//!
//! [`probe`] finds the blocked roots, [`repair`] reclaims them through the
//! shared primitives in [`super::doctor_actions`] — chown when we are root,
//! otherwise rename aside. Nothing is ever deleted.

use std::path::{Path, PathBuf};

use super::doctor::{DoctorCategory, DoctorItem, DoctorStatus};
use super::doctor_actions::{
    current_uid, dir_writable, escalate_chown, foreign_paths, owner_is_self, reclaim_root,
    reclaim_tree, Escalation, Reclaim,
};

/// Item id shared with `doctor::execute_install_action` and the setup cards.
pub(crate) const ITEM_ID: &str = "system:agent-state";

/// One agent CLI's state root, plus the file the CLI opens before doing
/// anything else.
struct StateRoot {
    /// Shown in the row's hint, e.g. "Pi".
    label: &'static str,
    path: PathBuf,
    /// Directory the CLI creates temporary siblings in. `None` when plain
    /// writability of the root is enough to judge.
    write_dir: Option<PathBuf>,
}

/// Every root Halo's harnesses write to. Missing directories are skipped —
/// a CLI that has never run has nothing to reclaim.
fn roots() -> Vec<StateRoot> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut roots: Vec<StateRoot> = Vec::new();

    // Pi keeps `~/.pi/agent` — that is where `auth.json` lives, and where the
    // `.tmp` sibling is created.
    roots.push(StateRoot {
        label: "Pi",
        path: home.join(".pi"),
        write_dir: Some(home.join(".pi/agent")),
    });

    // OpenCode follows XDG on every platform including macOS —
    // `dirs::data_local_dir()` would point at `~/Library/Application Support`,
    // which it never writes to.
    let opencode = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(|xdg| PathBuf::from(xdg).join("opencode"))
        .unwrap_or_else(|| home.join(".local/share/opencode"));
    roots.push(StateRoot {
        label: "OpenCode",
        path: opencode,
        write_dir: None,
    });

    // A root-owned run (`sudo make run`) keeps `HOME=/Users/<user>` on macOS,
    // but `sudo -H` does not — fall back to the invoking user's home.
    if let Ok(user) = std::env::var("SUDO_USER") {
        let user = user.trim();
        if !user.is_empty() && user != "root" {
            let pi = Path::new("/Users").join(user).join(".pi");
            if !roots.iter().any(|root| root.path == pi) {
                roots.push(StateRoot {
                    label: "Pi",
                    path: pi,
                    write_dir: None,
                });
            }
        }
    }

    roots.retain(|root| root.path.is_dir());
    roots
}

/// Why `root` cannot be written, if it cannot.
///
/// Order matters: a foreign root is checked first because a 0700 root-owned
/// directory is unreadable, so the deeper probes would silently report nothing.
fn blocked_reason(root: &StateRoot) -> Option<String> {
    if !owner_is_self(&root.path) {
        return Some("is owned by another user (usually root)".to_string());
    }
    if !dir_writable(&root.path) {
        return Some("is not writable by this user".to_string());
    }
    if let Some(dir) = &root.write_dir {
        if let Some(reason) = write_dir_blocked(dir) {
            return Some(reason);
        }
    }
    let foreign = foreign_paths(&root.path);
    if !foreign.is_empty() {
        return Some(format!(
            "contains {} folder(s) owned by another user",
            foreign.len()
        ));
    }
    None
}

/// Whether the CLI can create a temporary sibling in `dir`.
///
/// Pi writes `auth.json.<pid>.tmp` next to `auth.json` and renames it, so a
/// directory that accepts no new files breaks sign-in even when the existing
/// files look fine.
fn write_dir_blocked(dir: &Path) -> Option<String> {
    if !dir.is_dir() {
        return None;
    }
    let probe = dir.join(".halo-write-probe");
    match std::fs::write(&probe, b"1") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            None
        }
        Err(_) => Some(format!(
            "is not writable, so the CLI cannot create its temporary auth file"
        )),
    }
}

/// Doctor item for the agent state directories.
pub(crate) fn probe() -> DoctorItem {
    let found = roots();
    let blocked: Vec<(String, PathBuf, String)> = found
        .iter()
        .filter_map(|root| {
            blocked_reason(root).map(|why| (root.label.to_string(), root.path.clone(), why))
        })
        .collect();

    let (status, version, hint) = if found.is_empty() {
        (
            DoctorStatus::Ready,
            None,
            "No agent CLI has created a state directory yet.".to_string(),
        )
    } else if blocked.is_empty() {
        (
            DoctorStatus::Ready,
            Some("Writable".to_string()),
            format!(
                "Owned by the current user and writable: {}.",
                found
                    .iter()
                    .map(|root| root.path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    } else {
        let mut detail = String::new();
        for (label, path, why) in &blocked {
            detail.push_str(&format!("{label}: {} {why}. ", path.display()));
        }
        detail.push_str(
            "Until this is fixed the CLI cannot write its own auth or log files, so sign-in and \
             every run fail with EACCES. Fix Ownership reclaims them — nothing is deleted.",
        );
        (
            DoctorStatus::Failed,
            Some(format!("{} blocked", blocked.len())),
            detail,
        )
    };

    DoctorItem {
        id: ITEM_ID.to_string(),
        category: DoctorCategory::System,
        name: "Agent State Directories".to_string(),
        description:
            "Ownership of the folders the agent CLIs write to (~/.pi, ~/.local/share/opencode)"
                .to_string(),
        command: "chown".to_string(),
        status,
        version,
        resolved_path: blocked
            .first()
            .map(|(_, path, _)| path.display().to_string())
            .or_else(|| found.first().map(|root| root.path.display().to_string())),
        hint,
        can_auto_install: !found.is_empty(),
        action_label: "Fix Ownership".to_string(),
        doc_url: None,
    }
}

/// Reclaim every blocked agent state root.
///
/// Three routes, best first:
///
/// 1. **Take ownership in place** — one macOS administrator prompt, `chown -R`,
///    nothing moves. The CLI keeps working from a terminal, which is the whole
///    point: `pi` in iTerm hits the same `EACCES` Halo does.
/// 2. **Chown as root** — when Halo itself is running privileged.
/// 3. **Move the root aside** — unprivileged fallback for a headless or
///    sandboxed run where no authorization prompt can appear. Nothing is
///    deleted; the CLI recreates the directory on its next launch.
///
/// A *dismissed* password prompt is an answer: the repair stops and leaves the
/// paths untouched rather than silently moving a directory the user just
/// declined to change.
fn ownership_prompt_dismissed() -> String {
    "Ownership was not changed — the administrator prompt was dismissed. \
     Nothing was moved and nothing was deleted."
        .to_string()
}

fn ownership_prompt_unavailable(why: &str) -> String {
    format!(
        "macOS did not show the administrator dialog ({why}). Nothing was moved and nothing was deleted."
    )
}

pub(crate) fn repair() -> Result<String, String> {
    let found = roots();
    if found.is_empty() {
        return Ok("No agent state directory found — nothing to reclaim.".to_string());
    }

    let blocked: Vec<PathBuf> = found
        .iter()
        .filter(|root| blocked_reason(root).is_some())
        .map(|root| root.path.clone())
        .collect();
    if blocked.is_empty() {
        return Ok("Agent state directories are already owned by you and writable.".to_string());
    }

    // 1. Ask once for every blocked root, and take ownership in place.
    let mut chowned_in_place = false;
    let mut escalation_note: Option<String> = None;
    if current_uid() != Some(0) {
        match escalate_chown(&blocked) {
            Ok(()) => chowned_in_place = true,
            Err(Escalation::Cancelled) => {
                return Err(ownership_prompt_dismissed());
            }
            Err(Escalation::Unavailable(why)) => {
                // A Mac can show the dialog. Don't move the directory or copy
                // a sudo line when that call failed.
                #[cfg(target_os = "macos")]
                {
                    return Err(ownership_prompt_unavailable(&why));
                }
                #[cfg(not(target_os = "macos"))]
                {
                    escalation_note = Some(why);
                }
            }
        }
    }

    // 2. Whatever is still blocked, reclaim with what this process can do.
    let mut reclaim = Reclaim::default();
    let mut moved_root = false;
    for root in &found {
        if blocked_reason(root).is_none() {
            continue;
        }
        // A foreign root has to be taken whole: its children may be unreadable
        // (0700 root), so `reclaim_tree` would see nothing to do.
        if !owner_is_self(&root.path) {
            let outcome = reclaim_root(&root.path);
            moved_root |= !outcome.moved.is_empty();
            reclaim.merge(outcome);
            continue;
        }
        reclaim.merge(reclaim_tree(&root.path));
    }

    // Re-probe rather than trusting the primitives: a partially reclaimed tree
    // is still broken, and a green toast on a broken CLI is the worst outcome.
    let still: Vec<(String, PathBuf, String)> = roots()
        .iter()
        .filter_map(|root| {
            blocked_reason(root).map(|why| (root.label.to_string(), root.path.clone(), why))
        })
        .collect();
    if let Some((label, path, why)) = still.first() {
        let paths: Vec<PathBuf> = still.iter().map(|(_, path, _)| path.clone()).collect();
        if current_uid() != Some(0) {
            match escalate_chown(&paths) {
                Ok(()) => {}
                Err(Escalation::Cancelled) => return Err(ownership_prompt_dismissed()),
                Err(Escalation::Unavailable(detail)) => {
                    return Err(format!(
                        "{label}: {} still {why}. {}",
                        path.display(),
                        ownership_prompt_unavailable(&detail)
                    ));
                }
            }
        }
        let still: Vec<(String, PathBuf, String)> = roots()
            .iter()
            .filter_map(|root| {
                blocked_reason(root).map(|why| (root.label.to_string(), root.path.clone(), why))
            })
            .collect();
        if let Some((label, path, why)) = still.first() {
            return Err(format!(
                "{label}: {} still {why}.",
                path.display()
            ));
        }
    }
    if !reclaim.failures.is_empty() {
        return Err(format!(
            "reclaimed {} path(s) but {} failed: {}",
            reclaim.total(),
            reclaim.failures.len(),
            reclaim.failures.join("; ")
        ));
    }

    if chowned_in_place && reclaim.total() == 0 {
        return Ok(format!(
            "Took ownership of {} in place — nothing was moved, so the CLIs keep working from a \
             terminal too.",
            blocked
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let mut message = reclaim.summary("Agent state directories");
    if moved_root {
        message.push_str(
            " The whole folder was moved aside, so sign in again once — the CLI recreates it.",
        );
    }
    if let Some(why) = escalation_note {
        message.push_str(&format!(
            " No administrator prompt was available ({why}), so ownership was reclaimed without it."
        ));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::super::doctor_actions::move_aside;
    use super::*;

    #[test]
    fn item_id_is_stable() {
        assert_eq!(ITEM_ID, "system:agent-state");
    }

    #[test]
    fn every_existing_root_is_labelled() {
        for root in roots() {
            assert!(root.path.is_dir(), "{} should exist", root.path.display());
            assert!(!root.label.is_empty());
        }
    }

    #[test]
    fn writable_directory_is_not_blocked() {
        let dir = std::env::temp_dir().join(format!("halo-agent-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert_eq!(write_dir_blocked(&dir), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_write_directory_is_not_a_blocker() {
        let missing = std::env::temp_dir().join("halo-agent-missing-dir");
        assert_eq!(write_dir_blocked(&missing), None);
    }

    #[test]
    fn our_own_directory_is_never_foreign() {
        assert!(owner_is_self(&std::env::temp_dir()));
    }

    #[test]
    fn reclaim_root_leaves_our_own_directory_alone() {
        let dir = std::env::temp_dir().join(format!("halo-reclaim-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let outcome = reclaim_root(&dir);
        assert_eq!(outcome.total(), 0);
        assert!(dir.exists(), "nothing should have moved");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_aside_keeps_the_original_name_visible() {
        let dir = std::env::temp_dir().join(format!("halo-agent-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("auth.json");
        std::fs::write(&target, b"{}").unwrap();

        let dest = move_aside(&target).expect("move aside");
        assert!(dest.exists());
        assert!(!target.exists());
        assert!(dest
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("auth.json.root-bak-"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

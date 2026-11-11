//! Orphaned child-process detection.

use super::*;

/// Command lines this app spawns and must not leave running after a session.
///
/// One list, used by both the probe and the cleanup: when they were written out
/// twice they could drift, and a row that counts a process it will not kill is
/// worse than no row at all.
///
/// `opencode serve` belongs here because every `opencode models` probe starts a
/// managed service. Left behind, those services hold the fixed ports the next
/// probe needs, `opencode models` exits 1, and the model picker silently falls
/// back to the three hardcoded entries.
pub(crate) const ORPHANABLE_COMMANDS: &[&str] = &[
    "rust-analyzer",
    "typescript-language-server",
    "bash-language-server",
    "opencode serve",
];

pub(crate) fn is_orphanable(command: &str) -> bool {
    ORPHANABLE_COMMANDS
        .iter()
        .any(|needle| command.contains(needle))
}

/// Orphaned `(pid, command line)` pairs this app should have reaped.
///
/// One helper for both the probe and the cleanup, so a row can never count a
/// process the button will not kill. `args` is requested because `comm` is only
/// the executable path — it cannot tell `opencode serve` from an interactive
/// `opencode`. If the platform rejects that keyword the call degrades to
/// `comm`, which then simply cannot see the services; it never reports a
/// false all-clear *and* a false count.
pub(crate) fn orphan_pids() -> Vec<(String, String)> {
    let mut out = Vec::new();
    #[cfg(unix)]
    for format in ["pid,ppid,comm,args", "pid,ppid,comm"] {
        let Ok(output) = Command::new("ps").args(["-eo", format]).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let (pid, ppid) = (parts[0], parts[1]);
            let command = parts[2..].join(" ");
            if ppid == "1" && is_orphanable(&command) {
                out.push((pid.to_string(), command));
            }
        }
        return out;
    }
    out
}

pub(crate) fn probe_orphan_processes() -> DoctorItem {
    let orphans = orphan_pids();

    let count = orphans.len();
    let (status, hint, version, can_auto_install) = if count > 0 {
        (
            DoctorStatus::NeedsUpgrade,
            format!(
                "{count} orphaned background process(es) lingering with ppid=1 — language servers \
                 and the managed services `opencode models` starts. Left running they hold the \
                 ports the next probe needs."
            ),
            Some(format!("{count} orphans")),
            true,
        )
    } else {
        (
            DoctorStatus::Ready,
            "No orphaned background child processes detected.".to_string(),
            Some("Clean".to_string()),
            false,
        )
    };

    DoctorItem {
        id: "system:orphan-processes".to_string(),
        category: DoctorCategory::System,
        name: "Background Process Hygiene".to_string(),
        description:
            "Detects orphan language servers and leaked agent services lingering after sessions"
                .to_string(),
        command: "ps".to_string(),
        status,
        version,
        resolved_path: None,
        hint,
        can_auto_install,
        action_label: "Clean Processes".to_string(),
        doc_url: None,
    }
}

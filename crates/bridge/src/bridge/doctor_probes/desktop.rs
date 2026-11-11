//! macOS agent-desktop app detection and permission reporting.

use super::*;

pub(crate) fn probe_agent_desktop() -> (DoctorItem, Option<DoctorItem>) {
    let mut version = None;
    let mut resolved_path = None;
    let mut status = DoctorStatus::Missing;
    let mut permissions = None;

    if let Some(path) = doctor_actions::find_tool("agent-desktop") {
        resolved_path = Some(path.clone());
        // `status` carries the version and the TCC state in one spawn. An
        // engine too old to know the subcommand still gets an install row from
        // the `--version` fallback, minus the permission row.
        let report = agent_desktop_status(Path::new(&path));
        version = report.as_ref().and_then(|report| report.version.clone());
        if version.is_none() {
            if let Ok(output) = Command::new(&path).arg("--version").output() {
                if output.status.success() {
                    let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !v.is_empty() {
                        version = Some(v);
                    }
                }
            }
        }
        if version.is_some() {
            status = DoctorStatus::Ready;
            // TCC is a macOS concept; other platforms have nothing to grant.
            if cfg!(target_os = "macos") {
                permissions = Some(agent_desktop_permissions_item(
                    Path::new(&path),
                    report.as_ref().map(|report| &report.permissions),
                ));
            }
        }
    }

    let hint = if status == DoctorStatus::Ready {
        "agent-desktop is installed and ready (https://agent-browser.dev/desktop).".to_string()
    } else {
        "npm install -g agent-desktop (https://agent-browser.dev/desktop)".to_string()
    };

    (
        DoctorItem {
            id: "browser:agent-desktop".to_string(),
            category: DoctorCategory::BrowserProxy,
            name: "Agent Desktop".to_string(),
            description:
                "OS accessibility tree automation engine for AI agents (ref-based desktop computer use)"
                    .to_string(),
            command: "agent-desktop".to_string(),
            status,
            version,
            resolved_path,
            hint,
            can_auto_install: true,
            action_label: if status == DoctorStatus::Ready {
                "Reinstall".into()
            } else {
                "Install".into()
            },
            doc_url: Some("https://agent-browser.dev/desktop".to_string()),
        },
        permissions,
    )
}

/// The engine's own permission report.
#[derive(Clone, Copy, Default)]
pub(crate) struct DesktopPermissions {
    /// `None` means the engine did not say, which is not the same as denied.
    pub(crate) accessibility: Option<bool>,
    pub(crate) screen_recording: Option<bool>,
}

pub(crate) struct AgentDesktopReport {
    pub(crate) version: Option<String>,
    pub(crate) permissions: DesktopPermissions,
}

/// `agent-desktop status` → version + permission state.
///
/// One envelope on stdout; an install-level failure (`ok: false` or unparsable
/// output) is `None`, which leaves the install row intact and the permission
/// row in its "state unknown" form.
pub(crate) fn agent_desktop_status(path: &Path) -> Option<AgentDesktopReport> {
    let output = Command::new(path).arg("status").output().ok()?;
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    if !envelope
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    let data = envelope.get("data")?;
    let read = |key: &str| {
        data.get("permissions")
            .and_then(|permissions| permissions.get(key))
            .and_then(|permission| permission.get("state"))
            .and_then(|state| state.as_str())
            .map(|state| state.eq_ignore_ascii_case("granted"))
    };
    Some(AgentDesktopReport {
        version: data
            .get("version")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        permissions: DesktopPermissions {
            accessibility: read("accessibility"),
            screen_recording: read("screen_recording"),
        },
    })
}

/// The second row: what the engine can actually do right now.
///
/// "Installed" and "usable" are different states — the binary can be present
/// and still be unable to read a single window. Keeping this as its own row
/// means the install action stays "Reinstall" while the fix for a denied grant
/// is the one thing only the user can complete in System Settings.
pub(crate) fn agent_desktop_permissions_item(
    path: &Path,
    permissions: Option<&DesktopPermissions>,
) -> DoctorItem {
    const ACCESSIBILITY: &str = "System Settings › Privacy & Security › Accessibility";
    const SCREEN_RECORDING: &str = "System Settings › Privacy & Security › Screen Recording";

    let accessibility = permissions.and_then(|p| p.accessibility) == Some(true);
    let screen_recording = permissions.and_then(|p| p.screen_recording) == Some(true);

    let (status, description, hint, can_auto_install) = match permissions {
        // The probe ran but the engine would not answer: say so instead of
        // guessing "denied", which would send the user to a pane they may have
        // already configured.
        None => (
            DoctorStatus::Failed,
            "Permission state could not be read from the engine.".to_string(),
            format!(
                "Run `agent-desktop status` to see the error. If Accessibility is the problem, \
                 open {ACCESSIBILITY} and add Halo (plus agent-desktop if it is listed separately)."
            ),
            true,
        ),
        Some(_) if accessibility && screen_recording => (
            DoctorStatus::Ready,
            "Accessibility and Screen Recording are granted — computer use is ready.".to_string(),
            "Nothing to approve.".to_string(),
            false,
        ),
        // Screen Recording is optional: reading and acting work without it, so
        // this is a note, not a failure.
        Some(_) if accessibility => (
            DoctorStatus::Ready,
            "Accessibility granted. Screen Recording denied — screenshots only.".to_string(),
            format!(
                "For screenshots, open {SCREEN_RECORDING}, add Halo, then quit and reopen Halo: \
                 macOS applies a new Screen Recording grant only to a fresh process."
            ),
            true,
        ),
        Some(_) => (
            DoctorStatus::Failed,
            "Accessibility denied — desktop apps cannot be read or operated.".to_string(),
            format!(
                "Open {ACCESSIBILITY}, add Halo (the app that launches agent-desktop) and \
                 agent-desktop if it is listed separately, switch it on, then click Re-check All."
            ),
            true,
        ),
    };

    DoctorItem {
        id: "browser:agent-desktop:permissions".to_string(),
        category: DoctorCategory::BrowserProxy,
        name: "Agent Desktop Permissions".to_string(),
        description,
        command: "agent-desktop permissions".to_string(),
        status,
        version: None,
        resolved_path: Some(path.display().to_string()),
        hint,
        can_auto_install,
        action_label: "Open permission guide".to_string(),
        doc_url: Some("https://agent-browser.dev/desktop".to_string()),
    }
}

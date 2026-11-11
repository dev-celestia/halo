//! Computer-use status for Settings and the in-chat lease strip.
//!
//! The engine probe is the same `agent-desktop status` Doctor already runs.
//! The lease file is a local read, so the strip can follow it without spawning.

use std::path::PathBuf;
use std::process::Command;

use crate::DesktopStatus;

use super::super::*;

impl Bridge {
    /// Resolve the engine and the lease off the UI thread.
    pub fn halo_agent_refresh_desktop(cx: &mut App) -> BridgeResult<()> {
        let chat_id = Self::state(cx)
            .read(cx)
            .coder
            .agent_active_session_id
            .clone();
        Self::state(cx).update(cx, |s| s.coder.agent_desktop_loading = true);
        Self::run_blocking(
            cx,
            move || Ok(probe_desktop(&chat_id)),
            move |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.coder.agent_desktop_loading = false;
                    if let Ok(status) = result {
                        s.coder.agent_desktop = status;
                    }
                });
            },
        );
        Ok(())
    }

    /// Cheap lease read for the one-second work tick. Spawns nothing.
    pub fn halo_agent_poll_desktop_lease(cx: &mut App) {
        let chat_id = Self::state(cx)
            .read(cx)
            .coder
            .agent_active_session_id
            .clone();
        let (held, app) = lease_for_chat(&chat_id);
        Self::state(cx).update(cx, |s| {
            if s.coder.agent_desktop.lease_held == held && s.coder.agent_desktop.lease_app == app {
                return;
            }
            s.coder.agent_desktop.lease_held = held;
            s.coder.agent_desktop.lease_app = app;
        });
    }

    /// Drop the lease when this chat holds it. Stop does not touch another chat.
    pub fn halo_agent_release_desktop(cx: &mut App) -> BridgeResult<()> {
        let chat_id = Self::state(cx)
            .read(cx)
            .coder
            .agent_active_session_id
            .clone();
        Self::run_blocking(
            cx,
            move || release_lease(&chat_id).map_err(crate::BridgeError::message),
            move |result, cx| {
                if result.is_ok() {
                    Self::halo_agent_poll_desktop_lease(cx);
                }
            },
        );
        Ok(())
    }

    /// Open the macOS permission panes. Never requests a grant by itself.
    pub fn halo_agent_open_desktop_permissions(cx: &mut App) -> BridgeResult<()> {
        Self::run_blocking(
            cx,
            || open_permission_guide().map_err(crate::BridgeError::message),
            |_result, _cx| {},
        );
        Ok(())
    }

    pub fn halo_agent_toggle_dock_todo(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_dock_todo_open = !s.coder.agent_dock_todo_open;
        });
        Ok(())
    }

    pub fn halo_agent_toggle_dock_queue(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_dock_queue_open = !s.coder.agent_dock_queue_open;
        });
        Ok(())
    }
}

fn probe_desktop(chat_id: &str) -> DesktopStatus {
    let (lease_held, lease_app) = lease_for_chat(chat_id);
    let Some(path) = super::super::doctor_actions::find_tool("agent-desktop") else {
        return DesktopStatus {
            installed: false,
            version: None,
            detail: "npm install -g agent-desktop".into(),
            accessibility: None,
            screen_recording: None,
            lease_held,
            lease_app,
        };
    };
    let Some(report) = engine_status(&path) else {
        return DesktopStatus {
            installed: true,
            version: None,
            detail: "Installed, but status could not be read.".into(),
            accessibility: None,
            screen_recording: None,
            lease_held,
            lease_app,
        };
    };
    DesktopStatus {
        installed: true,
        detail: report
            .version
            .clone()
            .unwrap_or_else(|| "agent-desktop".into()),
        version: report.version,
        accessibility: report.accessibility,
        screen_recording: report.screen_recording,
        lease_held,
        lease_app,
    }
}

struct EngineReport {
    version: Option<String>,
    accessibility: Option<bool>,
    screen_recording: Option<bool>,
}

fn engine_status(path: &str) -> Option<EngineReport> {
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
    Some(EngineReport {
        version: data
            .get("version")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        accessibility: read("accessibility"),
        screen_recording: read("screen_recording"),
    })
}

fn lease_path() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join("Library/Caches/Halo/desktop/lease.json"),
    )
}

fn lease_for_chat(chat_id: &str) -> (bool, Option<String>) {
    if chat_id.is_empty() {
        return (false, None);
    }
    let Some(path) = lease_path() else {
        return (false, None);
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return (false, None);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return (false, None);
    };
    let holder = value.get("chat_id").and_then(|v| v.as_str()).unwrap_or("");
    if holder != chat_id {
        return (false, None);
    }
    let pid = value.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if pid != 0 && !pid_alive(pid) {
        return (false, None);
    }
    let app = value
        .get("app")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|app| !app.is_empty())
        .map(str::to_string);
    (true, app)
}

fn release_lease(chat_id: &str) -> Result<(), String> {
    let (held, _) = lease_for_chat(chat_id);
    if !held {
        return Ok(());
    }
    let path = lease_path().ok_or("no home directory for the desktop lease")?;
    std::fs::remove_file(&path).map_err(|error| format!("could not release the lease: {error}"))
}

fn open_permission_guide() -> Result<(), String> {
    let path = super::super::doctor_actions::find_tool("agent-desktop")
        .ok_or("agent-desktop is not installed — install it, then open the guide")?;
    let output = Command::new(&path)
        .args(["permissions", "--guide"])
        .output()
        .map_err(|error| format!("could not run {path}: {error}"))?;
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("agent-desktop printed something that is not JSON: {error}"))?;
    if envelope
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        Ok(())
    } else {
        let message = envelope
            .pointer("/error/message")
            .and_then(|value| value.as_str())
            .unwrap_or("the permission guide did not open");
        Err(message.to_string())
    }
}

fn pid_alive(pid: u32) -> bool {
    match Command::new("kill")
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

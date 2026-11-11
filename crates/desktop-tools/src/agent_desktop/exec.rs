//! Spawning the engine and shaping its argv.
//!
//! Flags are built here and nowhere else, so the wrapper and the engine cannot
//! drift: a flag this file does not emit is a flag the agent cannot reach.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::{parse_status, unwrap_envelope};
use crate::engine::Engine;
use crate::types::{ActRequest, EngineStatus, FindRequest, RawSnapshot, SnapshotRequest};
use crate::ToolResult;

pub struct AgentDesktopEngine {
    bin: PathBuf,
    /// Passed as `--session`; keeps refs and traces in one namespace per chat.
    session: Option<String>,
    /// `AGENT_DESKTOP_HOME`. Relocated so Halo's sessions never mix with the
    /// user's own CLI sessions.
    home: Option<String>,
}

impl AgentDesktopEngine {
    pub fn new(bin: PathBuf) -> Self {
        Self {
            bin,
            session: None,
            home: None,
        }
    }

    pub fn from_env() -> ToolResult<Self> {
        let bin = super::probe::resolve_bin(None)?;
        let session = std::env::var("HALO_DESKTOP_SESSION").ok().filter(|id| !id.trim().is_empty());
        let home = match std::env::var("HALO_DESKTOP_HOME") {
            Ok(home) if home.trim().is_empty() => Some(crate::engine_home().display().to_string()),
            Ok(home) if std::path::Path::new(&home).is_absolute() => Some(home),
            Ok(home) => {
                // The engine rejects a relative root; say so here, where the
                // message can name the variable.
                return Err(format!(
                    "HALO_DESKTOP_HOME must be an absolute path (got {home})"
                ));
            }
            // Halo's own root, so a run never writes into the user's
            // `~/.agent-desktop` sessions.
            Err(_) => Some(crate::engine_home().display().to_string()),
        };
        Ok(Self {
            bin,
            session,
            home,
        })
    }

    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session = Some(session.into());
        self
    }

    pub fn with_home(mut self, home: impl Into<String>) -> Self {
        self.home = Some(home.into());
        self
    }

    pub fn bin(&self) -> &PathBuf {
        &self.bin
    }

    fn run(&self, args: &[String]) -> ToolResult<Value> {
        let mut command = Command::new(&self.bin);
        command.args(args);
        if let Some(session) = &self.session {
            command.arg("--session").arg(session);
        }
        if let Some(home) = &self.home {
            command.env("AGENT_DESKTOP_HOME", home);
        }

        let output = command
            .output()
            .map_err(|error| format!("could not run {}: {error}", self.bin.display()))?;
        unwrap_envelope(
            &String::from_utf8_lossy(&output.stdout),
            output.status.code().unwrap_or(-1),
            &String::from_utf8_lossy(&output.stderr),
        )
    }
}

impl Engine for AgentDesktopEngine {
    fn status(&mut self) -> ToolResult<EngineStatus> {
        let data = self.run(&["status".to_string()])?;
        Ok(parse_status(&data))
    }

    fn apps(&mut self, filter: Option<&str>) -> ToolResult<Value> {
        let mut args = vec!["list-apps".to_string()];
        push_opt(&mut args, "--app", filter);
        self.run(&args)
    }

    fn windows(&mut self, app: Option<&str>) -> ToolResult<Value> {
        let mut args = vec!["list-windows".to_string()];
        push_opt(&mut args, "--app", app);
        self.run(&args)
    }

    fn snapshot_raw(&mut self, request: &SnapshotRequest) -> ToolResult<RawSnapshot> {
        let args = snapshot_args(request);
        let data = self.run(&args)?;
        Ok(raw_snapshot(&data))
    }

    fn find_raw(&mut self, request: &FindRequest) -> ToolResult<Value> {
        let mut args = vec!["find".to_string()];
        push_opt(&mut args, "--app", request.app.as_deref());
        push_opt(&mut args, "--role", request.role.as_deref());
        push_opt(&mut args, "--name", request.name.as_deref());
        push_opt(&mut args, "--text", request.text.as_deref());
        push_opt(&mut args, "--root", request.root.as_deref());
        push_opt(&mut args, "--surface", request.surface.as_deref());
        if request.exact {
            args.push("--exact".to_string());
        }
        if request.first {
            args.push("--first".to_string());
        }
        if let Some(limit) = request.limit {
            push_pair(&mut args, "--limit", &limit.to_string());
        }
        self.run(&args)
    }

    fn read_raw(&mut self, reference: &str, property: Option<&str>) -> ToolResult<Value> {
        let mut args = vec!["get".to_string(), reference.to_string()];
        push_opt(&mut args, "--property", property);
        self.run(&args)
    }

    fn perform(&mut self, request: &ActRequest) -> ToolResult<Value> {
        self.run(&act_args(request)?)
    }
}

fn raw_snapshot(data: &Value) -> RawSnapshot {
    let window_title = data
        .get("window")
        .and_then(|window| window.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    RawSnapshot {
        app: data
            .get("app")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        window_title,
        snapshot_id: data
            .get("snapshot_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ref_count: data.get("ref_count").and_then(Value::as_u64).unwrap_or(0),
        // Absent means the engine did not say it was partial.
        complete: data.get("complete").and_then(Value::as_bool).unwrap_or(true),
        truncated: data
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        nodes_observed: data.get("nodes_observed").and_then(Value::as_u64),
        tree: data.get("tree").cloned().unwrap_or(Value::Null),
    }
}

/// Ask the engine to open the right System Settings pane.
///
/// Only ever on an explicit user action. Opening a privacy pane unasked is
/// startling, and *requesting* a grant silently is worse — the plan's rule is
/// that Halo reports state and offers the guide, never prompts on its own.
pub fn open_permission_guide(bin: &Path) -> ToolResult<String> {
    let output = Command::new(bin)
        .args(["permissions", "--guide"])
        .output()
        .map_err(|error| format!("could not run {}: {error}", bin.display()))?;
    unwrap_envelope(
        &String::from_utf8_lossy(&output.stdout),
        output.status.code().unwrap_or(-1),
        &String::from_utf8_lossy(&output.stderr),
    )?;
    Ok("opened the permission guide".to_string())
}

fn push_opt(args: &mut Vec<String>, flag: &str, value: Option<&str>) {    if let Some(value) = value {
        let value = value.trim();
        if !value.is_empty() {
            push_pair(args, flag, value);
        }
    }
}

fn push_pair(args: &mut Vec<String>, flag: &str, value: &str) {
    args.push(flag.to_string());
    args.push(value.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_argv_matches_the_engine_flags() {
        let request = SnapshotRequest {
            app: Some("TextEdit".into()),
            window_id: Some("w-1".into()),
            surface: None,
            root: None,
            skeleton: true,
            interactive: true,
            compact: true,
            max_depth: Some(4),
            timeout_ms: Some(5_000),
            max_chars: None,
        };
        // Exercised through the pure builder below.
        let args = snapshot_args(&request);
        assert_eq!(args[0], "snapshot");
        assert!(args.contains(&"--app".to_string()));
        assert!(args.contains(&"TextEdit".to_string()));
        assert!(args.contains(&"--skeleton".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"--max-depth".to_string()));
        assert!(args.contains(&"5000".to_string()));
    }

    #[test]
    fn an_empty_app_name_is_not_passed_as_a_flag() {
        let request = SnapshotRequest {
            app: Some("   ".into()),
            interactive: false,
            ..SnapshotRequest::default()
        };
        let args = snapshot_args(&request);
        assert!(!args.contains(&"--app".to_string()), "{args:?}");
        assert!(!args.contains(&"-i".to_string()), "{args:?}");
    }

    #[test]
    fn act_argv_never_asks_for_physical_input() {
        let request = ActRequest {
            reference: Some("@snap:e1".into()),
            generation: 1,
            action: crate::types::ActAction::Click,
            value: None,
        };
        let args = act_args(&request).unwrap();
        assert_eq!(args, vec!["click".to_string(), "@snap:e1".to_string()]);
        assert!(!args.iter().any(|arg| arg == "--headed"), "{args:?}");

        let press = ActRequest {
            reference: None,
            generation: 1,
            action: crate::types::ActAction::Press,
            value: Some("return".into()),
        };
        assert_eq!(
            act_args(&press).unwrap(),
            vec!["press".to_string(), "return".to_string()]
        );
    }
}

/// Split out of [`Engine::perform`] so the argv shape is testable without a
/// process. `--headed` is never emitted.
fn act_args(request: &ActRequest) -> ToolResult<Vec<String>> {
    let name = request.action.engine_name();
    let mut args = vec![name.to_string()];
    match request.action {
        crate::types::ActAction::Press => {
            let combo = required_value(request, "press")?;
            args.push(combo);
        }
        crate::types::ActAction::Type | crate::types::ActAction::Select => {
            args.push(required_ref(request)?);
            args.push(required_value(request, name)?);
        }
        crate::types::ActAction::Scroll => {
            args.push(required_ref(request)?);
            if let Some(direction) = request.value.as_deref().map(str::trim).filter(|v| !v.is_empty())
            {
                args.push("--direction".to_string());
                args.push(direction.to_string());
            }
        }
        crate::types::ActAction::Click | crate::types::ActAction::Focus => {
            args.push(required_ref(request)?);
        }
    }
    Ok(args)
}

fn required_ref(request: &ActRequest) -> ToolResult<String> {
    request
        .reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{} needs --ref", request.action.engine_name()))
}

fn required_value(request: &ActRequest, action: &str) -> ToolResult<String> {
    request
        .value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| match action {
            "press" => "press needs --value (a key combo such as return or cmd+c)".to_string(),
            "type" => "type needs --value (the text to insert)".to_string(),
            _ => "select needs --value (the option to select)".to_string(),
        })
}

/// Split out of [`Engine::snapshot_raw`] so the argv shape is testable without
/// a process.
fn snapshot_args(request: &SnapshotRequest) -> Vec<String> {
    let mut args = vec!["snapshot".to_string()];
    push_opt(&mut args, "--app", request.app.as_deref());
    push_opt(&mut args, "--window-id", request.window_id.as_deref());
    push_opt(&mut args, "--surface", request.surface.as_deref());
    push_opt(&mut args, "--root", request.root.as_deref());
    if request.skeleton {
        args.push("--skeleton".to_string());
    }
    if request.interactive {
        args.push("-i".to_string());
    }
    if request.compact {
        args.push("--compact".to_string());
    }
    if let Some(depth) = request.max_depth {
        push_pair(&mut args, "--max-depth", &depth.to_string());
    }
    if let Some(timeout) = request.timeout_ms {
        push_pair(&mut args, "--timeout-ms", &timeout.to_string());
    }
    args
}

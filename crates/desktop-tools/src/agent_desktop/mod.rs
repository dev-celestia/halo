//! The `agent-desktop` CLI behind [`crate::Engine`].
//!
//! One JSON envelope per call, on stdout. The shape is
//! `{version, ok, command, data|error}` and the field order is not stable, so
//! everything here parses by key. Exit codes are a fast path only: `0` ok,
//! `1` runtime error, `2` argument error — the envelope is still the truth.

pub mod exec;
pub mod probe;

use serde_json::Value;

use crate::types::{EngineStatus, PermissionState};
use crate::ToolResult;

pub use exec::AgentDesktopEngine;

/// A refusal the engine already classified. Codes are the API; messages change.
#[derive(Debug, Clone)]
pub struct EngineError {
    pub code: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub delivery: Option<String>,
    pub retry: Option<String>,
}

impl EngineError {
    fn from_value(error: &Value) -> Self {
        let disposition = error.get("disposition");
        Self {
            code: text(error, "code").unwrap_or_else(|| "UNKNOWN".into()),
            message: text(error, "message").unwrap_or_default(),
            suggestion: text(error, "suggestion"),
            delivery: disposition.and_then(|value| text(value, "delivery")),
            retry: disposition.and_then(|value| text(value, "retry")),
        }
    }

    /// What the model is told. The engine's own words follow the advice, because
    /// the advice is what the model can act on and the wording is what a human
    /// debugging it needs.
    pub fn agent_message(&self) -> String {
        let advice = match self.code.as_str() {
            "PERM_DENIED" => crate::permissions::denied_advice(),
            "STALE_REF" => "That ref belongs to an older snapshot and no longer matches the \
                 live UI. Take a fresh snapshot and use the new ref."
                .to_string(),
            "SNAPSHOT_NOT_FOUND" => {
                "That snapshot expired. Take a fresh snapshot and use its refs.".to_string()
            }
            "AMBIGUOUS_TARGET" => "More than one element matches. Narrow it with --role, \
                 --name or --root, then use the new ref."
                .to_string(),
            "WINDOW_NOT_FOUND" => "That window is gone. List windows and pick another, or ask \
                 the user to open the app."
                .to_string(),
            "TIMEOUT" => "The engine hit its deadline before the UI settled. Raise --timeout-ms \
                 (a cold Chromium app can take 10-25 s) or narrow the read with --root."
                .to_string(),
            "POLICY_DENIED" => "The engine refused this action under its own policy.".to_string(),
            "INVALID_ARGS" => {
                "agent-desktop rejected the arguments; check the command's flags.".to_string()
            }
            _ => String::new(),
        };

        let mut out = String::new();
        if !advice.is_empty() {
            out.push_str(&advice);
        }
        if !self.message.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&format!("({})", self.message));
        }
        if let Some(suggestion) = &self.suggestion {
            if !out.contains(suggestion.as_str()) {
                out.push(' ');
                out.push_str(suggestion);
            }
        }
        if let Some(retry) = &self.retry {
            if retry != "safe" {
                out.push_str(&format!(
                    " Do not repeat this action: delivery was {:?}.",
                    self.delivery.as_deref().unwrap_or("unknown")
                ));
            }
        }
        out
    }
}

/// Pull `data` out of an envelope, or turn `error` into agent-facing text.
pub fn unwrap_envelope(stdout: &str, exit_code: i32, stderr: &str) -> ToolResult<Value> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            format!("agent-desktop printed nothing (exit {exit_code})")
        } else {
            format!("agent-desktop failed (exit {exit_code}): {detail}")
        });
    }

    let envelope: Value = serde_json::from_str(trimmed).map_err(|error| {
        let head: String = trimmed.chars().take(200).collect();
        format!("agent-desktop printed something that is not a JSON envelope ({error}): {head}")
    })?;

    if envelope.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(envelope.get("data").cloned().unwrap_or(Value::Null));
    }

    match envelope.get("error") {
        Some(error) => Err(EngineError::from_value(error).agent_message()),
        None => Err(format!(
            "agent-desktop reported a failure with no error object (exit {exit_code})"
        )),
    }
}

/// `status` → the fields this crate actually uses.
pub fn parse_status(data: &Value) -> EngineStatus {
    let permissions = data.get("permissions");
    let accessibility = permissions
        .and_then(|value| value.get("accessibility"))
        .and_then(|value| text(value, "state"))
        .map(|raw| PermissionState::parse(&raw))
        .unwrap_or(PermissionState::Unknown);
    let screen_recording = permissions
        .and_then(|value| value.get("screen_recording"))
        .and_then(|value| text(value, "state"))
        .map(|raw| PermissionState::parse(&raw))
        .unwrap_or(PermissionState::Unknown);

    EngineStatus {
        version: text(data, "version").unwrap_or_else(|| "unknown".into()),
        platform: text(data, "platform").unwrap_or_else(|| "unknown".into()),
        state_root: text(data, "state_root").unwrap_or_default(),
        accessibility,
        screen_recording,
        supported_surfaces: data
            .get("supported_surfaces")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        session_id: text(data, "session_id"),
    }
}

pub(crate) fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn success_returns_data() {
        let envelope = json!({"version":"2.5","ok":true,"command":"snapshot","data":{"ref_count":3}});
        let data = unwrap_envelope(&envelope.to_string(), 0, "").unwrap();
        assert_eq!(data["ref_count"], 3);
    }

    #[test]
    fn permission_denied_becomes_an_instruction() {
        let envelope = json!({
            "version":"2.5","ok":false,"command":"snapshot",
            "error":{"code":"PERM_DENIED","message":"Accessibility permission not granted",
                     "suggestion":"Run agent-desktop permissions --guide.",
                     "disposition":{"delivery":"not_delivered","retry":"safe"}}
        });
        let error = unwrap_envelope(&envelope.to_string(), 1, "").unwrap_err();
        assert!(error.contains("Accessibility"), "{error}");
        assert!(error.contains("Privacy & Security"), "{error}");
        // Safe to retry, so no scary warning.
        assert!(!error.contains("Do not repeat"), "{error}");
    }

    #[test]
    fn stale_ref_says_take_a_new_snapshot() {
        let envelope = json!({"ok":false,"error":{"code":"STALE_REF","message":"ref expired"}});
        let error = unwrap_envelope(&envelope.to_string(), 1, "").unwrap_err();
        assert!(error.contains("fresh snapshot"), "{error}");
    }

    #[test]
    fn an_unsafe_delivery_warns_against_repeating() {
        let envelope = json!({
            "ok":false,
            "error":{"code":"ACTION_FAILED","message":"click may have landed",
                     "disposition":{"delivery":"maybe","retry":"unsafe"}}
        });
        let error = unwrap_envelope(&envelope.to_string(), 1, "").unwrap_err();
        assert!(error.contains("Do not repeat"), "{error}");
    }

    #[test]
    fn empty_output_names_the_exit_code() {
        let error = unwrap_envelope("", 1, "boom").unwrap_err();
        assert!(error.contains("exit 1"), "{error}");
        assert!(error.contains("boom"), "{error}");
    }

    #[test]
    fn non_json_output_is_reported_not_swallowed() {
        let error = unwrap_envelope("<html>nope</html>", 0, "").unwrap_err();
        assert!(error.contains("not a JSON envelope"), "{error}");
    }

    #[test]
    fn status_maps_permissions_and_surfaces() {
        let data = json!({
            "version":"1.0.2","platform":"macos","state_root":"/tmp/x",
            "permissions":{"accessibility":{"state":"denied"},
                           "screen_recording":{"state":"granted"}},
            "supported_surfaces":["window","focused","menu"],
            "session_id": null
        });
        let status = parse_status(&data);
        assert_eq!(status.accessibility, PermissionState::Denied);
        assert_eq!(status.screen_recording, PermissionState::Granted);
        assert_eq!(status.supported_surfaces.len(), 3);
        assert_eq!(status.version, "1.0.2");
    }
}

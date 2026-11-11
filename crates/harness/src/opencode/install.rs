//! Install or upgrade OpenCode to a version that speaks the v2 HTTP API.

use std::path::Path;
use std::process::{Command, Stdio};

use super::probe::{MIN_VERSION, parse_semver, probe_environment, resolve_executable};
use super::types::OpenCodeProbeResult;

/// Official OpenCode 2 install script (curl installer).
const V2_INSTALL_URL: &str = "https://opencode.ai/v2/install";

/// Ensure a ready OpenCode ≥ [`MIN_VERSION`] is available.
///
/// - Missing → run the OpenCode 2 curl installer
/// - OpenCode 1.x → install OpenCode 2 via the v2 installer (1.x `upgrade`
///   often lacks `--major`, e.g. 1.2.24 just prints help)
/// - Too-old 2.x → `opencode upgrade`
pub fn install_or_upgrade(custom: Option<&str>) -> Result<(), String> {
    match probe_environment(custom) {
        OpenCodeProbeResult::Ready { .. } => Ok(()),
        OpenCodeProbeResult::WrongVersion { path, found, .. } => {
            if parse_semver(&found).is_some_and(|(maj, _, _)| maj < MIN_VERSION.0) {
                // Old 1.x CLIs reject `--major`; install v2 beside/over them.
                install_fresh()?;
                return verify_ready(custom);
            }
            upgrade_in_place(&path, custom)
        }
        OpenCodeProbeResult::Missing => {
            install_fresh()?;
            verify_ready(None)
        }
        // Reinstalling cannot repair an unwritable state directory.
        OpenCodeProbeResult::StateBlocked { message, .. }
        | OpenCodeProbeResult::Failed { message } => Err(message),
    }
}

fn upgrade_in_place(path: &Path, custom: Option<&str>) -> Result<(), String> {
    let output = Command::new(path)
        .arg("upgrade")
        .env("CI", "1")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run `opencode upgrade`: {e}"))?;
    if !output.status.success() {
        return Err(command_failure("opencode upgrade", &output));
    }
    verify_ready(custom)
}

fn verify_ready(custom: Option<&str>) -> Result<(), String> {
    match probe_environment(custom) {
        OpenCodeProbeResult::Ready { .. } => Ok(()),
        OpenCodeProbeResult::WrongVersion {
            found,
            required,
            path,
        } => Err(format!(
            "OpenCode is still {found} at {}; need ≥ {required}. Remove that binary (or clear the custom path) and retry — OpenCode 2 usually installs to ~/.opencode/bin.",
            path.display()
        )),
        other => Err(format!(
            "OpenCode install finished but probe failed: {}",
            other.setup_hint()
        )),
    }
}

fn install_fresh() -> Result<(), String> {
    #[cfg(windows)]
    {
        install_fresh_windows()
    }
    #[cfg(not(windows))]
    {
        install_fresh_unix()
    }
}

#[cfg(not(windows))]
fn install_fresh_unix() -> Result<(), String> {
    // `curl … | bash -s -- --no-modify-path` — don't rewrite the user's shell rc.
    let mut curl = Command::new("curl")
        .args(["-fsSL", V2_INSTALL_URL])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "could not run curl ({e}). Install curl, or install OpenCode from https://opencode.ai."
            )
        })?;

    let curl_stdout = curl
        .stdout
        .take()
        .ok_or_else(|| "curl produced no stdout".to_string())?;

    let bash = Command::new("bash")
        .args(["-s", "--", "--no-modify-path"])
        .stdin(Stdio::from(curl_stdout))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CI", "1")
        .output()
        .map_err(|e| format!("could not run OpenCode install script via bash: {e}"))?;

    let curl_status = curl.wait().map_err(|e| format!("waiting for curl: {e}"))?;
    if !curl_status.success() {
        return Err(format!(
            "failed to download OpenCode installer from {V2_INSTALL_URL} (curl exited {:?})",
            curl_status.code()
        ));
    }

    if !bash.status.success() {
        return Err(command_failure("OpenCode install script", &bash));
    }

    if resolve_executable(None).is_none() {
        return Err(
            "OpenCode installer finished, but `opencode` was not found. Open a new terminal or set ~/.opencode/bin/opencode as the custom path in Coder Settings."
                .into(),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn install_fresh_windows() -> Result<(), String> {
    // OpenCode 2 ships as `@opencode/cli` (not the legacy `opencode-ai` package).
    let output = Command::new("npm")
        .args(["install", "-g", "@opencode/cli@latest"])
        .env("CI", "1")
        .output()
        .map_err(|e| {
            format!(
                "could not run npm ({e}). Install OpenCode from https://opencode.ai, then retry."
            )
        })?;
    if !output.status.success() {
        return Err(command_failure("npm install -g @opencode/cli", &output));
    }
    Ok(())
}

fn command_failure(label: &str, output: &std::process::Output) -> String {
    let err = String::from_utf8_lossy(&output.stderr);
    let out = String::from_utf8_lossy(&output.stdout);
    let detail = if !err.trim().is_empty() {
        err.trim()
    } else if !out.trim().is_empty() {
        out.trim()
    } else {
        ""
    };
    // Trim huge yargs help dumps down to a short lead-in.
    let detail = detail.lines().take(4).collect::<Vec<_>>().join(" ");
    if detail.is_empty() {
        format!("{label} exited with code {:?}", output.status.code())
    } else {
        format!("{label} failed: {detail}")
    }
}

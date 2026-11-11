//! Finding the engine without trusting the caller's PATH.
//!
//! A Finder-launched app has `/usr/bin:/bin:/usr/sbin:/sbin`. `agent-desktop` is
//! usually installed by npm into a prefix that is on nobody's PATH in that
//! environment, and the user may also have installed it under Homebrew. Resolving
//! once, here, keeps every harness pointing at the same binary — and makes the
//! difference between "not installed" and "installed but denied" reportable.

use std::path::{Path, PathBuf};

use crate::ToolResult;

const BINARY: &str = "agent-desktop";
const CLI_BINARY: &str = "halo-desktop";

/// The `halo-desktop` front itself, which ships beside the app binary.
pub fn resolve_cli() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("HALO_DESKTOP_CLI") {
        let candidate = PathBuf::from(explicit);
        return is_executable(&candidate).then_some(candidate);
    }

    let mut dirs: Vec<PathBuf> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .into_iter()
        .collect();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.extend(standard_dirs());
    dirs.into_iter()
        .map(|dir| dir.join(CLI_BINARY))
        .find(|candidate| is_executable(candidate))
}

/// True when the agent can actually drive the desktop: the front resolves *and*
/// the engine does. Advertising one without the other costs the agent turns on a
/// tool that can only answer "not found".
pub fn ready() -> bool {
    resolve_cli().is_some() && resolve_bin(None).is_ok()
}

/// Explicit argument, then `HALO_DESKTOP_BIN`, then the usual install dirs.
pub fn resolve_bin(explicit: Option<&str>) -> ToolResult<PathBuf> {
    let explicit = explicit
        .map(str::to_owned)
        .or_else(|| std::env::var("HALO_DESKTOP_BIN").ok());

    if let Some(path) = explicit {
        let candidate = PathBuf::from(path);
        if !candidate.is_absolute() || !is_executable(&candidate) {
            return Err(format!(
                "HALO_DESKTOP_BIN must be an existing, executable absolute path (got {})",
                candidate.display()
            ));
        }
        return Ok(candidate);
    }

    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(standard_dirs());

    for dir in dirs {
        let candidate = dir.join(BINARY);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }

    Err(
        "agent-desktop was not found. Install it (`npm i -g agent-desktop`) or set \
         HALO_DESKTOP_BIN to its absolute path."
            .to_string(),
    )
}

/// `agent-desktop --version`, or `None` when the binary refuses to answer.
pub fn version(bin: &Path) -> Option<String> {
    let output = std::process::Command::new(bin)
        .arg("--version")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn standard_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.extend([
            home.join(".npm-global/bin"),
            home.join(".local/bin"),
            home.join(".bun/bin"),
        ]);
    }
    dirs
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

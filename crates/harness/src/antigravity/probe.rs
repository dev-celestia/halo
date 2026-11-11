//! Resolve and probe Google's `agy_acp_server` (antigravity-acp).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::install::{self, PINNED_VERSION};
use super::types::{AntigravityModelInfo, AntigravityProbeResult};

pub const ENV_OVERRIDE: &str = "ANTIGRAVITY_ACP_EXECUTABLE";

pub fn entry_name() -> &'static str {
    if cfg!(windows) {
        "agy_acp_server.exe"
    } else {
        "agy_acp_server.par"
    }
}

pub fn launch_args() -> &'static [&'static str] {
    if cfg!(target_os = "linux") {
        &["--uid="]
    } else {
        &[]
    }
}

pub fn resolve_executable(custom: Option<&str>) -> Option<PathBuf> {
    if let Some(custom) = custom.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(custom);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(env) = std::env::var(ENV_OVERRIDE) {
        let trimmed = env.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    if let Some(managed) = install::managed_entry() {
        if managed.is_file() {
            return Some(managed);
        }
    }
    find_on_path(entry_name()).or_else(|| find_on_path("agy_acp_server"))
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

pub fn is_installed(custom: Option<&str>) -> bool {
    matches!(
        probe_environment(custom),
        AntigravityProbeResult::Ready { .. }
    )
}

pub fn probe_environment(custom: Option<&str>) -> AntigravityProbeResult {
    let Some(path) = resolve_executable(custom) else {
        return AntigravityProbeResult::Missing;
    };
    match version_of(&path) {
        Ok(version) => AntigravityProbeResult::Ready { version, path },
        Err(message) => AntigravityProbeResult::Failed { message },
    }
}

fn version_of(path: &Path) -> Result<String, String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| format!("could not run {} --version: {e}", path.display()))?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(version) = parse_build_version(&text) {
        return Ok(version);
    }
    if output.status.success() {
        // Binary runs; version string optional.
        return Ok(PINNED_VERSION.into());
    }
    Err(format!(
        "{} --version failed ({})",
        path.display(),
        output.status
    ))
}

/// `Build label: 1.4.4` or legacy `Build label: agy_acp_server_1.1.1`.
pub fn parse_build_version(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Build label: "))
        .map(|label| label.trim_start_matches("agy_acp_server_").to_string())
        .filter(|v| !v.is_empty())
}

/// Curated Antigravity ACP catalog — keep aligned with
/// https://antigravity.google/docs/models/ (and what `pi --list-models`
/// reports under the `antigravity` provider).
pub fn default_models() -> Vec<AntigravityModelInfo> {
    vec![
        AntigravityModelInfo {
            id: "gemini-4-argon".into(),
            display_name: "Gemini 4 Argon".into(),
        },
        AntigravityModelInfo {
            id: "gemini-3.8-flash".into(),
            display_name: "Gemini 3.8 Flash".into(),
        },
        AntigravityModelInfo {
            id: "claude-opus-5-5".into(),
            display_name: "Claude Opus 5.5".into(),
        },
        AntigravityModelInfo {
            id: "claude-sonnet-5-5".into(),
            display_name: "Claude Sonnet 5.5".into(),
        },
        AntigravityModelInfo {
            id: "gpt-oss-120b".into(),
            display_name: "GPT-OSS-120B".into(),
        },
    ]
}

pub fn gemini_home() -> PathBuf {
    resolve_gemini_home(
        std::env::var("GEMINI_HOME").ok().as_deref(),
        &default_gemini_home(),
    )
}

fn default_gemini_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gemini")
}

/// `~/.gemini` when this user can read and update `antigravity-acp` there.
/// Otherwise a directory under Application Support that we can create.
///
/// A previous root-owned run leaves `~/.gemini/antigravity-acp/settings.json`
/// mode 600. The ACP server then accepts `authenticate` (keychain token, no
/// browser) and rejects `session/new` because it cannot record `auth.type`.
fn resolve_gemini_home(explicit: Option<&str>, default_home: &Path) -> PathBuf {
    if let Some(home) = explicit.map(str::trim).filter(|home| !home.is_empty()) {
        return PathBuf::from(home);
    }
    if acp_state_usable(default_home) {
        return default_home.to_path_buf();
    }
    let path = fallback_gemini_home();
    tracing::warn!(
        target: "antigravity",
        default = %default_home.display(),
        fallback = %path.display(),
        "Antigravity state directory is not usable by this user; using a writable Gemini home"
    );
    path
}

fn fallback_gemini_home() -> PathBuf {
    let path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("halo-agent")
        .join("gemini");
    let _ = std::fs::create_dir_all(&path);
    path
}

/// True when the server can create or update `antigravity-acp/settings.json`.
fn acp_state_usable(home: &Path) -> bool {
    let dir = home.join("antigravity-acp");
    if dir.is_dir() {
        return directory_writable(&dir) && settings_readable(&dir);
    }
    if home.is_dir() {
        return directory_writable(home);
    }
    home.parent()
        .is_some_and(|parent| parent.is_dir() && directory_writable(parent))
}

fn settings_readable(dir: &Path) -> bool {
    let settings = dir.join("settings.json");
    match std::fs::metadata(&settings) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
        Ok(_) => std::fs::File::open(&settings).is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_gemini_home_is_used_as_given() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = resolve_gemini_home(Some(dir.path().to_str().unwrap()), Path::new("/nope"));
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn writable_default_home_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = resolve_gemini_home(None, dir.path());
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn unreadable_settings_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let acp = dir.path().join("antigravity-acp");
        std::fs::create_dir(&acp).unwrap();
        let settings = acp.join("settings.json");
        std::fs::write(&settings, "{}").unwrap();
        assert!(acp_state_usable(dir.path()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&settings).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&settings, perms.clone()).unwrap();
            assert!(!acp_state_usable(dir.path()));
            let resolved = resolve_gemini_home(None, dir.path());
            assert_ne!(resolved, dir.path());
            assert!(resolved.ends_with("gemini"));
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&settings, perms);
        }
    }
}

fn directory_writable(dir: &Path) -> bool {
    let probe = dir.join(".halo-write-probe");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

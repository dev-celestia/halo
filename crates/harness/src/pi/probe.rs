//! Deterministic executable/Node discovery without shell startup files or global env writes.
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::process;
use super::types::{PiModelInfo, PiProbeResult, PiRunOptions, REQUIRED_VERSION};

fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn standard_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.extend([
            home.join(".halo/pi/bin"),
            home.join(".npm-global/bin"),
            home.join(".local/bin"),
            home.join(".pi/bin"),
        ]);
        if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            let mut nvm: Vec<_> = entries.flatten().map(|e| e.path().join("bin")).collect();
            nvm.sort();
            nvm.reverse();
            dirs.extend(nvm);
        }
    }
    dirs
}

fn search(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|p| p.join(name)).find(|p| executable(p))
}

/// Every executable `name` under `dirs`, first occurrence of each path wins.
fn collect(name: &str, dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in dirs {
        let path = dir.join(name);
        if executable(&path) && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

/// Prefer the pinned Pi over an older binary that appears earlier on `PATH`.
///
/// Homebrew's `/opt/homebrew/bin/pi` is often 1.0.2 and is searched first. A
/// 1.4.0 install under `~/.halo/pi/bin` must win, or Upgrade looks like it
/// did nothing.
pub(crate) fn prefer_required_version(
    candidates: &[(PathBuf, Option<String>)],
) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|(_, version)| version.as_deref() == Some(REQUIRED_VERSION))
        .or_else(|| candidates.first())
        .map(|(path, _)| path.clone())
}

fn version_of(executable: &Path, path: &OsStr) -> Option<String> {
    let mut dirs = Vec::new();
    if let Some(parent) = executable.parent() {
        dirs.push(parent.to_path_buf());
    }
    dirs.extend(std::env::split_paths(path).filter(|dir| dir.is_absolute()));
    dirs.extend(standard_dirs());
    let node = node_interpreter(executable, &dirs).ok().flatten();
    let mut cmd = if let Some(node) = node {
        let mut cmd = Command::new(node);
        cmd.arg(executable);
        cmd
    } else {
        Command::new(executable)
    };
    process::version(&mut cmd).ok()
}

fn resolve(custom: Option<&str>, path: &OsStr) -> Result<PathBuf, String> {
    let explicit = custom
        .map(str::to_owned)
        .or_else(|| std::env::var("PI_EXECUTABLE").ok());
    if let Some(explicit) = explicit {
        let candidate = PathBuf::from(explicit);
        if !candidate.is_absolute() || !executable(&candidate) {
            return Err(
                "Configured Pi executable must be an existing, executable absolute file path"
                    .into(),
            );
        }
        return Ok(candidate);
    }
    let mut dirs: Vec<_> = std::env::split_paths(path)
        .filter(|p| p.is_absolute())
        .collect();
    dirs.extend(standard_dirs());
    let found = collect("pi", &dirs);
    if found.is_empty() {
        return Err(
            "Pi executable not found; install @earendil-works/pi-coding-agent 1.4.0 or configure its absolute path"
                .into(),
        );
    }
    let ranked: Vec<_> = found
        .into_iter()
        .map(|candidate| {
            let version = version_of(&candidate, path);
            (candidate, version)
        })
        .collect();
    prefer_required_version(&ranked).ok_or_else(|| {
        "Pi executable not found; install @earendil-works/pi-coding-agent 1.4.0 or configure its absolute path"
            .into()
    })
}

/// Invalid explicit paths never silently fall back to another installation.
pub fn resolve_executable(custom_path: Option<&str>) -> Option<PathBuf> {
    resolve(custom_path, &std::env::var_os("PATH").unwrap_or_default()).ok()
}

/// The Halo browser CLI, when it is installed.
///
/// Advertising a browser the agent cannot actually run is worse than saying
/// nothing, so the prompt block is only appended when this resolves.
pub fn browser_cli() -> Option<PathBuf> {
    halo_browser_tools::agent_browser::resolve_cli()
}

pub(crate) struct Launch {
    pub executable: PathBuf,
    node: Option<PathBuf>,
    path: OsString,
}

impl Launch {
    pub fn resolve(custom: Option<&str>, path: Option<&str>) -> Result<Self, String> {
        let path = path
            .map(OsString::from)
            .unwrap_or_else(|| std::env::var_os("PATH").unwrap_or_default());
        let executable = resolve(custom, &path)?;
        let mut dirs = Vec::new();
        if let Some(parent) = executable.parent() {
            dirs.push(parent.to_path_buf());
        }
        dirs.extend(std::env::split_paths(&path).filter(|p| p.is_absolute()));
        dirs.extend(standard_dirs());
        dirs.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        let mut unique = Vec::new();
        for dir in dirs {
            if !unique.contains(&dir) {
                unique.push(dir);
            }
        }
        let node = node_interpreter(&executable, &unique)?;
        // Ensure tools using /usr/bin/env node see the very same interpreter.
        if let Some(parent) = node.as_ref().and_then(|n| n.parent()) {
            unique.retain(|p| p != parent);
            unique.insert(0, parent.to_path_buf());
        }
        let path = std::env::join_paths(unique).map_err(|_| "Cannot construct child PATH")?;
        Ok(Self {
            executable,
            node,
            path,
        })
    }

    fn base_command(&self) -> Command {
        if let Some(node) = &self.node {
            let mut cmd = Command::new(node);
            cmd.arg(&self.executable);
            cmd
        } else {
            Command::new(&self.executable)
        }
    }

    pub fn command(&self) -> Command {
        let mut command = self.base_command();
        apply_environment(&mut command, &self.path, None);
        command
    }

    pub fn options_command(&self, options: &PiRunOptions) -> Command {
        let mut cmd = self.base_command();
        cmd.current_dir(&options.cwd);
        apply_environment(&mut cmd, &self.path, Some(options));
        cmd
    }
}

/// Snapshot the parent environment, then apply this launch's overrides.
/// `env_clear` keeps a later process-global mutation from leaking into a child
/// whose command was already built, including a sibling chat's credentials.
fn apply_environment(cmd: &mut Command, path: &OsStr, options: Option<&PiRunOptions>) {
    let mut env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    env_set(&mut env, "PATH", path.to_os_string());
    env_set(&mut env, "FORCE_COLOR", OsString::from("0"));
    env_set(&mut env, "NO_COLOR", OsString::from("1"));
    if let Some(options) = options {
        for (key, value) in &options.extra_env {
            if key != "PATH" {
                env_set(&mut env, key, OsString::from(value));
            }
        }
        for (key, value) in [
            ("ANTHROPIC_API_KEY", &options.anthropic_api_key),
            ("OPENAI_API_KEY", &options.openai_api_key),
            ("GEMINI_API_KEY", &options.gemini_api_key),
            ("DEEPSEEK_API_KEY", &options.deepseek_api_key),
            ("OLLAMA_BASE_URL", &options.ollama_base_url),
        ] {
            match value {
                None => {}
                Some(value) if value.is_empty() => env_remove(&mut env, key),
                Some(value) => env_set(&mut env, key, OsString::from(value)),
            }
        }
    }
    cmd.env_clear();
    for (key, value) in env {
        cmd.env(key, value);
    }
}

fn env_set(env: &mut Vec<(OsString, OsString)>, key: &str, value: OsString) {
    let key = OsString::from(key);
    env.retain(|(existing, _)| existing != &key);
    env.push((key, value));
}

fn env_remove(env: &mut Vec<(OsString, OsString)>, key: &str) {
    let key = OsString::from(key);
    env.retain(|(existing, _)| existing != &key);
}

fn node_interpreter(file: &Path, dirs: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    let mut bytes = [0; 256];
    let count = std::fs::File::open(file)
        .and_then(|mut f| f.read(&mut bytes))
        .map_err(|_| "Cannot read Pi executable")?;
    let header = String::from_utf8_lossy(&bytes[..count]);
    let line = header.lines().next().unwrap_or_default();
    if !line.starts_with("#!") {
        return Ok(None);
    }
    let words: Vec<_> = line[2..].split_whitespace().collect();
    if words.iter().any(|s| *s == "node" || s.ends_with("/node")) {
        let direct = words
            .first()
            .filter(|s| s.ends_with("/node"))
            .map(PathBuf::from);
        let node = direct
            .filter(|p| executable(p))
            .or_else(|| search("node", dirs));
        return node.map(Some).ok_or_else(|| "Pi requires Node.js, but no executable node was found beside Pi, in PATH, Homebrew, or NVM".into());
    }
    Ok(None)
}

pub fn probe_environment(custom_path: Option<&str>) -> PiProbeResult {
    let launch = match Launch::resolve(custom_path, None) {
        Ok(launch) => launch,
        Err(error) if error.contains("requires Node.js") => return PiProbeResult::NodeMissing,
        Err(error) if custom_path.is_some() || std::env::var_os("PI_EXECUTABLE").is_some() => {
            return PiProbeResult::ProbeFailed(error);
        }
        Err(_) => {
            let mut dirs: Vec<_> =
                std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
            dirs.extend(standard_dirs());
            return PiProbeResult::BinaryNotFound {
                node_available: search("node", &dirs).is_some(),
            };
        }
    };
    match process::version(&mut launch.command()) {
        Ok(version) if version == REQUIRED_VERSION => PiProbeResult::Installed {
            version,
            path: launch.executable,
        },
        Ok(found) => PiProbeResult::WrongVersion {
            found,
            path: launch.executable,
        },
        Err(error) => PiProbeResult::ProbeFailed(error),
    }
}

/// True only when a resolvable Pi reports the pinned [`REQUIRED_VERSION`].
pub fn is_installed(custom_path: Option<&str>) -> bool {
    probe_environment(custom_path).is_ready()
}

/// The model catalog Pi itself will serve, via `pi --list-models`.
///
/// Pi filters this table by *credential readiness*: a provider whose key or
/// OAuth session is missing never appears. That is exactly what the picker
/// needs — the curated [`super::models::default_models`] list advertises
/// Claude, GPT and Gemini models that a Pi install without those keys cannot
/// run, so choosing one produced `No API key found for the selected model`
/// (or a bare `Unauthorized`) after the turn had already started.
///
/// `Some` means the CLI answered; an empty vector is a real answer ("nothing
/// is signed in"), not a failure. `None` means the question could not be
/// asked, and the caller should fall back to the curated list.
pub fn list_models(custom_path: Option<&str>) -> Option<Vec<PiModelInfo>> {
    let launch = Launch::resolve(custom_path, None).ok()?;
    let mut cmd = launch.command();
    cmd.arg("--list-models");
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_list_models(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// `provider  model  context  max-out  thinking  images` rows.
///
/// Model IDs never contain spaces, so whitespace splitting is exact; a row
/// without all six columns is the header or a warning line and is skipped.
fn parse_list_models(stdout: &str) -> Vec<PiModelInfo> {
    stdout
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 6 {
                return None;
            }
            let provider = fields[0];
            let model = fields[1];
            if provider == "provider" || provider.starts_with('-') {
                return None;
            }
            Some(PiModelInfo {
                id: format!("{provider}/{model}"),
                display_name: model.to_string(),
                provider: provider.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_pinned_pi_wins_over_an_older_binary_found_first() {
        let older = PathBuf::from("/opt/homebrew/bin/pi");
        let pinned = PathBuf::from("/Users/me/.halo/pi/bin/pi");
        let picked = prefer_required_version(&[
            (older, Some("1.0.2".into())),
            (pinned.clone(), Some(REQUIRED_VERSION.into())),
        ]);
        assert_eq!(picked, Some(pinned));
    }

    #[test]
    fn without_a_pinned_binary_the_first_hit_is_kept() {
        let older = PathBuf::from("/opt/homebrew/bin/pi");
        let picked = prefer_required_version(&[(older.clone(), Some("1.0.2".into()))]);
        assert_eq!(picked, Some(older));
    }

    #[test]
    fn missing_node_is_actionable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi");
        std::fs::write(&path, "#!/usr/bin/env node\n").unwrap();
        assert!(
            node_interpreter(&path, &[])
                .unwrap_err()
                .contains("requires Node.js")
        );
    }

    /// The real 1.4.0 table, including its header and a warning line.
    const TABLE: &str = "provider     model              context  max-out  thinking  images\n\
antigravity  claude-opus-5-5    1M       128K     yes       yes   \n\
antigravity  gpt-oss-120b       131.1K   32.8K    yes       no    \n";

    #[test]
    fn list_models_skips_the_header_and_keys_by_provider() {
        let models = parse_list_models(TABLE);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "antigravity/claude-opus-5-5");
        assert_eq!(models[0].display_name, "claude-opus-5-5");
        assert_eq!(models[0].provider, "antigravity");
        assert_eq!(models[1].id, "antigravity/gpt-oss-120b");
    }

    #[test]
    fn list_models_ignores_warnings_and_blank_lines() {
        let stdout = format!("Warning: something happened\n\n{TABLE}\n");
        assert_eq!(parse_list_models(&stdout).len(), 2);
    }

    /// Nothing signed in is an answer, not a reason to resurrect the curated
    /// list of models this install cannot run.
    #[test]
    fn an_empty_table_yields_no_models() {
        assert!(parse_list_models("provider     model              context\n").is_empty());
    }
}

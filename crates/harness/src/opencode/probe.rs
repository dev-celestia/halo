//! Resolve and probe the `opencode` binary. Halo speaks OpenCode **v2** only
//! (server API `/api/*`); require OpenCode ≥ 2.2.0.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::types::{OpenCodeModelInfo, OpenCodeProbeResult};

/// Minimum OpenCode release that speaks the v2 HTTP API Halo drives.
pub const MIN_VERSION: (u64, u64, u64) = (2, 2, 0);

/// Embedded in the failure text when OpenCode cannot write its own state
/// directory. [`probe_environment`] turns this into
/// [`OpenCodeProbeResult::StateBlocked`] so the UI offers the ownership repair
/// instead of a reinstall that cannot help.
pub const STATE_BLOCKED_MARKER: &str = "state directory is not writable";

fn explicit_executable(custom: Option<&str>) -> Option<PathBuf> {
    if let Some(custom) = custom.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(custom);
        if path.is_absolute() && path.is_file() {
            return Some(path);
        }
    }
    if let Ok(env) = std::env::var("OPENCODE_EXECUTABLE") {
        let trimmed = env.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if path.is_absolute() && path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

pub fn resolve_executable(custom: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = explicit_executable(custom) {
        return Some(path);
    }
    let candidates = candidate_binaries("opencode");
    if candidates.len() <= 1 {
        return candidates.into_iter().next();
    }
    // Prefer a ready (≥ MIN_VERSION) binary when several exist — an old
    // `/usr/local/bin/opencode` must not shadow `~/.opencode/bin` after a v2 install.
    prefer_ready_or_first(candidates).map(|(path, _)| path)
}

fn candidate_binaries(name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_absolute() && p.is_file() && !out.iter().any(|e| e == &p) {
            out.push(p);
        }
    };

    // Managed curl install first — where OpenCode 2 lands by default.
    if let Some(home) = dirs::home_dir() {
        push(home.join(".opencode").join("bin").join(name));
    }

    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path).filter(|dir| dir.is_absolute()) {
            push(dir.join(name));
            #[cfg(windows)]
            push(dir.join(format!("{name}.exe")));
        }
    }

    if let Some(home) = dirs::home_dir() {
        push(home.join(".local").join("bin").join(name));
        push(home.join(".npm-global").join("bin").join(name));
    }
    push(PathBuf::from("/opt/homebrew/bin").join(name));
    push(PathBuf::from("/usr/local/bin").join(name));
    out
}

fn prefer_ready_or_first(
    candidates: Vec<PathBuf>,
) -> Option<(PathBuf, Result<String, String>)> {
    let mut first = None;
    for path in candidates {
        let ver_res = version_of(&path);
        let is_ready = ver_res
            .as_ref()
            .ok()
            .and_then(|raw| parse_semver(raw))
            .is_some_and(|ver| ver >= MIN_VERSION);
        if is_ready {
            return Some((path, ver_res));
        }
        if first.is_none() {
            first = Some((path, ver_res));
        }
    }
    first
}

pub fn is_installed(custom: Option<&str>) -> bool {
    matches!(probe_environment(custom), OpenCodeProbeResult::Ready { .. })
}

pub fn probe_environment(custom: Option<&str>) -> OpenCodeProbeResult {
    let (path, ver_res) = if let Some(path) = explicit_executable(custom) {
        let res = version_of(&path);
        (path, res)
    } else {
        let Some(found) = prefer_ready_or_first(candidate_binaries("opencode")) else {
            return OpenCodeProbeResult::Missing;
        };
        found
    };
    match ver_res {
        Ok(raw) => match parse_semver(&raw) {
            Some(ver) if ver >= MIN_VERSION => OpenCodeProbeResult::Ready { version: raw, path },
            Some(ver) => OpenCodeProbeResult::WrongVersion {
                found: format!("{}.{}.{}", ver.0, ver.1, ver.2),
                path,
                required: format!("{}.{}.{}", MIN_VERSION.0, MIN_VERSION.1, MIN_VERSION.2),
            },
            None => OpenCodeProbeResult::Failed {
                message: format!("could not parse OpenCode version from `{raw}`"),
            },
        },
        Err(message) => {
            if message.contains(STATE_BLOCKED_MARKER) {
                OpenCodeProbeResult::StateBlocked { message, path }
            } else {
                OpenCodeProbeResult::Failed { message }
            }
        }
    }
}

fn version_of(path: &Path) -> Result<String, String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version");
    let output = crate::pi::process::output_bounded(
        &mut cmd,
        std::time::Duration::from_secs(5),
        64 * 1024,
    )
    .map_err(|e| format!("could not run opencode --version: {e}"))?;
    if !output.status.success() {
        return Err(failure_message(&output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("unknown")
        .to_string();
    Ok(line)
}

/// Explain a failed `opencode --version`.
///
/// OpenCode reports startup failures — an unwritable state directory, a
/// corrupted config — as a Bun `PlatformError` on **stdout**, then exits 1
/// before printing any version. Reporting only the exit code reads like
/// "not installed" and sends people to the installer for a binary that is
/// already there, so keep the first meaningful line and name the known cause.
fn failure_message(output: &std::process::Output) -> String {
    let detail = [&output.stdout, &output.stderr]
        .into_iter()
        .find_map(|stream| {
            String::from_utf8_lossy(stream)
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(|line| line.chars().take(240).collect::<String>())
        });

    match detail {
        Some(line) if is_permission_failure(&line) => {
            // Name the directory so the line is actionable even without the UI,
            // and point at the button that exists on this very card.
            let at = state_dir()
                .map(|dir| format!(" ({})", dir.display()))
                .unwrap_or_default();
            format!(
                "OpenCode can't start: its {STATE_BLOCKED_MARKER}{at}. Click Fix Ownership — reinstalling won't help (opencode --version {}).",
                output.status
            )
        }
        Some(line) => format!("opencode --version failed ({}): {line}", output.status),
        None => format!("opencode --version failed ({})", output.status),
    }
}

/// OpenCode follows XDG on every platform, macOS included — `dirs::data_local_dir()`
/// would point at `~/Library/Application Support`, which it never writes to.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(xdg).join("opencode"));
    }
    dirs::home_dir().map(|home| home.join(".local/share/opencode"))
}

/// Bun reports a refused `open` as `PlatformError: PermissionDenied` plus the
/// underlying `EACCES`.
fn is_permission_failure(line: &str) -> bool {
    line.contains("EACCES")
        || line.contains("PermissionDenied")
        || line.contains("permission denied")
}

/// First `x.y.z` triple found in the version string.
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let mut nums = Vec::new();
    let mut cur = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            nums.push(cur.parse().ok()?);
            cur.clear();
            if nums.len() == 3 {
                break;
            }
        }
    }
    if !cur.is_empty() && nums.len() < 3 {
        nums.push(cur.parse().ok()?);
    }
    match nums.as_slice() {
        [a, b, c, ..] => Some((*a, *b, *c)),
        [a, b] => Some((*a, *b, 0)),
        [a] => Some((*a, 0, 0)),
        _ => None,
    }
}

/// `provider/model` → whether it accepts image input, from OpenCode's
/// models.dev cache (`~/.cache/opencode/models.json`).
///
/// Cheap enough to call during a catalog refresh: one file read and parse.
/// An empty map means the cache is missing or unreadable; callers treat that
/// as "unknown" rather than "no images".
pub fn model_image_support() -> HashMap<String, bool> {
    for path in image_support_cache_candidates() {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let support = parse_model_image_support(&raw);
        if !support.is_empty() {
            return support;
        }
    }
    HashMap::new()
}

/// Models.dev cache files, freshest first. Halo can run as root (launched via
/// `sudo`) while OpenCode keeps its cache in the logged-in user's home, so the
/// environment paths alone are not always enough; on macOS also look at every
/// user home and prefer the most recently written cache.
fn image_support_cache_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut push = |path: PathBuf| {
        if !paths.contains(&path) {
            paths.push(path);
        }
    };
    for root in [
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")),
        dirs::home_dir().map(|home| home.join(".cache")),
    ]
    .into_iter()
    .flatten()
    {
        push(root.join("opencode/models.json"));
    }
    #[cfg(target_os = "macos")]
    if let Ok(entries) = std::fs::read_dir("/Users") {
        for entry in entries.flatten() {
            push(entry.path().join(".cache/opencode/models.json"));
        }
    }
    newest_first(paths.into_iter().filter(|path| path.is_file()).collect())
}

/// Sort cache paths newest first; a fresh cache from any user home is more
/// trustworthy than a stale one under the current `HOME`.
fn newest_first(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort_by_cached_key(|path| {
        std::cmp::Reverse(
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok(),
        )
    });
    paths
}

fn parse_model_image_support(raw: &str) -> HashMap<String, bool> {
    let Ok(catalog) = serde_json::from_str::<serde_json::Value>(raw) else {
        return HashMap::new();
    };
    let mut support = HashMap::new();
    let Some(providers) = catalog.as_object() else {
        return support;
    };
    for (provider_id, provider) in providers {
        let Some(models) = provider.get("models").and_then(|models| models.as_object()) else {
            continue;
        };
        for (model_id, model) in models {
            let Some(inputs) = model
                .pointer("/modalities/input")
                .and_then(|inputs| inputs.as_array())
            else {
                continue;
            };
            support.insert(
                format!("{provider_id}/{model_id}"),
                inputs.iter().any(|kind| kind.as_str() == Some("image")),
            );
        }
    }
    support
}

/// Best-effort model list via `opencode models` (provider/id lines).
///
/// Every candidate binary is tried in turn, not just the one
/// [`resolve_executable`] prefers: an OpenCode 2 binary answers `--version`
/// happily but fails `models` when its *managed service* cannot start (port
/// already taken, stale service lock). One bad binary must not cost the user
/// their whole model list, so a failure falls through to the next candidate.
pub fn list_models(custom: Option<&str>) -> Vec<OpenCodeModelInfo> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(bin) = explicit_executable(custom) {
        candidates.push(bin);
    }
    for path in candidate_binaries("opencode") {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    candidates
        .iter()
        .find_map(|bin| models_from(bin))
        .unwrap_or_else(default_models)
}

/// `opencode models` from one binary, or `None` when it cannot answer.
fn models_from(bin: &Path) -> Option<Vec<OpenCodeModelInfo>> {
    let mut cmd = Command::new(bin);
    cmd.arg("models");
    let output = crate::pi::process::output_bounded(
        &mut cmd,
        std::time::Duration::from_secs(10),
        256 * 1024,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let images = model_image_support();
    let mut models: Vec<OpenCodeModelInfo> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let id = line.trim();
            if id.is_empty() {
                return None;
            }
            let display_name = id
                .rsplit_once('/')
                .map(|(_, name)| name.to_string())
                .unwrap_or_else(|| id.to_string());
            Some(OpenCodeModelInfo {
                supports_images: images.get(id).copied(),
                id: id.to_string(),
                display_name,
            })
        })
        .collect();
    if models.is_empty() {
        return None;
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Some(models)
}

pub fn default_models() -> Vec<OpenCodeModelInfo> {
    vec![
        OpenCodeModelInfo {
            id: "anthropic/claude-sonnet-4-5".into(),
            display_name: "Claude Sonnet 4.5".into(),
            supports_images: Some(true),
        },
        OpenCodeModelInfo {
            id: "openai/gpt-5".into(),
            display_name: "GPT-5".into(),
            supports_images: Some(true),
        },
        OpenCodeModelInfo {
            id: "google/gemini-2.5-pro".into(),
            display_name: "Gemini 2.5 Pro".into(),
            supports_images: Some(true),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_semver_reads_leading_triple() {
        assert_eq!(parse_semver("2.2.0"), Some((2, 2, 0)));
        assert_eq!(parse_semver("opencode 1.2.24"), Some((1, 2, 24)));
        assert_eq!(parse_semver("v2.3.1-beta"), Some((2, 3, 1)));
    }

    #[test]
    fn model_image_support_reads_models_dev_modalities() {
        let raw = r#"{
            "opencode": { "models": {
                "beam-free": { "modalities": { "input": ["text"] } },
                "nemotron-vl": { "modalities": { "input": ["text", "image"] } },
                "no-modalities": {}
            }},
            "broken": {}
        }"#;
        let support = parse_model_image_support(raw);
        assert_eq!(support.get("opencode/beam-free"), Some(&false));
        assert_eq!(support.get("opencode/nemotron-vl"), Some(&true));
        assert!(!support.contains_key("opencode/no-modalities"));
        assert!(parse_model_image_support("{").is_empty());
    }

    #[test]
    fn min_version_rejects_v1() {
        assert!(parse_semver("1.2.24").unwrap() < MIN_VERSION);
        assert!(parse_semver("2.2.0").unwrap() >= MIN_VERSION);
    }

    #[test]
    fn explicit_executable_rejects_relative_paths() {
        assert_eq!(explicit_executable(Some("./opencode")), None);
        assert_eq!(explicit_executable(Some("Cargo.toml")), None);
    }
}

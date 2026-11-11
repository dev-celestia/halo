//! Cursor SDK driver via a Halo-owned Node shim.
//!
//! Spawns `node <shim>` against a pinned `@cursor/sdk` install under the app
//! data dir, speaking JSONL over stdio. Auth is the SDK's own credentials
//! (`CURSOR_API_KEY` or prior SDK login) — separate from `cursor-agent login`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::types::{AgentEvent, DoneStatus, RunRequest, TodoItem, ToolCall};
use halo_core::paths::get_shared_app_dir;

const CURSOR_SDK_NAME: &str = "@cursor/sdk";
const CURSOR_SDK_VERSION: &str = "1.0.32";
const SHIM_NAME: &str = "halo-cursor-shim.mjs";
const SHIM_SOURCE: &str = include_str!("shim.mjs");
const OK_MARKER: &str = ".halo-install-ok";

pub fn default_models() -> Vec<(String, String)> {
    vec![
        ("auto".into(), "Auto".into()),
        ("composer-1".into(), "Composer 1".into()),
        ("gpt-5.4".into(), "GPT-5.4".into()),
        ("claude-4.6-sonnet".into(), "Claude 4.6 Sonnet".into()),
        ("claude-4.6-opus".into(), "Claude 4.6 Opus".into()),
    ]
}

const NPM_INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(20);

/// Live catalog from `node <shim> models`, with [`default_models`] if discovery fails.
pub fn discover_models() -> Vec<(String, String)> {
    let fallback = default_models();
    if !sdk_ready() {
        return fallback;
    }
    match list_models_from_shim() {
        Ok(mut models) if !models.is_empty() => {
            if let Some(index) = models.iter().position(|(id, _)| id == "auto") {
                if index != 0 {
                    let auto = models.remove(index);
                    models.insert(0, auto);
                }
            }
            models
        }
        _ => fallback,
    }
}

/// SDK installed and a user key is present. The setup card stays up until both are true.
pub fn session_ready() -> (bool, Option<String>) {
    if !sdk_ready() {
        return (
            false,
            Some("Cursor harness needs Node.js and the pinned @cursor/sdk. Click Install.".into()),
        );
    }
    if !auth_status().connected {
        return (
            false,
            Some(
                "Cursor SDK is installed. Set CURSOR_API_KEY or paste a user key into Halo cursor-state/auth.json."
                    .into(),
            ),
        );
    }
    (true, None)
}

/// Resolve `name` on PATH, then the usual macOS/Linux install prefixes.
///
/// Finder/Dock launches give Halo a minimal `PATH`, so Homebrew / nvm / fnm /
/// Volta installs are often invisible to a bare `which`. Doctor already probes
/// these locations; Cursor install must match or "Node.js not found" appears
/// even when Terminal can run `node`.
fn which(name: &str) -> Option<PathBuf> {
    for candidate in path_candidates(name) {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for candidate in well_known_candidates(name) {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn path_candidates(name: &str) -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dir in std::env::split_paths(&path) {
        out.push(dir.join(name));
        #[cfg(windows)]
        out.push(dir.join(format!("{name}.exe")));
    }
    out
}

fn well_known_candidates(name: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
        dirs.push(home.join(".bun").join("bin"));
        dirs.push(home.join(".volta").join("bin"));
        dirs.push(home.join(".local").join("share").join("pnpm"));
        #[cfg(target_os = "macos")]
        dirs.push(home.join("Library").join("pnpm"));
        dirs.push(home.join(".fnm").join("current").join("bin"));
        #[cfg(target_os = "macos")]
        dirs.push(
            home.join("Library")
                .join("Application Support")
                .join("fnm")
                .join("aliases")
                .join("default")
                .join("bin"),
        );
        dirs.push(home.join(".asdf").join("shims"));
        dirs.push(home.join(".local").join("share").join("mise").join("shims"));

        let nvm_nodes = home.join(".nvm").join("versions").join("node");
        if let Ok(entries) = std::fs::read_dir(&nvm_nodes) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_dir())
                .collect();
            versions.sort();
            if let Some(latest) = versions.last() {
                dirs.push(latest.join("bin"));
            }
        }
    }

    let mut out = Vec::new();
    for dir in dirs {
        out.push(dir.join(name));
        #[cfg(windows)]
        {
            out.push(dir.join(format!("{name}.exe")));
            out.push(dir.join(format!("{name}.cmd")));
        }
    }
    out
}

pub fn resolve_node() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("NODE_EXECUTABLE") {
        let p = PathBuf::from(override_path.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    which("node")
}

fn resolve_npm() -> Option<PathBuf> {
    // Prefer npm next to the resolved node binary (nvm/fnm/Homebrew layouts).
    if let Some(node) = resolve_node() {
        if let Some(dir) = node.parent() {
            let sibling = dir.join("npm");
            if sibling.is_file() {
                return Some(sibling);
            }
            #[cfg(windows)]
            {
                let sibling = dir.join("npm.cmd");
                if sibling.is_file() {
                    return Some(sibling);
                }
                let sibling = dir.join("npm.exe");
                if sibling.is_file() {
                    return Some(sibling);
                }
            }
        }
    }
    which("npm")
}

fn adapters_root() -> PathBuf {
    if let Ok(dir) = std::env::var("HALO_ADAPTERS_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    get_shared_app_dir().join("adapters")
}

fn install_dir() -> PathBuf {
    adapters_root().join("cursor__sdk").join(CURSOR_SDK_VERSION)
}

fn state_root() -> PathBuf {
    if let Ok(dir) = std::env::var("HALO_CURSOR_STATE_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    get_shared_app_dir().join("cursor-state")
}

fn auth_store_path() -> PathBuf {
    state_root().join("auth.json")
}

/// SDK credential file shape under `cursor-state/auth.json`.
/// Extra JSON fields (e.g. `backendUrl`) are ignored on read.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSdkCredentials {
    version: u32,
    api_key: String,
    #[serde(default)]
    api_key_expires_at_ms: Option<u64>,
    #[serde(default)]
    email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorAuthStatus {
    pub connected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn load_stored() -> Option<StoredSdkCredentials> {
    let path = auth_store_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    let creds: StoredSdkCredentials = serde_json::from_str(&raw).ok()?;
    if creds.version != 1 || creds.api_key.trim().is_empty() {
        return None;
    }
    if let Some(expires) = creds.api_key_expires_at_ms {
        if expires > 0 && expires < now_ms() {
            return None;
        }
    }
    Some(creds)
}

/// Never returns the key — only connected flag + optional email.
pub fn auth_status() -> CursorAuthStatus {
    match load_stored() {
        Some(c) => CursorAuthStatus {
            connected: true,
            email: c.email.filter(|e| !e.is_empty()),
        },
        None => {
            // Process env alone also counts as connected for the UI signal.
            let env_set = std::env::var_os("CURSOR_API_KEY")
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            CursorAuthStatus {
                connected: env_set,
                email: None,
            }
        }
    }
}

fn drain_stderr(stderr: Option<std::process::ChildStderr>) {
    let Some(stderr) = stderr else {
        return;
    };
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            line.clear();
        }
    });
}

fn wait_child(child: &mut Child, timeout: Duration) -> Result<std::process::ExitStatus, String> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {}s", timeout.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn list_models_from_shim() -> Result<Vec<(String, String)>, String> {
    let node = resolve_node().ok_or_else(|| "Node.js not found".to_string())?;
    let dir = install_dir();
    let shim = shim_path(&dir);
    if !shim.is_file() {
        return Err("Cursor shim is not installed".into());
    }
    let mut cmd = Command::new(&node);
    cmd.arg(&shim)
        .arg("models")
        .env("HALO_CURSOR_STATE_DIR", state_root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apply_stored_api_key(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn cursor models: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "cursor models has no stdout".to_string())?;
    let status = wait_child(&mut child, MODEL_LIST_TIMEOUT)?;
    if !status.success() {
        return Err(format!(
            "cursor models exited {}",
            status.code().unwrap_or(-1)
        ));
    }
    let reader = BufReader::new(stdout);
    let mut models = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        let Ok(frame) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if frame.get("ev").and_then(Value::as_str) != Some("models") {
            continue;
        }
        let Some(items) = frame.get("items").and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(id) = item
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            let name = item
                .get("displayName")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or(id);
            models.push((id.to_string(), name.to_string()));
        }
    }
    Ok(models)
}

/// Prefer process env; otherwise inject the Halo-stored key into the child.
fn apply_stored_api_key(cmd: &mut Command) {
    let env_set = std::env::var_os("CURSOR_API_KEY")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if env_set {
        return;
    }
    if let Some(creds) = load_stored() {
        cmd.env("CURSOR_API_KEY", creds.api_key);
    }
}

fn sdk_package_present(dir: &Path) -> bool {
    dir.join("node_modules")
        .join("@cursor")
        .join("sdk")
        .join("package.json")
        .is_file()
}

fn shim_path(dir: &Path) -> PathBuf {
    dir.join(SHIM_NAME)
}

/// Pinned `@cursor/sdk` + shim are on disk (or a test override executable exists).
pub fn sdk_ready() -> bool {
    if let Ok(override_path) = std::env::var("CURSOR_SDK_SHIM_EXECUTABLE") {
        if !override_path.is_empty() {
            return PathBuf::from(override_path).is_file();
        }
    }
    let dir = install_dir();
    dir.join(OK_MARKER).is_file() && sdk_package_present(&dir) && shim_path(&dir).is_file()
}

/// Ensure pinned SDK + shim exist; returns `(node, shim_path)`.
pub fn ensure_shim() -> Result<(PathBuf, PathBuf), String> {
    if let Ok(override_path) = std::env::var("CURSOR_SDK_SHIM_EXECUTABLE") {
        let p = PathBuf::from(&override_path);
        if p.is_file() {
            // Override is a direct executable (tests) — no node wrapper.
            return Ok((p, PathBuf::new()));
        }
        return Err(format!(
            "CURSOR_SDK_SHIM_EXECUTABLE points to missing file: {override_path}"
        ));
    }

    let node = resolve_node().ok_or_else(|| {
        "Node.js not found. Install Node or set NODE_EXECUTABLE to run the Cursor SDK shim."
            .to_string()
    })?;

    let dir = install_dir();
    let marker = dir.join(OK_MARKER);
    let shim = shim_path(&dir);

    if marker.is_file() && sdk_package_present(&dir) && shim.is_file() {
        // Refresh shim bytes if the embedded source changed.
        let current = std::fs::read_to_string(&shim).unwrap_or_default();
        if current != SHIM_SOURCE {
            write_shim(&dir)?;
        }
        return Ok((node, shim));
    }

    install_sdk_and_shim(&node, &dir)?;
    Ok((node, shim_path(&dir)))
}

fn write_shim(dir: &Path) -> Result<(), String> {
    let shim = shim_path(dir);
    let tmp = dir.join(format!("{SHIM_NAME}.tmp-{}", std::process::id()));
    std::fs::write(&tmp, SHIM_SOURCE).map_err(|e| format!("write shim tmp: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    if let Err(e) = std::fs::rename(&tmp, &shim) {
        let _ = std::fs::remove_file(&tmp);
        // Concurrent writer with identical bytes is fine.
        if std::fs::read(&shim).ok().as_deref() != Some(SHIM_SOURCE.as_bytes()) {
            return Err(format!("publish shim: {e}"));
        }
    }
    Ok(())
}

fn install_sdk_and_shim(node: &Path, final_dir: &Path) -> Result<(), String> {
    let npm = resolve_npm().ok_or_else(|| {
        "npm not found. Install Node.js (with npm) to download the pinned @cursor/sdk.".to_string()
    })?;

    let root = adapters_root();
    std::fs::create_dir_all(&root).map_err(|e| format!("create adapters dir: {e}"))?;
    let cache_dir = root.join(".npm-cache");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("create npm cache: {e}"))?;

    let tmp_dir = root.join(format!(
        ".tmp-cursor__sdk-{}-{}",
        CURSOR_SDK_VERSION,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("create tmp install dir: {e}"))?;
    std::fs::write(tmp_dir.join("package.json"), "{\"private\":true}\n")
        .map_err(|e| format!("write package.json: {e}"))?;

    let spec = format!("{CURSOR_SDK_NAME}@{CURSOR_SDK_VERSION}");
    let mut npm_child = Command::new(&npm)
        .args([
            "install",
            "--no-audit",
            "--no-fund",
            "--no-progress",
            "--loglevel=error",
            "--include=optional",
            "--cache",
        ])
        .arg(&cache_dir)
        .arg(&spec)
        .current_dir(&tmp_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn npm ({npm:?}): {e}"))?;
    let status = wait_child(&mut npm_child, NPM_INSTALL_TIMEOUT).map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        format!("npm install {spec} {e}")
    })?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let _ = std::fs::remove_dir_all(&cache_dir);
        return Err(format!(
            "npm install {spec} failed (exit {:?}). Check network and retry.",
            status.code()
        ));
    }

    if !sdk_package_present(&tmp_dir) {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let _ = std::fs::remove_dir_all(&cache_dir);
        return Err(format!(
            "npm install {spec} finished but package is missing under node_modules"
        ));
    }

    write_shim(&tmp_dir)?;
    std::fs::write(tmp_dir.join(OK_MARKER), CURSOR_SDK_VERSION)
        .map_err(|e| format!("write install marker: {e}"))?;

    if let Some(parent) = final_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create install parent: {e}"))?;
    }
    if final_dir.exists() {
        let _ = std::fs::remove_dir_all(final_dir);
    }
    if let Err(e) = std::fs::rename(&tmp_dir, final_dir) {
        // Cross-device rename may fail — fall back to recursive copy.
        copy_dir_recursive(&tmp_dir, final_dir).map_err(|copy_e| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            let _ = std::fs::remove_dir_all(&cache_dir);
            format!("publish SDK install (rename: {e}, copy: {copy_e})")
        })?;
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    let _ = std::fs::remove_dir_all(&cache_dir);

    let _ = node; // kept for future node-sibling npm resolution
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn decode_tool(name: &str, args: &Value) -> ToolCall {
    let path = || {
        let p = str_field(args, "path");
        if p.is_empty() {
            str_field(args, "file_path")
        } else {
            p
        }
    };
    match name {
        "shell" | "Shell" | "bash" | "Bash" | "await" => {
            let command = {
                let cmd = str_field(args, "command");
                if cmd.is_empty() {
                    str_field(args, "cmd")
                } else {
                    cmd
                }
            };
            match super::browser::classify_command(&command) {
                Some(found) => ToolCall::Browser {
                    action: found.action,
                    target: found.target,
                },
                None => ToolCall::Exec { command },
            }
        }
        "read" | "Read" | "readFile" | "ReadFile" => ToolCall::ReadFile { path: path() },
        "write" | "Write" | "writeFile" | "WriteFile" => ToolCall::WriteFile {
            path: path(),
            content: args
                .get("contents")
                .or_else(|| args.get("content"))
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "edit" | "Edit" | "editFile" | "strReplace" | "StrReplace" => ToolCall::EditFile {
            path: path(),
            old_string: args
                .get("old_string")
                .or_else(|| args.get("oldString"))
                .and_then(Value::as_str)
                .map(str::to_string),
            new_string: args
                .get("new_string")
                .or_else(|| args.get("newString"))
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        "grep" | "Grep" | "search" | "Search" => ToolCall::Search {
            pattern: {
                let p = str_field(args, "pattern");
                if p.is_empty() {
                    str_field(args, "query")
                } else {
                    p
                }
            },
            path: {
                let p = path();
                if p.is_empty() { None } else { Some(p) }
            },
        },
        "glob" | "Glob" | "ls" | "LS" => ToolCall::Glob {
            pattern: {
                let p = str_field(args, "glob_pattern");
                if p.is_empty() {
                    let p = str_field(args, "pattern");
                    if p.is_empty() { path() } else { p }
                } else {
                    p
                }
            },
        },
        "todoWrite" | "TodoWrite" | "updateTodos" => ToolCall::Todo {
            items: args
                .get("todos")
                .or_else(|| args.get("items"))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|t| {
                            TodoItem::new(
                                t.get("content")
                                    .or_else(|| t.get("text"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                t.get("status").and_then(Value::as_str).unwrap_or(""),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        "task" | "Task" | "subagent" | "Subagent" => ToolCall::Task {
            description: {
                let d = str_field(args, "description");
                if d.is_empty() {
                    str_field(args, "title")
                } else {
                    d
                }
            },
            agent: args
                .get("agent")
                .or_else(|| args.get("subagent_type"))
                .or_else(|| args.get("subagentType"))
                .and_then(Value::as_str)
                .map(str::to_string),
            prompt: str_field(args, "prompt"),
        },
        "webSearch" | "WebSearch" | "web_search" | "websearch" => ToolCall::WebSearch {
            query: {
                let q = str_field(args, "query");
                if q.is_empty() {
                    str_field(args, "search_term")
                } else {
                    q
                }
            },
        },
        "webFetch" | "WebFetch" | "web_fetch" | "webfetch" => ToolCall::WebFetch {
            url: str_field(args, "url"),
        },
        _ => ToolCall::Unknown {
            name: name.into(),
            input_json: if args.is_null() {
                None
            } else {
                Some(args.to_string())
            },
        },
    }
}

fn map_frame(frame: &Value) -> Vec<AgentEvent> {
    if let Some(parent_id) = frame
        .get("parent")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return map_inner_frame(frame, false)
            .into_iter()
            .map(|event| AgentEvent::SubagentEvent {
                session_id: parent_id.to_string(),
                event: Box::new(event),
            })
            .collect();
    }
    map_inner_frame(frame, true)
}

fn map_inner_frame(frame: &Value, is_root: bool) -> Vec<AgentEvent> {
    match frame.get("ev").and_then(Value::as_str).unwrap_or("") {
        "text" => {
            let text = str_field(frame, "text");
            if text.is_empty() {
                Vec::new()
            } else {
                vec![AgentEvent::TextDelta { text }]
            }
        }
        "thinking" => {
            let text = str_field(frame, "text");
            if text.is_empty() {
                Vec::new()
            } else {
                vec![AgentEvent::ReasoningDelta { text }]
            }
        }
        "tool" => {
            let id = str_field(frame, "id");
            let name = str_field(frame, "name");
            let args = frame.get("args").cloned().unwrap_or(Value::Null);
            let phase = str_field(frame, "phase");
            let is_task = is_root && matches!(name.as_str(), "task" | "Task" | "subagent" | "Subagent");
            if phase == "start" {
                let call = decode_tool(&name, &args);
                if is_task {
                    let (title, agent) = match &call {
                        ToolCall::Task {
                            description,
                            agent,
                            prompt,
                        } => {
                            let t = if !description.trim().is_empty() {
                                description.trim().to_string()
                            } else if !prompt.trim().is_empty() {
                                prompt.lines().next().unwrap_or("Subagent").trim().to_string()
                            } else {
                                "Subagent".to_string()
                            };
                            (t, agent.clone())
                        }
                        _ => ("Subagent".to_string(), None),
                    };
                    vec![
                        AgentEvent::SubagentStarted {
                            parent_tool_call_id: Some(id.clone()),
                            session_id: id.clone(),
                            title,
                            agent,
                        },
                        AgentEvent::ToolCall { id, call },
                    ]
                } else {
                    vec![AgentEvent::ToolCall { id, call }]
                }
            } else {
                let is_error = frame.get("error").and_then(Value::as_bool).unwrap_or(false);
                if is_task {
                    vec![
                        AgentEvent::ToolResult {
                            id: id.clone(),
                            is_error,
                            output: None,
                        },
                        AgentEvent::SubagentDone {
                            session_id: id,
                            status: if is_error {
                                DoneStatus::Errored
                            } else {
                                DoneStatus::Completed
                            },
                            error: None,
                        },
                    ]
                } else {
                    vec![AgentEvent::ToolResult {
                        id,
                        is_error,
                        output: None,
                    }]
                }
            }
        }
        "steered" => vec![AgentEvent::Steered],
        _ => Vec::new(),
    }
}

#[derive(Clone)]
pub struct CursorRun {
    pub steer_tx: Sender<(String, Vec<halo_harness::PiImage>)>,
    pub interrupt: Arc<AtomicBool>,
}

impl CursorRun {
    pub fn spawn(
        request: RunRequest,
        mut on_event: impl FnMut(AgentEvent) + Send + 'static,
    ) -> Result<Self, String> {
        if !Path::new(&request.cwd).is_dir() {
            return Err(format!("Not a directory: {}", request.cwd));
        }

        let (exe, shim) = ensure_shim()?;
        let state_dir = state_root();
        std::fs::create_dir_all(&state_dir).map_err(|e| format!("create cursor state dir: {e}"))?;

        let mut cmd = Command::new(&exe);
        if !shim.as_os_str().is_empty() {
            cmd.arg(&shim);
        }
        cmd.current_dir(&request.cwd)
            .env("HALO_CURSOR_STATE_DIR", &state_dir)
            .env(
                "HALO_BROWSER_SESSION",
                super::browser::session_for(&request.chat_id),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if request.computer_use {
            cmd.env("HALO_DESKTOP_CHAT", &request.chat_id)
                .env("HALO_DESKTOP_SESSION", &request.chat_id)
                .env(
                    "HALO_DESKTOP_HOME",
                    halo_harness::desktop::engine_home().display().to_string(),
                )
                .env("HALO_DESKTOP_RUN_PID", std::process::id().to_string());
            if request.computer_use_headed {
                cmd.env("HALO_DESKTOP_ALLOW_HEADED", "1");
            }
        }
        apply_stored_api_key(&mut cmd);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn Cursor shim ({exe:?}): {e}"))?;
        drain_stderr(child.stderr.take());

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to take Cursor stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to take Cursor stdout".to_string())?;

        let images: Vec<serde_json::Value> = super::images::prepare_images(&request.attachments)?
            .into_iter()
            .map(|img| {
                json!({
                    "data": img.data,
                    "mimeType": img.mime_type,
                })
            })
            .collect();
        let preamble = super::skills_preamble(&request.skills);
        let base_prompt = super::with_skills_preamble(request.prompt.clone(), &preamble);
        let desktop_note = if request.computer_use {
            halo_harness::desktop::prompt_suffix()
        } else {
            String::new()
        };
        let prompt = format!(
            "{base_prompt}{}{desktop_note}",
            halo_harness::browser::prompt_suffix()
        );
        let first = json!({
            "op": "run",
            "prompt": prompt,
            "cwd": request.cwd,
            "model": request.model,
            "mode": if request.plan_mode { "plan" } else { "agent" },
            "resume": request.resume,
            "images": images,
        });
        writeln!(stdin, "{first}").map_err(|e| e.to_string())?;
        stdin.flush().ok();

        let (steer_tx, steer_rx): (
            Sender<(String, Vec<halo_harness::PiImage>)>,
            Receiver<(String, Vec<halo_harness::PiImage>)>,
        ) = mpsc::channel();
        let interrupt = Arc::new(AtomicBool::new(false));
        let interrupt_flag = interrupt.clone();

        let run_id = request.run_id.clone();
        let _join = thread::Builder::new()
            .name("coder-agent-cursor".into())
            .spawn(move || {
                run_loop(
                    child,
                    stdin,
                    stdout,
                    request.cwd,
                    request.model.unwrap_or_default(),
                    run_id,
                    steer_rx,
                    interrupt_flag,
                    &mut on_event,
                );
            })
            .map_err(|e| format!("Failed to spawn Cursor reader thread: {e}"))?;

        Ok(Self {
            steer_tx,
            interrupt,
        })
    }

    pub fn steer_with_images(
        &self,
        prompt: &str,
        images: Vec<halo_harness::PiImage>,
    ) -> Result<(), String> {
        self.steer_tx
            .send((prompt.to_string(), images))
            .map_err(|_| "Agent run is not accepting steers".to_string())
    }

    /// The shim pumps steers as follow-up turns with its own delivery acks,
    /// so the only hard refusal is a dead or interrupted run loop.
    pub fn accepts_steers(&self) -> bool {
        !self.interrupt.load(Ordering::SeqCst)
    }

    pub fn request_interrupt(&self) {
        self.interrupt.store(true, Ordering::SeqCst);
    }
}

fn run_loop(
    mut child: Child,
    mut stdin: ChildStdin,
    stdout: impl std::io::Read + Send + 'static,
    cwd: String,
    request_model: String,
    run_id: String,
    steer_rx: Receiver<(String, Vec<halo_harness::PiImage>)>,
    interrupt: Arc<AtomicBool>,
    on_event: &mut dyn FnMut(AgentEvent),
) {
    let reader = BufReader::new(stdout);
    let mut done = false;
    let mut session_id: Option<String> = None;
    let mut saw_ready = false;
    let mut active_model = request_model;

    let (line_tx, line_rx) = mpsc::channel::<Result<String, String>>();
    thread::spawn(move || {
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if line_tx.send(Ok(l)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = line_tx.send(Err(e.to_string()));
                    break;
                }
            }
        }
    });

    loop {
        if interrupt.load(Ordering::SeqCst) {
            let _ = writeln!(stdin, "{}", json!({ "op": "interrupt" }));
            let _ = stdin.flush();
            thread::sleep(Duration::from_millis(400));
            let _ = child.kill();
            on_event(AgentEvent::Done {
                status: DoneStatus::Interrupted,
                result: None,
                error: None,
                session_id: session_id.clone(),
            });
            done = true;
            break;
        }

        while let Ok((prompt, images)) = steer_rx.try_recv() {
            let imgs: Vec<Value> = images
                .into_iter()
                .map(|img| json!({ "data": img.data, "mimeType": img.mime_type }))
                .collect();
            let line = json!({ "op": "steer", "prompt": prompt, "images": imgs });
            if writeln!(stdin, "{line}").is_ok() {
                let _ = stdin.flush();
            }
        }

        match line_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                match frame.get("ev").and_then(Value::as_str).unwrap_or("") {
                    "ready" => {
                        let agent_id = str_field(&frame, "agentId");
                        session_id = Some(agent_id.clone());
                        // SDK may emit `default` for Auto; never let that replace
                        // the catalog id the user (or seed) picked.
                        let model = {
                            let m = str_field(&frame, "model");
                            if m.is_empty() || m.eq_ignore_ascii_case("default") {
                                if active_model.is_empty() {
                                    "auto".into()
                                } else {
                                    active_model.clone()
                                }
                            } else {
                                m
                            }
                        };
                        active_model = model.clone();
                        saw_ready = true;
                        on_event(AgentEvent::SessionStarted {
                            harness: "cursor".into(),
                            model,
                            tools: Vec::new(),
                            cwd: cwd.clone(),
                            session_id: agent_id,
                        });
                        // Without this, settle_run promotes the Sending queue
                        // row to Uncertain ("No command acknowledgement…")
                        // even after a successful turn — Cursor never had a
                        // delivery ack path like Pi/OpenCode.
                        on_event(AgentEvent::CommandAccepted {
                            command_id: run_id.clone(),
                        });
                    }
                    "fatal" => {
                        let message = str_field(&frame, "message");
                        on_event(AgentEvent::Error {
                            message: message.clone(),
                        });
                        on_event(AgentEvent::Done {
                            status: DoneStatus::Errored,
                            result: None,
                            error: Some(message),
                            session_id: session_id.clone(),
                        });
                        done = true;
                    }
                    "turn" => {
                        let status = str_field(&frame, "status");
                        let error = frame
                            .get("error")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string);
                        let done_status = match status.as_str() {
                            "cancelled" => DoneStatus::Interrupted,
                            "error" => DoneStatus::Errored,
                            _ => DoneStatus::Completed,
                        };
                        if let Some(err) = &error {
                            on_event(AgentEvent::Error {
                                message: err.clone(),
                            });
                        }
                        on_event(AgentEvent::Done {
                            status: done_status,
                            result: None,
                            error,
                            session_id: session_id.clone(),
                        });
                        done = true;
                    }
                    "usage" => {
                        let input = json_count(frame.get("input"));
                        let output = json_count(frame.get("output"));
                        let cache = json_count(frame.get("cacheRead"))
                            .saturating_add(json_count(frame.get("cacheWrite")));
                        if let Some(event) = usage_event(&active_model, input, output, cache) {
                            on_event(event);
                        }
                    }
                    _ => {
                        for event in map_frame(&frame) {
                            on_event(event);
                        }
                    }
                }
                if done {
                    break;
                }
            }
            Ok(Err(err)) => {
                on_event(AgentEvent::Error { message: err });
                on_event(AgentEvent::Done {
                    status: DoneStatus::Errored,
                    result: None,
                    error: Some("Cursor shim stdout read failed".into()),
                    session_id: session_id.clone(),
                });
                done = true;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(status)) = child.try_wait() {
                    if !done {
                        let code = status.code().unwrap_or(-1);
                        on_event(AgentEvent::Done {
                            status: if status.success() && saw_ready {
                                DoneStatus::Completed
                            } else {
                                DoneStatus::Errored
                            },
                            result: None,
                            error: if status.success() {
                                None
                            } else {
                                Some(format!("Cursor shim exited with code {code}"))
                            },
                            session_id: session_id.clone(),
                        });
                        done = true;
                    }
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !done {
                    on_event(AgentEvent::Done {
                        status: DoneStatus::Errored,
                        result: None,
                        error: Some("Cursor shim closed stdout before the turn finished".into()),
                        session_id: session_id.clone(),
                    });
                }
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = done;
}

fn usage_event(model: &str, input: u64, output: u64, cache: u64) -> Option<AgentEvent> {
    halo_harness::usage::meter(model, input, output, cache, None).map(|meter| AgentEvent::Usage {
        used_tokens: meter.used_tokens,
        window_tokens: meter.window_tokens,
        input_tokens: meter.input_tokens,
        output_tokens: meter.output_tokens,
    })
}

fn json_count(value: Option<&serde_json::Value>) -> u64 {
    halo_harness::usage::json_count(value)
}

#[cfg(test)]
mod resolve_tests {
    use super::resolve_node;

    #[test]
    fn node_executable_override_wins() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("node");
        std::fs::write(&fake, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755));
        }
        // SAFETY: test-only env mutation; not run concurrently with other resolve tests.
        unsafe {
            std::env::set_var("NODE_EXECUTABLE", &fake);
        }
        let resolved = resolve_node();
        unsafe {
            std::env::remove_var("NODE_EXECUTABLE");
        }
        assert_eq!(resolved.as_deref(), Some(fake.as_path()));
    }
}

#[cfg(test)]
mod usage_tests {
    use serde_json::json;

    use super::super::types::{AgentEvent, DoneStatus, ToolCall};
    use super::{decode_tool, map_frame, usage_event};

    #[test]
    fn cursor_usage_carries_a_context_window() {
        let AgentEvent::Usage {
            used_tokens,
            window_tokens,
            input_tokens,
            output_tokens,
        } = usage_event("composer-2.5", 4_000, 800, 200).unwrap()
        else {
            panic!("usage");
        };
        assert_eq!(input_tokens, 4_000);
        assert_eq!(output_tokens, 800);
        assert_eq!(used_tokens, 5_000);
        assert!(window_tokens > 0);
    }

    #[test]
    fn cursor_subagent_frames_map_to_subagent_events() {
        let start = json!({
            "ev": "tool",
            "phase": "start",
            "id": "call-task-1",
            "name": "task",
            "args": {
                "description": "Audit harness",
                "agent": "research",
                "prompt": "Check all files"
            }
        });
        let events = map_frame(&start);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            AgentEvent::SubagentStarted {
                parent_tool_call_id: Some(pid),
                session_id,
                title,
                agent: Some(agent),
            } if pid == "call-task-1" && session_id == "call-task-1" && title == "Audit harness" && agent == "research"
        ));
        assert!(matches!(
            &events[1],
            AgentEvent::ToolCall {
                id,
                call: ToolCall::Task { description, .. }
            } if id == "call-task-1" && description == "Audit harness"
        ));

        let child_text = json!({
            "ev": "text",
            "text": "Found 3 files",
            "parent": "call-task-1"
        });
        let child_events = map_frame(&child_text);
        assert_eq!(child_events.len(), 1);
        assert!(matches!(
            &child_events[0],
            AgentEvent::SubagentEvent { session_id, event }
                if session_id == "call-task-1"
                    && matches!(event.as_ref(), AgentEvent::TextDelta { text } if text == "Found 3 files")
        ));

        let end = json!({
            "ev": "tool",
            "phase": "end",
            "id": "call-task-1",
            "name": "task",
            "error": false
        });
        let end_events = map_frame(&end);
        assert_eq!(end_events.len(), 2);
        assert!(matches!(
            &end_events[1],
            AgentEvent::SubagentDone {
                session_id,
                status: DoneStatus::Completed,
                error: None,
            } if session_id == "call-task-1"
        ));
    }

    #[test]
    fn cursor_shell_classifies_halo_browser_commands() {
        let call = decode_tool(
            "shell",
            &json!({ "command": "halo-browser open https://example.com" }),
        );
        assert!(matches!(
            call,
            ToolCall::Browser {
                ref action,
                ref target,
            } if action == "open" && target == "https://example.com"
        ));
    }
}

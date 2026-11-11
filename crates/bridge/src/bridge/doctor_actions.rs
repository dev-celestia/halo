//! Doctor action primitives.
//!
//! Everything here runs on a worker thread, never the UI thread, and is built
//! for failure: a command that hangs is killed at a deadline, a missing tool is
//! reported by name with the fix, an install that "succeeds" without producing
//! the binary is reported as a failure, and a repair that needs administrator
//! rights asks macOS for the administrator dialog instead of copying a command.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::backend::Backend;

/// Probes and single queries.
pub(crate) const QUICK: Duration = Duration::from_secs(20);
/// Package installers (`npm`, `brew`, `cargo`).
pub(crate) const INSTALL: Duration = Duration::from_secs(600);

/// Result of a finished process.
pub(crate) struct RunOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutput {
    /// Best available explanation of what happened, trimmed for a log line.
    pub fn detail(&self) -> String {
        for stream in [&self.stderr, &self.stdout] {
            if let Some(line) = stream.lines().map(str::trim).rev().find(|l| !l.is_empty()) {
                return tail(line, 400);
            }
        }
        if self.ok {
            "ok".to_string()
        } else {
            "no output".to_string()
        }
    }
}

fn tail(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= limit {
        return trimmed.to_string();
    }
    format!(
        "…{}",
        trimmed.chars().skip(count - limit).collect::<String>()
    )
}

fn read_stream<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        String::from_utf8_lossy(&buffer).to_string()
    })
}

/// Run `command` with a deadline.
///
/// Both pipes are drained on their own threads — an installer that logs more
/// than a pipe buffer would otherwise deadlock — and the child is killed when
/// the deadline passes, so no action can wedge the Doctor forever.
pub(crate) fn run_command(
    command: &mut Command,
    timeout: Duration,
    label: &str,
) -> Result<RunOutput, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {label}: {e}"))?;

    let stdout = child.stdout.take().map(read_stream);
    let stderr = child.stderr.take().map(read_stream);
    let deadline = Instant::now() + timeout;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let partial = stdout
                        .and_then(|handle| handle.join().ok())
                        .unwrap_or_default();
                    return Err(format!(
                        "{label} timed out after {}s and was stopped. {}",
                        timeout.as_secs(),
                        tail(&partial, 300)
                    ));
                }
                std::thread::sleep(Duration::from_millis(60));
            }
            Err(e) => return Err(format!("could not wait for {label}: {e}")),
        }
    };

    Ok(RunOutput {
        ok: status.success(),
        stdout: stdout
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
        stderr: stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
    })
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    }
    #[cfg(not(windows))]
    {
        // A login shell so `brew`, `npm` and friends resolve the way they do in
        // the user's terminal instead of the app's minimal launch PATH.
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", command]);
        cmd
    }
}

pub(crate) fn run_shell_with(command: &str, timeout: Duration) -> Result<String, String> {
    let label = format!("`{command}`");
    let output = run_command(&mut shell_command(command), timeout, &label)?;
    if output.ok {
        Ok(output.detail())
    } else {
        Err(format!("{label} failed: {}", output.detail()))
    }
}

pub(crate) fn run_shell(command: &str) -> Result<String, String> {
    run_shell_with(command, INSTALL)
}

/// Single-quote a path for `sh -lc`.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool discovery
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve a tool the way the app actually would, with the usual macOS package
/// prefixes as a fallback for a Finder launch (minimal `PATH`).
pub(crate) fn find_tool(name: &str) -> Option<String> {
    if let Some(path) = halo_lsp::which(name) {
        return Some(path);
    }
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join(".cargo/bin").join(name));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
}

/// Preflight: fail with the fix, not with `command not found`.
pub(crate) fn require_tool(name: &str, fix: &str) -> Result<String, String> {
    find_tool(name).ok_or_else(|| format!("`{name}` is not installed or not on PATH. {fix}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Privilege hand-off
// ─────────────────────────────────────────────────────────────────────────────

/// Marker the UI looks for so it can put the command on the clipboard.
const PRIVILEGED_MARKER: &str = "Run this in Terminal:";

/// An error that a click cannot solve, carrying the exact command that can.
pub(crate) fn privileged_hint(reason: &str, command: &str) -> String {
    format!("{reason} This needs administrator rights. {PRIVILEGED_MARKER} {command}")
}

/// Pull the shell command back out of a [`privileged_hint`] message.
pub(crate) fn privileged_command(message: &str) -> Option<String> {
    message
        .split_once(PRIVILEGED_MARKER)
        .map(|(_, command)| command.trim().to_string())
        .filter(|command| !command.is_empty())
}

/// `osascript` reports a dismissed administrator dialog as -128 / "User canceled".
pub(crate) fn osascript_cancelled(detail: &str) -> bool {
    detail.contains("-128") || detail.to_ascii_lowercase().contains("cancel")
}

/// A global install that the user can authorize. `npm -g` into a protected
/// prefix fails this way; a user-owned prefix does not.
pub(crate) fn permission_denied(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("eacces") || lower.contains("permission denied") || lower.contains("eperm")
}

/// AppleScript that runs `command` after the system administrator dialog.
pub(crate) fn admin_shell_script(command: &str) -> String {
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    format!("do shell script \"{escaped}\" with administrator privileges")
}

/// Run `command` via the macOS administrator dialog.
pub(crate) fn run_admin_shell(command: &str) -> Result<(), Escalation> {
    let mut cmd = Command::new("osascript");
    cmd.arg("-e").arg(admin_shell_script(command));
    let out = match run_command(&mut cmd, INSTALL, "the administrator prompt") {
        Ok(out) => out,
        Err(err) => return Err(Escalation::Unavailable(err)),
    };
    if out.ok {
        return Ok(());
    }
    let detail = out.detail();
    Err(if osascript_cancelled(&detail) {
        Escalation::Cancelled
    } else {
        Escalation::Unavailable(detail)
    })
}

/// The exact command a human can paste to take ownership of `path`.
pub(crate) fn sudo_chown_command(path: &Path) -> String {
    format!(
        "sudo chown -R $(id -u):$(id -g) {}",
        shell_quote(&path.display().to_string())
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Administrator authorization
// ─────────────────────────────────────────────────────────────────────────────

/// How long a human gets to type an administrator password.
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(120);

/// Why an in-place `chown` did not happen.
pub(crate) enum Escalation {
    /// The user dismissed the password prompt. The caller must leave the paths
    /// exactly as they are — a dismissed prompt is an answer, not a failure.
    Cancelled,
    /// No GUI authorization is reachable (headless run, sandbox, other OS).
    /// The caller may fall back to the unprivileged route.
    Unavailable(String),
}

/// AppleScript that hands `chown -R` to the system authorization dialog.
///
/// Pure so the quoting is testable: the shell command is nested inside an
/// AppleScript string literal, which is where a path containing a quote breaks.
pub(crate) fn admin_chown_script(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let uid = current_uid()?;
    let gid = current_gid()?;
    let mut command = format!("chown -R {uid}:{gid}");
    for path in paths {
        command.push(' ');
        command.push_str(&shell_quote(&path.display().to_string()));
    }
    // Escape for the AppleScript literal first, then let AppleScript run it.
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    Some(format!(
        "do shell script \"{escaped}\" with administrator privileges"
    ))
}

/// Take ownership of `paths` **in place**, after asking macOS for administrator
/// rights.
///
/// This is the preferred repair: the directories keep their identity and their
/// contents, and — unlike renaming them aside — the CLI keeps working from a
/// terminal, where it has no idea Halo moved anything. One prompt covers every
/// path, so a blocked Pi *and* OpenCode root costs a single password.
pub(crate) fn escalate_chown(paths: &[PathBuf]) -> Result<(), Escalation> {
    let Some(script) = admin_chown_script(paths) else {
        return Err(Escalation::Unavailable(
            "cannot resolve the owner these paths should have".to_string(),
        ));
    };
    let mut cmd = Command::new("osascript");
    cmd.arg("-e").arg(script);
    let out = match run_command(&mut cmd, AUTHORIZATION_TIMEOUT, "the administrator prompt") {
        Ok(out) => out,
        Err(err) => return Err(Escalation::Unavailable(err)),
    };
    if out.ok {
        return Ok(());
    }
    let detail = out.detail();
    Err(if osascript_cancelled(&detail) {
        Escalation::Cancelled
    } else {
        Escalation::Unavailable(detail)
    })
}

/// Where Halo installs the pinned Pi so it does not have to overwrite Homebrew.
pub(crate) fn pinned_pi_prefix() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".halo").join("pi"))
}

/// `{prefix}/bin/pi` after [`install_pinned_pi`].
pub(crate) fn pinned_pi_executable() -> Option<PathBuf> {
    let name = if cfg!(windows) { "pi.cmd" } else { "pi" };
    pinned_pi_prefix().map(|prefix| prefix.join("bin").join(name))
}

/// Install `@earendil-works/pi-coding-agent@1.4.0` under `~/.halo/pi`.
///
/// A permission error asks macOS for administrator rights and runs the same
/// npm command. A dismissed dialog does not change anything.
pub(crate) fn install_pinned_pi() -> Result<PathBuf, String> {
    ensure_node()?;
    let prefix = pinned_pi_prefix()
        .ok_or_else(|| "Cannot resolve a home directory for the Pi install.".to_string())?;
    std::fs::create_dir_all(&prefix)
        .map_err(|error| format!("Cannot create {}: {error}", prefix.display()))?;
    let npm = find_tool("npm").ok_or_else(|| {
        "npm is not installed. Install Node.js ≥ 22.19, then retry.".to_string()
    })?;
    let command = format!(
        "{} install -g --prefix {} --ignore-scripts @earendil-works/pi-coding-agent@1.4.0",
        shell_quote(&npm),
        shell_quote(&prefix.display().to_string()),
    );
    match run_shell(&command) {
        Ok(_) => {}
        Err(err) if permission_denied(&err) => match run_admin_shell(&command) {
            Ok(()) => {}
            Err(Escalation::Cancelled) => {
                return Err(
                    "Pi was not installed — the administrator prompt was dismissed. Nothing was changed."
                        .to_string(),
                );
            }
            Err(Escalation::Unavailable(why)) => {
                return Err(format!(
                    "Pi install needs administrator rights, and macOS did not show the dialog ({why})."
                ));
            }
        },
        Err(err) => return Err(err),
    }
    let bin = pinned_pi_executable()
        .ok_or_else(|| "Cannot resolve the installed Pi path.".to_string())?;
    if !bin.is_file() {
        return Err(format!(
            "npm finished but {} is missing, so Halo is still not on Pi 1.4.0.",
            bin.display()
        ));
    }
    Ok(bin)
}

// ─────────────────────────────────────────────────────────────────────────────
// Ownership
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn current_uid() -> Option<u32> {
    static UID: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *UID.get_or_init(|| id_value("-u"))
}

pub(crate) fn current_gid() -> Option<u32> {
    static GID: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *GID.get_or_init(|| id_value("-g"))
}

fn id_value(flag: &str) -> Option<u32> {
    let out = Command::new("id").arg(flag).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(unix)]
fn owner_of(path: &Path) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .ok()
        .map(|meta| (meta.uid(), meta.gid()))
}

#[cfg(not(unix))]
fn owner_of(_path: &Path) -> Option<(u32, u32)> {
    None
}

/// Who a directory *should* belong to: the invoking user when we run as root,
/// otherwise ourselves.
fn target_owner(root: &Path) -> Option<(u32, u32)> {
    let uid = current_uid()?;
    if uid != 0 {
        return Some((uid, current_gid()?));
    }
    let sudo_uid = std::env::var("SUDO_UID")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok());
    let sudo_gid = std::env::var("SUDO_GID")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok());
    if let (Some(uid), Some(gid)) = (sudo_uid, sudo_gid) {
        return Some((uid, gid));
    }
    owner_of(root.parent()?)
}

/// Top-most paths under `root` owned by somebody else. Stops descending at the
/// first foreign entry — its subtree is foreign too.
pub(crate) fn foreign_paths(root: &Path) -> Vec<PathBuf> {
    const MAX_WALK: usize = 4000;

    let Some(uid) = current_uid() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0usize;

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_WALK {
                out.sort();
                return out;
            }
            let path = entry.path();
            if owner_of(&path).is_some_and(|(owner, _)| owner != uid) {
                out.push(path);
                continue;
            }
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

pub(crate) fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".doctor-write-probe");
    match std::fs::write(&probe, b"1") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn chown(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    let out = Command::new("chown")
        .arg("-R")
        .arg(format!("{uid}:{gid}"))
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
}

/// Rename a foreign entry out of the way. Nothing is deleted: the timestamped
/// sibling stays in the same directory. This works unprivileged because the
/// *parent* directory is ours even when the entry is not.
pub(crate) fn move_aside(path: &Path) -> Result<PathBuf, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "path has no file name".to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "path has no parent directory".to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");

    let mut dest = parent.join(format!("{name}.root-bak-{stamp}"));
    let mut attempt = 1;
    while dest.exists() {
        dest = parent.join(format!("{name}.root-bak-{stamp}-{attempt}"));
        attempt += 1;
    }
    std::fs::rename(path, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// What a reclaim pass achieved.
#[derive(Default)]
pub(crate) struct Reclaim {
    pub chowned: usize,
    pub moved: Vec<String>,
    pub failures: Vec<String>,
}

impl Reclaim {
    pub fn total(&self) -> usize {
        self.chowned + self.moved.len()
    }

    pub fn merge(&mut self, other: Reclaim) {
        self.chowned += other.chowned;
        self.moved.extend(other.moved);
        self.failures.extend(other.failures);
    }

    pub fn summary(&self, what: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.chowned > 0 {
            parts.push(format!("{} chowned", self.chowned));
        }
        if !self.moved.is_empty() {
            parts.push(format!(
                "{} moved aside to *.root-bak-* (recreated on next launch)",
                self.moved.len()
            ));
        }
        if parts.is_empty() {
            format!("{what} is already owned by the current user.")
        } else {
            format!("{what}: {}.", parts.join(", "))
        }
    }
}

/// Whether `path` is owned by the invoking user.
///
/// A directory can be foreign at the *root* as well as in its children: a
/// root-run `sudo make run` leaves `~/.pi` owned by root with mode 0755 and
/// `~/.pi/agent` at 0700, so the app cannot even list it. [`foreign_paths`]
/// never reports that case — it walks *inside* the root — which is why the
/// whole-directory check has to be separate.
pub(crate) fn owner_is_self(path: &Path) -> bool {
    match (owner_of(path), current_uid()) {
        (Some((owner, _)), Some(uid)) => owner == uid,
        // Unknown ownership is not treated as foreign: a false positive would
        // rename a perfectly good directory.
        _ => true,
    }
}

/// Reclaim a directory that is *itself* foreign-owned.
///
/// Chown when privileged; otherwise rename the whole directory aside. The
/// rename works unprivileged because the *parent* is ours (usually `$HOME`),
/// and nothing is deleted — the timestamped sibling keeps every file.
pub(crate) fn reclaim_root(path: &Path) -> Reclaim {
    let mut reclaim = Reclaim::default();
    if !path.exists() || owner_is_self(path) {
        return reclaim;
    }
    let Some((uid, gid)) = target_owner(path) else {
        reclaim.failures.push(format!(
            "cannot determine the owner that {} should have",
            path.display()
        ));
        return reclaim;
    };
    if current_uid() == Some(0) && chown(path, uid, gid).is_ok() {
        reclaim.chowned += 1;
        return reclaim;
    }
    match move_aside(path) {
        Ok(dest) => reclaim.moved.push(dest.display().to_string()),
        Err(err) => reclaim.failures.push(format!("{}: {err}", path.display())),
    }
    reclaim
}

/// Take back every foreign-owned entry under `root`: `chown` when we are root,
/// otherwise rename it aside. Never deletes.
pub(crate) fn reclaim_tree(root: &Path) -> Reclaim {
    let mut reclaim = Reclaim::default();
    let Some((uid, gid)) = target_owner(root) else {
        reclaim.failures.push(format!(
            "cannot determine the owner that {} should have",
            root.display()
        ));
        return reclaim;
    };
    let can_chown = current_uid() == Some(0);
    for path in foreign_paths(root) {
        if can_chown && chown(&path, uid, gid).is_ok() {
            reclaim.chowned += 1;
            continue;
        }
        match move_aside(&path) {
            Ok(dest) => reclaim.moved.push(dest.display().to_string()),
            Err(err) => reclaim.failures.push(format!("{}: {err}", path.display())),
        }
    }
    reclaim
}

// ─────────────────────────────────────────────────────────────────────────────
// Single-instance socket directory
// ─────────────────────────────────────────────────────────────────────────────

/// Candidate run directories for the instance lock, both brands.
pub(crate) fn run_dirs() -> Vec<PathBuf> {
    brand_roots()
        .into_iter()
        .map(|root| root.join("run"))
        .collect()
}

/// The per-brand cache roots (`~/Library/Caches/Halo`), which hold the instance
/// lock, the browser cache and the LSP state. A root-owned entry anywhere here
/// breaks whichever subsystem owns it.
pub(crate) fn brand_roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    #[cfg(target_os = "macos")]
    {
        vec![home.join("Library/Caches/Halo")]
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![home.join(".cache/halo")]
    }
}

/// Reclaim the app cache tree and drop stale sockets.
///
/// A live socket is never *removed* — it belongs to a running instance — but a
/// socket owned by root is still reclaimed, because a non-root app cannot use
/// it anyway.
pub(crate) fn repair_run_dir() -> Result<String, String> {
    let roots: Vec<PathBuf> = brand_roots()
        .into_iter()
        .filter(|root| root.is_dir())
        .collect();

    // Ask once for everything that is foreign, and take it in place when the
    // user authorizes it — a moved cache root loses the browser profile and any
    // session state living under it, which a chown preserves.
    let foreign: Vec<PathBuf> = roots
        .iter()
        .filter(|root| !owner_is_self(root) || !foreign_paths(root).is_empty())
        .cloned()
        .collect();
    if !foreign.is_empty() && current_uid() != Some(0) {
        match escalate_chown(&foreign) {
            Ok(()) => {}
            Err(Escalation::Cancelled) => {
                return Err(
                    "Ownership was not changed — the administrator prompt was dismissed. \
                     Nothing was moved and nothing was deleted."
                        .to_string(),
                );
            }
            Err(Escalation::Unavailable(why)) => {
                // The system dialog is the permission step. Don't move the
                // cache or hand back a sudo line when macOS can show it.
                #[cfg(target_os = "macos")]
                {
                    return Err(format!(
                        "macOS did not show the administrator dialog ({why}). Nothing was moved and nothing was deleted."
                    ));
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = why;
                }
            }
        }
    }

    let mut reclaim = Reclaim::default();
    for root in &roots {
        if !root.is_dir() {
            continue;
        }
        // A foreign root cannot have its *children* renamed — that needs write
        // on the root itself — so take the whole cache directory aside. Its
        // parent (`~/Library/Caches`) is user-owned, so this works unprivileged.
        // Everything under here is regenerable cache: sockets, browser profile.
        if !owner_is_self(root.as_path()) {
            reclaim.merge(reclaim_root(root.as_path()));
            continue;
        }
        reclaim.merge(reclaim_tree(root.as_path()));
    }

    // Anything that could not be reclaimed leaves the directory unusable, and
    // only an administrator can fix that — hand over the command.
    let mut blocked: Vec<PathBuf> = Vec::new();
    for dir in run_dirs() {
        let _ = std::fs::create_dir_all(&dir);
        if !dir.is_dir() || !dir_writable(&dir) {
            blocked.push(dir);
        }
    }
    if let Some(dir) = blocked.first() {
        if current_uid() != Some(0) {
            match escalate_chown(&blocked) {
                Ok(()) => {}
                Err(Escalation::Cancelled) => {
                    return Err(
                        "Ownership was not changed — the administrator prompt was dismissed. \
                         Nothing was moved and nothing was deleted."
                            .to_string(),
                    );
                }
                Err(Escalation::Unavailable(why)) => {
                    #[cfg(target_os = "macos")]
                    {
                        return Err(format!(
                            "{} is not writable. macOS did not show the administrator dialog ({why}).",
                            dir.display()
                        ));
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        return Err(privileged_hint(
                            &format!(
                                "{} is owned by another user, so the app cannot write its instance lock.",
                                dir.display()
                            ),
                            &sudo_chown_command(&dir.parent().unwrap_or(dir)),
                        ));
                    }
                }
            }
        }
        if let Some(dir) = blocked.iter().find(|dir| !dir_writable(dir)) {
            return Err(format!(
                "{} is still not writable after the administrator prompt.",
                dir.display()
            ));
        }
    }
    if !reclaim.failures.is_empty() {
        return Err(format!(
            "reclaimed {} entr(ies) but {} failed: {}",
            reclaim.total(),
            reclaim.failures.len(),
            reclaim.failures.join("; ")
        ));
    }

    let mut removed = 0usize;
    for dir in run_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "sock")
                && !socket_is_live(&path)
                && std::fs::remove_file(&path).is_ok()
            {
                removed += 1;
            }
        }
    }

    let mut parts = vec![format!("{removed} stale socket(s) removed")];
    if reclaim.total() > 0 {
        parts.push(reclaim.summary("ownership reclaimed"));
    }
    Ok(parts.join(", "))
}

#[cfg(unix)]
pub(crate) fn socket_is_live(path: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

#[cfg(not(unix))]
pub(crate) fn socket_is_live(_path: &Path) -> bool {
    false
}

// ─────────────────────────────────────────────────────────────────────────────
// Manual hand-offs
// ─────────────────────────────────────────────────────────────────────────────

fn opener(url: &str) -> Command {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.arg(url);
        cmd
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(url);
        cmd
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", ""]).arg(url);
        cmd
    }
}

/// The action for things that genuinely cannot be installed non-interactively:
/// open the vendor's download page instead of leaving a dead row.
pub(crate) fn open_url(url: &str) -> Result<String, String> {
    let output = run_command(&mut opener(url), QUICK, "open")?;
    if output.ok {
        Ok(format!("Opened {url} in the default browser."))
    } else {
        Err(format!(
            "Could not open a browser. Download it from {url} ({})",
            output.detail()
        ))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Node.js
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn node_ready() -> bool {
    find_tool("node").is_some() && find_tool("npm").is_some()
}

/// Install Node.js — the prerequisite for every npm-based harness and LSP.
pub(crate) fn install_node() -> Result<String, String> {
    if node_ready() {
        return Ok("Node.js and npm are already available.".to_string());
    }
    let Some(brew) = find_tool("brew") else {
        return Err(
            "Node.js is missing and Homebrew is not available, so it cannot be installed from here. \
             Download the LTS installer from https://nodejs.org (or install Homebrew from https://brew.sh), \
             then run Check again."
                .to_string(),
        );
    };
    let output = run_command(
        &mut shell_command(&format!("{} install node", shell_quote(&brew))),
        INSTALL,
        "brew install node",
    )?;
    if !output.ok {
        return Err(format!("`brew install node` failed: {}", output.detail()));
    }
    if !node_ready() {
        return Err(format!(
            "Homebrew finished but `node`/`npm` are still not resolvable ({}). Open a new terminal once, then run Check again.",
            output.detail()
        ));
    }
    Ok("Installed Node.js and npm.".to_string())
}

/// Ensure Node exists before an npm-based install; returns a note when it had
/// to install it first.
pub(crate) fn ensure_node() -> Result<Option<String>, String> {
    if node_ready() {
        return Ok(None);
    }
    install_node().map(Some)
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage
// ─────────────────────────────────────────────────────────────────────────────

fn default_database_path() -> String {
    dirs::data_local_dir()
        .map(|dir| dir.join("com.halo").join("halo.db"))
        .unwrap_or_else(|| PathBuf::from("~/Library/Application Support/com.halo/halo.db"))
        .display()
        .to_string()
}

/// Verify the SQLite database and reclaim space. Never deletes user data.
pub(crate) fn repair_storage(backend: Option<&dyn Backend>) -> Result<String, String> {
    let Some(backend) = backend else {
        return Err("Backend unavailable for storage repair.".to_string());
    };

    let info = match backend.get_storage_info() {
        Ok(info) => info,
        Err(err) => {
            let path = default_database_path();
            return Err(format!(
                "Halo cannot read its database ({err}). File: {path}. \
                 Copy it somewhere safe, then move it aside — Halo recreates an empty one on the next launch."
            ));
        }
    };

    let path = info.database_path.clone();
    let mut notes: Vec<String> = Vec::new();
    if find_tool("sqlite3").is_some() {
        let quoted = shell_quote(&path);
        match run_shell_with(&format!("sqlite3 {quoted} 'PRAGMA quick_check;'"), QUICK) {
            Ok(out) => notes.push(format!(
                "quick_check {}",
                out.lines().next().unwrap_or("ok")
            )),
            Err(err) => notes.push(format!("quick_check failed: {err}")),
        }
        match run_shell_with(
            &format!("sqlite3 {quoted} 'PRAGMA wal_checkpoint(TRUNCATE); VACUUM;'"),
            INSTALL,
        ) {
            Ok(_) => notes.push("checkpoint + vacuum".to_string()),
            Err(err) => notes.push(format!("vacuum skipped: {err}")),
        }
    } else {
        notes.push("sqlite3 CLI not found, integrity check skipped".to_string());
    }
    let _ = backend.clear_chronicle_logs();

    Ok(format!("Database at {path} — {}.", notes.join(", ")))
}

// ─────────────────────────────────────────────────────────────────────────────
// halo-browser CLI
// ─────────────────────────────────────────────────────────────────────────────

/// Nearest ancestor holding a `Cargo.toml` (a dev checkout, not a bundle).
fn manifest_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .take(6)
        .find(|dir| dir.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

/// Dev-only: rebuild `halo-browser` next to the running executable, which is
/// where the agent's browser probe looks for it.
pub(crate) fn rebuild_halo_browser() -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot locate the running executable: {e}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "running executable has no directory".to_string())?
        .to_path_buf();

    let Some(root) = manifest_root(&exe_dir) else {
        return Err(
            "This build has no Cargo workspace beside it — `halo-browser` ships inside the app \
             bundle. Reinstall Halo (or rebuild the bundle) to restore it."
                .to_string(),
        );
    };
    let cargo = require_tool(
        "cargo",
        "Install the Rust toolchain (https://rustup.rs) or reinstall Halo.",
    )?;

    let mut command = Command::new(cargo);
    command
        .current_dir(&root)
        .args(["build", "-p", "halo-browser-tools", "--bin", "halo-browser"]);
    if let Some(target) = exe_dir.parent() {
        command.env("CARGO_TARGET_DIR", target);
    }

    let output = run_command(&mut command, INSTALL, "cargo build -p halo-browser-tools")?;
    if !output.ok {
        return Err(format!(
            "cargo build failed in {}: {}",
            root.display(),
            output.detail()
        ));
    }

    let produced = exe_dir.join(if cfg!(windows) {
        "halo-browser.exe"
    } else {
        "halo-browser"
    });
    if produced.is_file() {
        Ok(format!("Rebuilt {}.", produced.display()))
    } else {
        Err(format!(
            "cargo reported success but {} is still missing.",
            produced.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_administrator_prompt_is_not_a_clipboard_command() {
        assert!(osascript_cancelled("osascript: execution error: User canceled. (-128)"));
        let dismissed = "Ownership was not changed — the administrator prompt was dismissed. \
             Nothing was moved and nothing was deleted.";
        assert!(privileged_command(dismissed).is_none());
        assert!(admin_shell_script("chown -R 1:1 '/Users/me/.pi'")
            .ends_with("with administrator privileges"));
    }

    #[test]
    fn privileged_hint_round_trips() {
        let message = privileged_hint("Denied.", "sudo chown -R $(id -u) /tmp/x");
        assert_eq!(
            privileged_command(&message).as_deref(),
            Some("sudo chown -R $(id -u) /tmp/x")
        );
        assert_eq!(privileged_command("plain error"), None);
    }

    #[test]
    fn the_admin_script_quotes_paths_that_would_break_the_literal() {
        let paths = vec![
            PathBuf::from("/Users/me/.pi"),
            PathBuf::from("/Users/me/Library/Application Support/x"),
            PathBuf::from("/Users/me/od\"d"),
        ];
        let script = admin_chown_script(&paths).expect("script");
        // One prompt, one command, every path quoted for the shell.
        assert!(script.starts_with("do shell script \"chown -R "));
        assert!(script.ends_with("with administrator privileges"));
        assert!(script.contains("'/Users/me/.pi'"));
        assert!(script.contains("'/Users/me/Library/Application Support/x'"));
        // The embedded quote must be escaped, not close the AppleScript string.
        assert!(script.contains("\\\""));
        assert_eq!(script.matches("do shell script").count(), 1);
    }

    #[test]
    fn no_paths_means_no_prompt() {
        assert_eq!(admin_chown_script(&[]), None);
    }

    #[test]
    fn run_shell_reports_exit_and_output() {
        let ok = run_shell_with("echo hello", QUICK).expect("echo should succeed");
        assert!(ok.contains("hello"));

        let err = run_shell_with("exit 3", QUICK).expect_err("non-zero exit is an error");
        assert!(err.contains("failed"), "{err}");
    }

    #[test]
    fn run_shell_kills_a_hanging_command() {
        let started = Instant::now();
        let err = run_shell_with("sleep 30", Duration::from_millis(400))
            .expect_err("a hanging command must time out");
        assert!(err.contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn own_files_are_not_foreign() {
        let dir = std::env::temp_dir().join(format!("halo-doctor-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), b"{}").unwrap();
        assert!(foreign_paths(&dir).is_empty());
        assert!(dir_writable(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_aside_never_collides() {
        let dir = std::env::temp_dir().join(format!("halo-doctor-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("opencode.log");
        std::fs::write(&target, b"x").unwrap();

        let first = move_aside(&target).expect("first move");
        std::fs::write(&target, b"y").unwrap();
        let second = move_aside(&target).expect("second move");
        assert_ne!(first, second);
        assert!(first.exists() && second.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reclaim_summary_reads_naturally() {
        let mut reclaim = Reclaim::default();
        assert!(reclaim.summary("State").contains("already owned"));
        reclaim.chowned = 2;
        reclaim.moved.push("a".into());
        let text = reclaim.summary("State");
        assert!(text.contains("2 chowned"), "{text}");
        assert!(text.contains("1 moved aside"), "{text}");
    }
}

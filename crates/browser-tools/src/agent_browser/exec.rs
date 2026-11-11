//! One CLI invocation: bounded, isolated, and never silent about failure.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::ToolResult;

/// A browser command can hang on a slow page or a wedged daemon. Ten seconds is
/// long enough for a real navigation and short enough that an agent recovers.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Cli {
    pub bin: PathBuf,
    pub session: String,
    /// When set, agent-browser attaches to this CDP endpoint instead of
    /// launching a browser of its own.
    pub cdp: Option<String>,
    /// Chrome user-data-dir for this session. Set by [`Self::hardened`] so a
    /// chat's cookies live under Halo's cache and die with the chat.
    pub profile: Option<PathBuf>,
    /// Where downloads land. Never `~/Downloads`.
    pub download_dir: Option<PathBuf>,
    /// Mirror of [`crate::policy`], handed to agent-browser as well: the tool
    /// enforces it before the page loads, so a redirect cannot walk past it.
    pub allowed_domains: Vec<String>,
    pub timeout: Duration,
}

impl Cli {
    pub fn new(bin: PathBuf, session: String, cdp: Option<String>) -> Self {
        Self {
            bin,
            session,
            cdp,
            profile: None,
            download_dir: None,
            allowed_domains: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Point this session at Halo's own profile and download directories.
    pub fn hardened(mut self, session: &str, allowed_domains: Vec<String>) -> Self {
        let profile = crate::browser::profile_dir(session);
        let downloads = crate::browser::download_dir(session);
        let _ = std::fs::create_dir_all(&profile);
        let _ = std::fs::create_dir_all(&downloads);
        self.profile = Some(profile);
        self.download_dir = Some(downloads);
        self.allowed_domains = allowed_domains;
        self
    }

    /// Run one command and return its stdout on success.
    ///
    /// `NO_PROXY` is forced for localhost: an inherited `HTTP_PROXY` makes the
    /// CDP handshake fail with `Connection refused` against 127.0.0.1, which
    /// reads like a dead browser rather than a proxy in the way.
    pub fn run(&self, args: &[&str]) -> ToolResult<String> {
        let mut command = Command::new(&self.bin);
        if !self.session.is_empty() {
            command.arg("--session").arg(&self.session);
        }
        if let Some(cdp) = self.cdp.as_deref().filter(|value| !value.is_empty()) {
            command.arg("--cdp").arg(cdp);
        }
        if let Some(profile) = &self.profile {
            command.arg("--profile").arg(profile);
        }
        if let Some(downloads) = &self.download_dir {
            command.arg("--download-path").arg(downloads);
        }
        if !self.allowed_domains.is_empty() {
            command
                .arg("--allowed-domains")
                .arg(self.allowed_domains.join(","));
        }
        // A page cannot be allowed to hand back more than the hard cap before
        // the truncation in `Browser` ever sees it.
        command
            .arg("--max-output")
            .arg(crate::HARD_MAX_CHARS.to_string());
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env("no_proxy", "127.0.0.1,localhost");

        let mut child = command.spawn().map_err(|error| {
            format!(
                "cannot run {}: {error}. Install agent-browser or set HALO_BROWSER_BIN.",
                self.bin.display()
            )
        })?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let out_reader = stdout.map(|pipe| thread::spawn(move || read_all(pipe)));
        let err_reader = stderr.map(|pipe| thread::spawn(move || read_all(pipe)));

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{} timed out after {:?}",
                        args.first().copied().unwrap_or("command"),
                        self.timeout
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(40)),
                Err(error) => return Err(format!("waiting on the browser failed: {error}")),
            }
        };

        let stdout = out_reader.and_then(|h| h.join().ok()).unwrap_or_default();
        let stderr = err_reader.and_then(|h| h.join().ok()).unwrap_or_default();

        if status.success() && !stdout.trim_start().starts_with('✗') {
            Ok(stdout.trim().to_string())
        } else {
            Err(describe_failure(&stdout, &stderr, status.code()))
        }
    }
}

fn read_all(mut pipe: impl Read) -> String {
    let mut buffer = Vec::new();
    let _ = pipe.read_to_end(&mut buffer);
    String::from_utf8_lossy(&buffer).to_string()
}

fn describe_failure(stdout: &str, stderr: &str, code: Option<i32>) -> String {
    let detail = [stderr.trim(), stdout.trim()]
        .into_iter()
        .find(|part| !part.is_empty())
        .unwrap_or("no output");
    let clipped: String = detail.chars().take(600).collect();
    match code {
        Some(code) => format!("browser command failed (exit {code}): {clipped}"),
        None => format!("browser command failed: {clipped}"),
    }
}

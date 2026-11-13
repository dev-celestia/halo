//! Bounded, nonblocking pipes. Only the supervisor owns these OS resources.
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(crate) const MAX_FRAME: usize = 4 * 1024 * 1024;
pub(crate) const MAX_COMMAND: usize = 12 * 1024 * 1024;
const MAX_WRITER_BYTES: usize = 24 * 1024 * 1024;

#[cfg(unix)]
pub(crate) fn nonblocking(fd: &impl std::os::fd::AsRawFd) -> io::Result<()> {
    let raw = fd.as_raw_fd();
    // SAFETY: the borrowed pipe is valid throughout these fcntl calls.
    unsafe {
        let flags = libc::fcntl(raw, libc::F_GETFL);
        if flags < 0 || libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn nonblocking<T>(_: &T) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Pi supervision currently requires Unix",
    ))
}

pub(crate) fn grouped(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
}

pub(crate) fn signal_group(child: &mut Child, force: bool) {
    #[cfg(unix)]
    // SAFETY: process_group(0) made the child leader of its own group. Never signal our group.
    unsafe {
        libc::kill(
            -(child.id() as i32),
            if force { libc::SIGKILL } else { libc::SIGTERM },
        );
    }
    #[cfg(not(unix))]
    let _ = child.kill();
}

/// LF-only byte framing preserves split UTF-8 and embedded U+2028/U+2029.
#[derive(Default)]
pub(crate) struct Framer {
    bytes: Vec<u8>,
    scanned: usize,
}

impl Framer {
    pub fn read(&mut self, input: &mut impl Read) -> Result<Option<Vec<u8>>, String> {
        for _ in 0..32 {
            if let Some(n) = self.bytes[self.scanned..].iter().position(|b| *b == b'\n') {
                let end = self.scanned + n;
                if end > MAX_FRAME {
                    return Err("Pi stdout frame exceeds 4 MiB".into());
                }
                let mut frame: Vec<_> = self.bytes.drain(..=end).collect();
                frame.pop();
                if frame.last() == Some(&b'\r') {
                    frame.pop();
                }
                self.scanned = 0;
                return Ok(Some(frame));
            }
            self.scanned = self.bytes.len();
            if self.bytes.len() > MAX_FRAME {
                return Err("Pi stdout frame exceeds 4 MiB".into());
            }
            let mut chunk = [0; 8192];
            match input.read(&mut chunk) {
                Ok(0) => {
                    return Err(if self.bytes.is_empty() {
                        "Pi closed stdout before shutdown".into()
                    } else {
                        "Pi closed stdout with an incomplete JSON record".into()
                    });
                }
                Ok(n) => self.bytes.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err("Failed to read Pi stdout".into()),
            }
        }
        Ok(None)
    }
}

pub(crate) struct Transport {
    pub child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    stderr: ChildStderr,
    framer: Framer,
    writes: VecDeque<Vec<u8>>,
    offset: usize,
    queued_bytes: usize,
    // Diagnostics are drained but never logged: they can contain secrets. A
    // bounded tail is kept only so a child that dies before the session starts
    // can be *classified* (see `startup_diagnosis`) — it is never shown raw.
    pub diagnostic_bytes: usize,
    diagnostic_tail: String,
}

/// Cap on the retained stderr tail. A startup failure is one line; anything
/// longer is a log, and the newest bytes are the ones that explain the exit.
const DIAGNOSTIC_TAIL: usize = 2_048;

/// Startup failures we are willing to quote back to the user. Anything else
/// stays opaque rather than risking a token from a provider's error body.
const DIAGNOSTIC_SIGNATURES: [&str; 6] = [
    "permission denied",
    "EACCES",
    "EPERM",
    "no such file or directory",
    "command not found",
    "is not a directory",
];

/// A failure caused by the CLI being unable to touch its own state directory —
/// the one class Doctor can actually repair.
pub(crate) fn is_permission_failure(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("permission denied") || lower.contains("eacces") || lower.contains("eperm")
}

/// Append `chunk` to `tail`, keeping only the newest `DIAGNOSTIC_TAIL` chars.
///
/// Counted in chars, not bytes: a multi-byte path must never be split.
fn push_tail(tail: &mut String, chunk: &str) {
    tail.push_str(chunk);
    let length = tail.chars().count();
    if length > DIAGNOSTIC_TAIL {
        let skip = length - DIAGNOSTIC_TAIL;
        *tail = tail.chars().skip(skip).collect();
    }
}

/// First line of `stderr` that names a known startup failure.
///
/// Deliberately narrow: only a recognised signature is ever returned, because
/// stderr can carry a provider's error body and that body can contain a key.
pub(crate) fn startup_diagnosis(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            DIAGNOSTIC_SIGNATURES
                .iter()
                .any(|signature| lower.contains(signature))
        })
}

impl Transport {
    pub fn spawn(cmd: &mut Command) -> Result<Self, String> {
        grouped(cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start Pi: {e}"))?;
        let pipes = (|| {
            let stdin = child.stdin.take().ok_or("Pi stdin unavailable")?;
            let stdout = child.stdout.take().ok_or("Pi stdout unavailable")?;
            let stderr = child.stderr.take().ok_or("Pi stderr unavailable")?;
            nonblocking(&stdin).map_err(|_| "Cannot configure Pi stdin")?;
            nonblocking(&stdout).map_err(|_| "Cannot configure Pi stdout")?;
            nonblocking(&stderr).map_err(|_| "Cannot configure Pi stderr")?;
            Ok::<_, &str>((stdin, stdout, stderr))
        })();
        let (stdin, stdout, stderr) = match pipes {
            Ok(pipes) => pipes,
            Err(error) => {
                signal_group(&mut child, true);
                let _ = child.wait();
                return Err(error.into());
            }
        };
        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout,
            stderr,
            framer: Framer::default(),
            writes: VecDeque::new(),
            offset: 0,
            queued_bytes: 0,
            diagnostic_bytes: 0,
            diagnostic_tail: String::new(),
        })
    }

    pub fn enqueue(&mut self, mut bytes: Vec<u8>) -> Result<(), String> {
        if self.stdin.is_none() {
            return Err("Pi stdin is closed".into());
        }
        if bytes.len() > MAX_COMMAND
            || self.writes.len() >= 16
            || self.queued_bytes + bytes.len() + 1 > MAX_WRITER_BYTES
        {
            return Err("Pi command queue is full; command was not sent".into());
        }
        bytes.push(b'\n');
        self.queued_bytes += bytes.len();
        self.writes.push_back(bytes);
        Ok(())
    }

    pub fn write_pending(&mut self) -> Result<(), String> {
        let Some(stdin) = &mut self.stdin else {
            return Ok(());
        };
        // A bounded work budget keeps cancellation responsive even with a fast reader.
        for _ in 0..16 {
            let Some(front) = self.writes.front() else {
                break;
            };
            let end = (self.offset + 16384).min(front.len());
            match stdin.write(&front[self.offset..end]) {
                Ok(0) => {
                    return Err(
                        "Pi stdin closed while delivering a command; delivery is unknown".into(),
                    );
                }
                Ok(n) => {
                    self.offset += n;
                    if self.offset == front.len() {
                        self.queued_bytes -= front.len();
                        self.writes.pop_front();
                        self.offset = 0;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(
                    "Pi command write failed; delivery is unknown (do not replay automatically)"
                        .into(),
                ),
            }
        }
        Ok(())
    }

    /// Discard unsent work, but never interleave abort into a partially written JSON record.
    pub fn abort(&mut self) {
        if self.offset == 0 {
            self.writes.clear();
        } else {
            self.writes.truncate(1);
        }
        self.queued_bytes = self.writes.iter().map(Vec::len).sum();
        for command in ["clear_queue", "abort"] {
            let _ = self.enqueue(
                format!("{{\"type\":\"{command}\",\"id\":\"halo-shutdown-{command}\"}}")
                    .into_bytes(),
            );
        }
    }

    pub fn close_stdin(&mut self) {
        self.stdin.take();
        self.writes.clear();
        self.offset = 0;
        self.queued_bytes = 0;
    }

    pub fn read_frame(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.framer.read(&mut self.stdout)
    }

    pub fn drain_diagnostics(&mut self) {
        let mut bytes = [0; 4096];
        for _ in 0..8 {
            match self.stderr.read(&mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    self.diagnostic_bytes = self.diagnostic_bytes.saturating_add(n);
                    push_tail(
                        &mut self.diagnostic_tail,
                        &String::from_utf8_lossy(&bytes[..n]),
                    );
                }
            }
        }
    }

    /// The line that explains a failure to start, if stderr contains one.
    ///
    /// Returns `None` unless the text matches a known startup signature, so a
    /// provider's error body (which can echo an API key) never reaches the UI.
    pub fn startup_diagnosis(&self) -> Option<&str> {
        startup_diagnosis(&self.diagnostic_tail)
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, String> {
        self.child
            .try_wait()
            .map_err(|_| "Failed to inspect Pi child status".into())
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        // Worker-only safety net. PiRun handle Drop never reaches this code.
        signal_group(&mut self.child, true);
        let _ = self.child.wait();
    }
}

/// No model call: bounded --version probe using the identical child-only launch environment.
pub(crate) fn version(cmd: &mut Command) -> Result<String, String> {
    grouped(cmd);
    cmd.arg("--version");
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Cannot execute Pi version probe: {e}"))?;
    child.stdin.take();
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or("Missing probe stdout")?;
        let mut stderr = child.stderr.take().ok_or("Missing probe stderr")?;
        nonblocking(&stdout).map_err(|_| "Cannot configure probe stdout")?;
        nonblocking(&stderr).map_err(|_| "Cannot configure probe stderr")?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut output = Vec::new();
        let mut diagnostic_size = 0;
        loop {
            let mut bytes = [0; 1024];
            for _ in 0..8 {
                match stdout.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => output.extend_from_slice(&bytes[..n]),
                }
            }
            for _ in 0..8 {
                match stderr.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => diagnostic_size += n,
                }
            }
            if output.len() > 4096 || diagnostic_size > 16384 {
                return Err("Pi version probe output exceeded its limit");
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|_| "Pi version probe wait failed")?
            {
                if !status.success() {
                    return Err(
                        "Pi --version failed; check the executable and its Node interpreter",
                    );
                }
                // Drain bytes written just before exit.
                let _ = stdout.take(4097).read_to_end(&mut output);
                if output.len() > 4096 {
                    return Err("Pi version probe output exceeded its limit");
                }
                return std::str::from_utf8(&output)
                    .map(|s| s.trim().to_owned())
                    .map_err(|_| "Invalid Pi version output");
            }
            if Instant::now() >= deadline {
                return Err("Pi --version timed out (3 seconds)");
            }
            std::thread::park_timeout(Duration::from_millis(10));
        }
    })();
    signal_group(&mut child, true);
    let _ = child.wait();
    result.map_err(str::to_owned)
}

/// Cross-platform bounded subprocess capture with `stdin(Stdio::null())`,
/// process-group cleanup on timeout, and a hard byte cap per stream.
pub(crate) fn output_bounded(
    cmd: &mut Command,
    timeout: Duration,
    max_bytes: usize,
) -> Result<std::process::Output, String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stdout_pipe.take() {
            let _ = pipe.take(max_bytes as u64).read_to_end(&mut buf);
        }
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stderr_pipe.take() {
            let _ = pipe.take(max_bytes as u64).read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_handle.join().unwrap_or_default();
                let stderr = stderr_handle.join().unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    signal_group(&mut child, true);
                    let _ = child.wait();
                    return Err(format!("timed out after {}s", timeout.as_secs()));
                }
                std::thread::park_timeout(Duration::from_millis(10));
            }
            Err(e) => {
                signal_group(&mut child, true);
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_permission_failure_is_recognised_and_quoted() {
        // The real line Pi prints when ~/.pi is root-owned.
        let stderr = "Error: EACCES: permission denied, mkdir \
                      '/Users/me/.pi/agent/sessions/--Users-me-project--'";
        let cause = startup_diagnosis(stderr).expect("cause");
        assert!(cause.contains("EACCES"));
        assert!(is_permission_failure(cause));
    }

    #[test]
    fn an_unrelated_provider_error_is_never_quoted() {
        // A provider body can echo a key, so only known signatures come back.
        let stderr = "Error: 401 invalid api key sk-live-abcdef1234567890";
        assert_eq!(startup_diagnosis(stderr), None);
        assert!(!is_permission_failure(stderr));
    }

    #[test]
    fn blank_lines_are_skipped() {
        let stderr = "\n\n   \ncommand not found: pi\n";
        assert_eq!(startup_diagnosis(stderr), Some("command not found: pi"));
    }

    #[test]
    fn empty_stderr_has_no_diagnosis() {
        assert_eq!(startup_diagnosis(""), None);
        assert!(!is_permission_failure(""));
    }

    #[test]
    fn the_tail_keeps_the_newest_chars() {
        // Newest bytes explain the exit; older ones are a log.
        let mut tail = String::new();
        for index in 0..500 {
            push_tail(&mut tail, &format!("line {index}\n"));
        }
        assert!(tail.chars().count() <= DIAGNOSTIC_TAIL);
        assert!(tail.ends_with("line 499\n"), "newest line must survive");
        assert!(!tail.contains("line 0\n"), "oldest line must be dropped");
    }

    #[test]
    fn truncation_never_splits_a_multibyte_char() {
        let mut tail = String::new();
        push_tail(&mut tail, &"é".repeat(DIAGNOSTIC_TAIL + 10));
        assert_eq!(tail.chars().count(), DIAGNOSTIC_TAIL);
    }
}

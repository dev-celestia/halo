//! Google browser OAuth for the Antigravity ACP harness.
//!
//! `agy_acp_server` implements ACP `authenticate` with `oauth-personal`: it
//! starts a loopback server, prints the sign-in URL to stderr, opens the
//! browser (`BROWSER` is `/usr/bin/open` or `xdg-open`), and waits up to 300 s
//! for the redirect. Tokens land in the macOS keychain
//! (`gemini` / `antigravity-acp`) or `$GEMINI_HOME/antigravity-acp/acp_token.json`.
//!
//! Halo captures the URL from stderr for the waiting state. A stored refresh
//! token finishes `authenticate` with no browser.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::jsonrpc::RpcClient;
use super::run::spawn_server;
use super::types::AntigravityProbeResult;
use super::{install, probe};

const AUTH_METHOD: &str = "oauth-personal";
const AUTH_PROMPT: &str = "Open the following link to authenticate the ACP server: ";
/// Server-side loopback timeout is 300 s; leave room for the token exchange.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(6 * 60);
const INIT_TIMEOUT: Duration = Duration::from_secs(90);
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(45);

const TOKEN_DIR: &str = "antigravity-acp";
const TOKEN_FILE: &str = "acp_token.json";
const KEYCHAIN_SERVICE: &str = "gemini";
const KEYCHAIN_ACCOUNT: &str = "antigravity-acp";

/// Status cache TTL — `is_signed_in` may shell out to `security`.
const STATUS_TTL: Duration = Duration::from_secs(2);

static STATUS_CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Events surfaced while a sign-in is in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityAuthEvent {
    /// Sign-in URL emitted by the ACP server; the browser has been opened.
    AuthUrl(String),
    /// `authenticate` returned successfully.
    SignedIn,
}

/// `$GEMINI_HOME/antigravity-acp/acp_token.json` (secure-file backend).
pub fn token_path() -> PathBuf {
    probe::gemini_home().join(TOKEN_DIR).join(TOKEN_FILE)
}

/// Whether the ACP server has stored Google credentials (file or keychain).
pub fn is_signed_in() -> bool {
    let mut cache = STATUS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, signed)) = *cache {
        if at.elapsed() < STATUS_TTL {
            return signed;
        }
    }
    let signed = token_file_has_refresh_token() || keychain_entry_exists();
    *cache = Some((Instant::now(), signed));
    signed
}

/// Drop the cached sign-in answer after a login/logout.
fn invalidate_status_cache() {
    let mut cache = STATUS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *cache = None;
}

/// Runs ACP `authenticate` with `oauth-personal`, opening the browser with the
/// URL the server prints. Blocks until the flow completes or times out.
pub fn run_login(
    custom_executable: Option<&str>,
    mut on_event: impl FnMut(AntigravityAuthEvent) + Send,
) -> Result<(), String> {
    let executable = match probe::probe_environment(custom_executable) {
        AntigravityProbeResult::Ready { path, .. } => path,
        AntigravityProbeResult::Missing => install::ensure_installed().map_err(|e| {
            format!(
                "Antigravity ACP server missing and install failed: {e}. Install from settings or set ANTIGRAVITY_ACP_EXECUTABLE."
            )
        })?,
        AntigravityProbeResult::Failed { message } => return Err(message),
    };

    let cwd = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut child = spawn_server(&executable, &cwd, &Default::default())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "agy_acp_server has no stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agy_acp_server has no stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "agy_acp_server has no stderr".to_string())?;

    // stdout is the JSON-RPC stream; the sign-in URL arrives on stderr.
    let (url_tx, url_rx) = mpsc::channel::<String>();
    thread::Builder::new()
        .name("antigravity-acp-auth-stderr".into())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let text = line.trim();
                if text.is_empty() {
                    continue;
                }
                if let Some(url) = auth_url_from_line(text) {
                    if url_tx.send(url).is_err() {
                        break;
                    }
                }
                tracing::debug!(target: "antigravity.stderr", "{text}");
            }
        })
        .map_err(|_| "Cannot start Antigravity sign-in log reader".to_string())?;

    let (client, _incoming) = RpcClient::start(stdin, stdout);
    let init = client.request_timeout(
        "initialize",
        json!({
            "protocolVersion": 1,
            "clientInfo": {
                "name": "halo",
                "title": "Halo",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "clientCapabilities": {
                "fs": { "readTextFile": false, "writeTextFile": false },
                "terminal": false,
            },
        }),
        INIT_TIMEOUT,
    )?;

    let method = preferred_auth_method(&init);
    let (done_tx, done_rx) = mpsc::channel::<Result<Value, String>>();
    let auth_client = client.clone();
    thread::Builder::new()
        .name("antigravity-acp-authenticate".into())
        .spawn(move || {
            let result = auth_client.request_timeout(
                "authenticate",
                json!({ "methodId": method }),
                LOGIN_TIMEOUT,
            );
            let _ = done_tx.send(result);
        })
        .map_err(|_| "Cannot start Antigravity sign-in worker".to_string())?;

    let deadline = Instant::now() + LOGIN_TIMEOUT + Duration::from_secs(15);
    let outcome = loop {
        while let Ok(url) = url_rx.try_recv() {
            // The server opens this URL via `BROWSER`. Open here too only when
            // that handoff did not already launch a browser (non-Unix).
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            open_browser(&url);
            on_event(AntigravityAuthEvent::AuthUrl(url));
        }
        match done_rx.recv_timeout(Duration::from_millis(150)) {
            Ok(Ok(_)) => break Ok(()),
            Ok(Err(err)) => break Err(err),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline {
                    break Err("Timed out waiting for Google sign-in".into());
                }
                if let Ok(Some(status)) = child.try_wait() {
                    break Err(format!(
                        "Antigravity ACP server exited during sign-in ({status})"
                    ));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("Antigravity sign-in ended unexpectedly".into());
            }
        }
    };

    let _ = child.kill();
    let _ = child.wait();
    outcome?;
    invalidate_status_cache();
    on_event(AntigravityAuthEvent::SignedIn);
    Ok(())
}

/// Signs out by asking the ACP server to clear both stored token blobs.
pub fn logout(custom_executable: Option<&str>) -> Result<(), String> {
    let result = logout_via_server(custom_executable);
    invalidate_status_cache();
    match result {
        Ok(()) => Ok(()),
        Err(server_err) => {
            // The server may be missing; clear what we can on disk and let the
            // keychain CLI remove its entry too.
            clear_stored_credentials();
            invalidate_status_cache();
            if is_signed_in() {
                Err(server_err)
            } else {
                Ok(())
            }
        }
    }
}

fn logout_via_server(custom_executable: Option<&str>) -> Result<(), String> {
    let executable = match probe::probe_environment(custom_executable) {
        AntigravityProbeResult::Ready { path, .. } => path,
        AntigravityProbeResult::Missing => {
            return Err("Antigravity ACP server is not installed".into());
        }
        AntigravityProbeResult::Failed { message } => return Err(message),
    };
    let cwd = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut child = spawn_server(&executable, &cwd, &Default::default())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "agy_acp_server has no stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agy_acp_server has no stdout".to_string())?;
    if let Some(stderr) = child.stderr.take() {
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                tracing::debug!(target: "antigravity.stderr", "{line}");
            }
        });
    }
    let (client, _incoming) = RpcClient::start(stdin, stdout);
    let result = client
        .request_timeout(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientInfo": {
                    "name": "halo",
                    "title": "Halo",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
            INIT_TIMEOUT,
        )
        .and_then(|_| client.request_timeout("logout", json!({}), LOGOUT_TIMEOUT));
    let _ = child.kill();
    let _ = child.wait();
    result.map(|_| ())
}

/// Pull the sign-in URL out of one stderr line, if it carries the prompt.
pub(super) fn auth_url_from_line(line: &str) -> Option<String> {
    let text = line.trim();
    let pos = text.find(AUTH_PROMPT)?;
    let url = text[pos + AUTH_PROMPT.len()..].trim();
    (!url.is_empty()).then(|| url.to_string())
}

/// Method id to `authenticate` with: `oauth-personal` when offered, else the
/// first one listed. Also tells the caller whether `authenticate` is callable.
pub(super) fn preferred_auth_method(init: &Value) -> String {
    let methods: Vec<&str> = init
        .get("authMethods")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    if methods.iter().any(|id| *id == AUTH_METHOD) {
        AUTH_METHOD.to_string()
    } else {
        methods
            .first()
            .map(|id| (*id).to_string())
            .unwrap_or_else(|| AUTH_METHOD.to_string())
    }
}

fn token_file_has_refresh_token() -> bool {
    let Ok(text) = std::fs::read_to_string(token_path()) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    value
        .get("refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.trim().is_empty())
}

#[cfg(target_os = "macos")]
fn keychain_entry_exists() -> bool {
    Command::new(SECURITY_CLI)
        .args([
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            KEYCHAIN_ACCOUNT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn keychain_entry_exists() -> bool {
    false
}

#[cfg(target_os = "macos")]
const SECURITY_CLI: &str = "/usr/bin/security";

fn clear_stored_credentials() {
    let _ = std::fs::remove_file(token_path());
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new(SECURITY_CLI)
            .args([
                "delete-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                KEYCHAIN_ACCOUNT,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg_attr(any(target_os = "macos", target_os = "linux"), allow(dead_code))]
pub(super) fn open_browser(url: &str) {
    let _ = (|| -> Result<(), std::io::Error> {
        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/C", "start", "", url])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = url;
        }
        Ok(())
    })();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_oauth_personal_when_advertised() {
        let init = json!({
            "authMethods": [
                { "id": "gemini-api-key" },
                { "id": "oauth-personal" },
            ]
        });
        assert_eq!(preferred_auth_method(&init), "oauth-personal");
    }

    #[test]
    fn extracts_sign_in_url_from_stderr_line() {
        let line = "I1030 x main.py:210] Open the following link to authenticate the ACP server: https://accounts.google.com/o/oauth2/auth?x=1";
        assert_eq!(
            auth_url_from_line(line).as_deref(),
            Some("https://accounts.google.com/o/oauth2/auth?x=1")
        );
        assert_eq!(auth_url_from_line("Starting AGY ACP Server..."), None);
        assert_eq!(
            auth_url_from_line("Open the following link to authenticate the ACP server: "),
            None
        );
    }

    #[test]
    fn falls_back_to_first_advertised_method() {
        let init = json!({ "authMethods": [{ "id": "oauth-business" }] });
        assert_eq!(preferred_auth_method(&init), "oauth-business");
        assert_eq!(preferred_auth_method(&Value::Null), "oauth-personal");
    }

    #[test]
    fn token_file_requires_a_refresh_token() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: held under LOCK for this unit test.
        unsafe {
            std::env::set_var("GEMINI_HOME", dir.path());
        }
        let token_dir = dir.path().join(TOKEN_DIR);
        std::fs::create_dir_all(&token_dir).unwrap();
        assert!(!token_file_has_refresh_token());
        std::fs::write(token_dir.join(TOKEN_FILE), r#"{"refresh_token":""}"#).unwrap();
        assert!(!token_file_has_refresh_token());
        std::fs::write(token_dir.join(TOKEN_FILE), r#"{"refresh_token":"1//abc"}"#).unwrap();
        assert!(token_file_has_refresh_token());
        assert_eq!(
            token_path(),
            token_dir.join(TOKEN_FILE),
            "token path must follow GEMINI_HOME"
        );
        unsafe {
            std::env::remove_var("GEMINI_HOME");
        }
    }
}

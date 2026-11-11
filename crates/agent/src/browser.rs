//! Browser tool status and command classification for the agent surface.
//!
//! Resolution and the version probe spawn a process, so callers run
//! [`browser_status`] off the UI thread (see `Bridge::halo_agent_refresh_browser`).

use serde::{Deserialize, Serialize};

use halo_browser_tools::Engine as _;
use halo_browser_tools::agent_browser::{AgentBrowserEngine, resolve_bin, resolve_cli, version};

/// What the agent surface can say about browser tools.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserStatus {
    pub available: bool,
    pub binary: Option<String>,
    /// The binary's own `--version`. It can lag the package version.
    pub version: Option<String>,
    /// The `halo-browser` front the agent is told to run. Absent from a build
    /// that never compiled it, which makes `binary` irrelevant.
    #[serde(default)]
    pub cli: Option<String>,
    /// `HALO_BROWSER_ALLOWED_DOMAINS`, empty meaning "any host". Surfaced so
    /// the row can say which mode the agent is in — a silent policy is a
    /// policy nobody can check.
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    /// The app-level default session. Runs override it per chat — see
    /// [`session_for`].
    pub session: String,
    /// Set when agent-browser attaches to a browser Halo already owns.
    pub cdp: Option<String>,
    /// One sentence for the settings row.
    pub detail: String,
}

/// The browser session a chat owns.
///
/// The rule itself lives in [`halo_browser_tools::agent_browser::session_for`]:
/// the live pane has to name the same session without depending on this crate,
/// so there is exactly one spelling of it.
pub fn session_for(chat_id: &str) -> String {
    halo_browser_tools::agent_browser::session_for(chat_id)
}

/// Reports the same gates the prompt uses, so the row cannot say "Ready"
/// while the agent is told nothing: the `halo-browser` front must exist *and*
/// the `agent-browser` binary it drives must be installed.
pub fn browser_status() -> BrowserStatus {
    let session = std::env::var("HALO_BROWSER_SESSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "halo".to_string());
    let cdp = std::env::var("HALO_BROWSER_CDP")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let cli = resolve_cli().map(|path| path.display().to_string());

    let (available, binary, version, detail) = match resolve_bin(None) {
        Ok(bin) => {
            let version = version(&bin);
            let binary = Some(bin.display().to_string());
            if cli.is_some() {
                (
                    true,
                    binary,
                    version,
                    "Browser tools are available to the agent.".to_string(),
                )
            } else {
                (
                    false,
                    binary,
                    version,
                    "halo-browser is missing from this build, so the agent cannot reach a \
                     browser. Build it with `cargo build -p halo-browser-tools --bin \
                     halo-browser`."
                        .to_string(),
                )
            }
        }
        Err(error) => (false, None, None, error),
    };

    BrowserStatus {
        available,
        binary,
        version,
        cli,
        allowed_domains: halo_browser_tools::policy::allowed_domains_from_env(),
        session,
        cdp,
        detail,
    }
}

/// Tear down a chat's browser session. The daemon idles for an hour on its own,
/// so this is about not leaving a Chrome behind, not about correctness.
pub fn browser_close(session: &str) -> Result<(), String> {
    let bin = resolve_bin(None)?;
    let cdp = std::env::var("HALO_BROWSER_CDP")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let mut engine = AgentBrowserEngine::new(bin, session.to_string(), cdp);
    engine.close()
}

/// Tear down a chat that is being deleted: close it *and* erase its state.
///
/// Closing alone leaves the profile — cookies, localStorage, any login the
/// agent established — on disk under a session name nobody will look at again.
/// Deleting a chat should mean deleting what it accumulated.
pub fn browser_forget(session: &str) -> Result<(), String> {
    let closed = browser_close(session);
    // Remove even when the close failed: a stray Chrome is less harmful than
    // cookies that outlive the chat it belonged to.
    let _ = std::fs::remove_dir_all(halo_browser_tools::profile_dir(session));
    let _ = std::fs::remove_dir_all(halo_browser_tools::download_dir(session));
    let _ = std::fs::remove_file(halo_browser_tools::state_path(session));
    closed
}

/// Close every session in one call. Used on quit and by the settings row.
pub fn browser_close_all() -> Result<(), String> {
    halo_browser_tools::agent_browser::close_all()
}

/// A `halo-browser` invocation recognised inside a shell command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCommand {
    /// `open`, `snapshot`, `act`, `read`, `close`, `status`.
    pub action: String,
    /// The thing the action targets: a URL, a ref, or a selector.
    pub target: String,
}

/// Recognise `halo-browser <verb> …` inside a shell command line.
///
/// The CLI front is how the harness actually reaches the browser, so a plain
/// `bash` tool call is the only place the transcript can learn that the agent
/// drove a page. Anything else (a pipeline, a variable) is left as an exec.
pub fn classify_command(command: &str) -> Option<BrowserCommand> {
    let mut words = command.split_whitespace();
    if words.next()? != "halo-browser" {
        return None;
    }
    let action = words.next()?.to_string();
    if !matches!(
        action.as_str(),
        "open" | "snapshot" | "act" | "read" | "close" | "status"
    ) {
        return None;
    }

    let target = match action.as_str() {
        "open" => words.next().unwrap_or_default().to_string(),
        "act" => words
            .clone()
            .position(|word| word == "--ref")
            .and_then(|index| words.nth(index + 1))
            .unwrap_or_default()
            .to_string(),
        "read" => words
            .clone()
            .position(|word| word == "--selector")
            .and_then(|index| words.nth(index + 1))
            .unwrap_or_default()
            .to_string(),
        _ => words.next().unwrap_or_default().to_string(),
    };

    Some(BrowserCommand { action, target })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_open() {
        let command = classify_command("halo-browser open https://example.com").unwrap();
        assert_eq!(command.action, "open");
        assert_eq!(command.target, "https://example.com");
    }

    #[test]
    fn recognises_act_ref() {
        let command =
            classify_command("halo-browser act --ref @e12 --gen 3 --action click").unwrap();
        assert_eq!(command.action, "act");
        assert_eq!(command.target, "@e12");
    }

    #[test]
    fn ignores_other_commands() {
        assert!(classify_command("cargo test").is_none());
        assert!(classify_command("agent-browser open example.com").is_none());
        assert!(classify_command("halo-browser --help").is_none());
    }

    #[test]
    fn session_is_per_chat_and_path_safe() {
        assert_eq!(session_for("a3f9-c1"), "chat-a3f9-c1");
        assert_eq!(session_for("  "), "halo");
        // A title with spaces or slashes must not become a path or a flag.
        assert_eq!(session_for("my chat/one"), "chat-my-chat-one");
        assert!(session_for(&"x".repeat(200)).chars().count() <= 53);
    }
}

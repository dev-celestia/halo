//! agent-browser as an [`Engine`].
//!
//! Everything specific to the CLI lives here: argument shapes, the session flag,
//! and the `get text body` fallback. Nothing above this module knows the tool is
//! a subprocess.

mod exec;
mod probe;

use std::time::Duration;

/// Chrome wants the pointer move to land in its own frame before a press at
/// that point, or the press goes to wherever the pointer used to be.
const MOVE_SETTLE: u64 = 60;

/// A press and its release in the same task is swallowed by frameworks that
/// look for a distinct `mouseup` — a click that reports success and does
/// nothing is the usual symptom.
const BUTTON_SETTLE: u64 = 40;

pub use probe::{ready, resolve_bin, resolve_cli, version};

/// The browser session a chat owns.
///
/// One session per chat, not per app: the generation guard, the persisted
/// counter and the daemon's tab all key off this name, so two chats sharing one
/// would reject each other's refs and steal each other's page.
///
/// It lives here, not in the agent crate, because the live pane has to name the
/// same session the agent ran under — and the pane is not allowed to depend on
/// the agent. One function, one spelling.
pub fn session_for(chat_id: &str) -> String {
    let trimmed = chat_id.trim();
    if trimmed.is_empty() {
        return "halo".to_string();
    }
    let safe: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Chrome profile dirs have a length limit; a full uuid is fine, a pasted
    // title is not.
    let safe = safe.trim_matches('-');
    let short: String = safe.chars().take(48).collect();
    format!("chat-{short}")
}

/// The first `ws://` URL in a chunk of CLI output.
fn find_ws_url(output: &str) -> Option<String> {
    let start = output.find("ws://")?;
    let rest = &output[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Close every session on this machine, in one call.
///
/// One process instead of one per chat, which is what makes this cheap enough
/// to run during app quit. `--session` is deliberately omitted: with no session
/// the command addresses the daemon's whole set.
pub fn close_all() -> ToolResult<()> {
    let bin = resolve_bin(None)?;
    Cli::new(bin, String::new(), None)
        .run(&["close", "--all"])
        .map(|_| ())
}

use crate::engine::Engine;
use crate::ToolResult;
use exec::Cli;

pub struct AgentBrowserEngine {
    cli: Cli,
}

impl AgentBrowserEngine {
    pub fn new(bin: std::path::PathBuf, session: String, cdp: Option<String>) -> Self {
        Self {
            cli: Cli::new(bin, session, cdp),
        }
    }

    /// `HALO_BROWSER_BIN`, `HALO_BROWSER_SESSION`, `HALO_BROWSER_CDP`,
    /// `HALO_BROWSER_ALLOWED_DOMAINS`.
    ///
    /// With `HALO_BROWSER_CDP` set the CLI attaches to a browser Halo already
    /// owns (proxy, CA and profile included) instead of launching its own.
    ///
    /// The profile and download directory are forced to Halo's cache: the agent
    /// gets its own Chrome state, and a deleted chat can take its cookies with
    /// it instead of leaving them in agent-browser's shared default.
    pub fn from_env() -> ToolResult<Self> {
        let session = std::env::var("HALO_BROWSER_SESSION")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "halo".to_string());
        Self::for_session(&session)
    }

    /// The engine for one named session, ignoring `HALO_BROWSER_SESSION`.
    ///
    /// This is how the live pane attaches: it is not the process the agent ran
    /// in, so it has no session env var to inherit — it derives the name from
    /// the chat id instead, via [`session_for`].
    pub fn for_session(session: &str) -> ToolResult<Self> {
        let bin = resolve_bin(None)?;
        let session = session.trim();
        let session = if session.is_empty() { "halo" } else { session };
        let cdp = std::env::var("HALO_BROWSER_CDP")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let cli = Cli::new(bin, session.to_string(), cdp)
            .hardened(session, crate::policy::allowed_domains_from_env());
        Ok(Self { cli })
    }

    pub fn session(&self) -> &str {
        &self.cli.session
    }

    /// The screencast address for this session, enabling streaming if needed.
    ///
    /// `stream status` answers with a human line — `Streaming enabled on
    /// ws://127.0.0.1:53375` — and streaming is switched on by the daemon as
    /// soon as a page exists, so the enable branch is the rare path.
    pub fn stream_url(&mut self) -> ToolResult<String> {
        if let Ok(output) = self.cli.run(&["stream", "status"])
            && let Some(url) = find_ws_url(&output)
        {
            return Ok(url);
        }
        let output = self.cli.run(&["stream", "enable"])?;
        find_ws_url(&output).ok_or_else(|| format!("stream enable returned no address: {output}"))
    }

    // ── Viewport input ──────────────────────────────────────────────────
    //
    // These are *not* on [`Engine`]: the agent drives the page by reference
    // (`click @e3`), which is stable across layout. Coordinates are what the
    // live pane has, and they go stale the moment the page scrolls — so they
    // belong to the human watching, not to the model. The screencast socket is
    // read-only, so every gesture is a CLI round trip; the pane queues them on
    // one thread to keep them ordered.

    /// Move the pointer, in CSS pixels of the page viewport.
    pub fn mouse_move(&mut self, x: i32, y: i32) -> ToolResult<()> {
        self.cli
            .run(&["mouse", "move", &x.to_string(), &y.to_string()])
            .map(|_| ())
    }

    /// Press or release a button at the pointer's current position.
    pub fn mouse_button(&mut self, button: &str, down: bool) -> ToolResult<()> {
        let action = if down { "down" } else { "up" };
        self.cli.run(&["mouse", action, button]).map(|_| ())
    }

    /// Scroll by `dy` CSS pixels (positive is down).
    pub fn wheel(&mut self, dy: i32) -> ToolResult<()> {
        self.cli.run(&["mouse", "wheel", &dy.to_string()]).map(|_| ())
    }

    /// Press the left button at `(x, y)`.
    ///
    /// The move and the press are separate processes, and Chrome dispatches the
    /// press at wherever the pointer was when *its* frame ran, so the two are
    /// spaced out — without the gap the press lands at the old position.
    pub fn mouse_down_at(&mut self, x: i32, y: i32) -> ToolResult<()> {
        self.mouse_move(x, y)?;
        std::thread::sleep(Duration::from_millis(MOVE_SETTLE));
        self.mouse_button("left", true)
    }

    /// Release the left button where the pointer already is.
    pub fn mouse_up(&mut self) -> ToolResult<()> {
        self.mouse_button("left", false)
    }

    /// A full click at `(x, y)`.
    pub fn click_at(&mut self, x: i32, y: i32) -> ToolResult<()> {
        self.mouse_down_at(x, y)?;
        // A press and its release in the same task is swallowed by frameworks
        // that look for a distinct `mouseup`.
        std::thread::sleep(Duration::from_millis(BUTTON_SETTLE));
        self.mouse_up()
    }

    /// Type text as real keystrokes, into whatever holds focus.
    pub fn type_text(&mut self, text: &str) -> ToolResult<()> {
        self.cli.run(&["keyboard", "type", text]).map(|_| ())
    }

    /// Press one named key (`Enter`, `Tab`, `ArrowLeft`, `Control+a`).
    pub fn press_key(&mut self, key: &str) -> ToolResult<()> {
        self.cli.run(&["press", key]).map(|_| ())
    }
}

impl Engine for AgentBrowserEngine {
    fn navigate(&mut self, url: &str) -> ToolResult<()> {
        self.cli.run(&["open", url]).map(|_| ())
    }

    fn snapshot_raw(&mut self, scope: Option<&str>, interactive: bool) -> ToolResult<String> {
        let mut args = vec!["snapshot"];
        if interactive {
            args.push("-i");
        }
        if let Some(scope) = scope.map(str::trim).filter(|value| !value.is_empty()) {
            args.push("-s");
            args.push(scope);
        }
        self.cli.run(&args)
    }

    fn click(&mut self, reference: &str) -> ToolResult<()> {
        self.cli.run(&["click", reference]).map(|_| ())
    }

    fn fill(&mut self, reference: &str, value: &str) -> ToolResult<()> {
        self.cli.run(&["fill", reference, value]).map(|_| ())
    }

    fn value(&mut self, reference: &str) -> ToolResult<String> {
        self.cli.run(&["get", "value", reference])
    }

    fn focus(&mut self, reference: &str) -> ToolResult<()> {
        self.cli.run(&["focus", reference]).map(|_| ())
    }

    fn press(&mut self, key: &str) -> ToolResult<()> {
        self.cli.run(&["press", key]).map(|_| ())
    }

    fn text(&mut self, selector: Option<&str>) -> ToolResult<String> {
        match selector.map(str::trim).filter(|value| !value.is_empty()) {
            Some(selector) => self.cli.run(&["get", "text", selector]),
            // Older builds require a selector; fall back rather than fail the
            // fingerprint for a missing convenience.
            None => self
                .cli
                .run(&["get", "text", "body"])
                .or_else(|_| self.cli.run(&["get", "text"])),
        }
    }

    fn url(&mut self) -> ToolResult<String> {
        self.cli.run(&["get", "url"])
    }

    fn title(&mut self) -> ToolResult<String> {
        self.cli.run(&["get", "title"])
    }

    fn close(&mut self) -> ToolResult<()> {
        self.cli.run(&["close"]).map(|_| ())
    }
}

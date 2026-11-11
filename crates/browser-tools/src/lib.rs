//! # halo-browser-tools
//!
//! Browser tools for Halo agents, written against a contract rather than a browser.
//!
//! The agent sees four tools — open, snapshot, act, read. Everything that makes
//! those tools safe to call lives here, not in the harness:
//!
//! * **Stale-ref guard.** agent-browser reassigns `@eN` on every snapshot, so a
//!   cached ref can silently address a different element (`@e29` was "Search" on
//!   one page and "Privacy Policy" on the next, and `click` still printed
//!   `✓ Done`). Every snapshot publishes a generation; `act` refuses a ref from
//!   an older one.
//! * **Size budget.** A Wikipedia article snapshots to 71,590 chars (~18k
//!   tokens) with `-i` alone. Output is truncated on a line boundary with an
//!   explicit hint, and `scope` narrows at the source.
//! * **Verify after act.** A successful action proves dispatch, not change. The
//!   fingerprint (url + title + body-text hash) decides whether the page moved,
//!   which is also what decides whether the generation advances.
//! * **Scheme and host policy.** Only http(s); `file:` reads the local disk,
//!   `javascript:` runs in the page's origin, `data:`/`blob:` bypass a host
//!   allowlist. See [`policy`].
//!
//! The browser itself sits behind [`Engine`]. `AgentBrowserEngine` drives a
//! separate browser over the CLI; everything above the trait is shared.

pub mod agent_browser;
pub mod browser;
pub mod engine;
pub mod mcp;
pub mod policy;
pub mod stream;
pub mod types;

pub use browser::{
    cache_dir, download_dir, profile_dir, state_path, Browser, DEFAULT_MAX_CHARS, HARD_MAX_CHARS,
};
pub use engine::Engine;
pub use types::{
    Action, ActOutcome, ActRequest, Fingerprint, NavOutcome, Snapshot, SnapshotRequest, Truncation,
};

/// Every failure in this crate is a message the calling agent can act on.
pub type ToolResult<T> = Result<T, String>;

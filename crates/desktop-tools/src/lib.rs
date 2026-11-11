//! # halo-desktop-tools
//!
//! Computer-use tools for Halo agents, written against a contract rather than a
//! window server.
//!
//! The agent sees one CLI — `halo-desktop` — which drives `agent-desktop` for it.
//! Everything that makes those calls safe lives here, not in the harness:
//!
//! * **Stale-ref guard.** `agent-desktop` refs look like `@s8f3k2p9:e12`. The
//!   engine still resolves a ref from an older snapshot, so this crate records
//!   the generation and the snapshot id and refuses a mismatch *before*
//!   spawning anything.
//! * **Rendering budget.** The engine bounds its own tree (`complete`,
//!   `truncated`, `nodes_observed`), which is a different thing from how many
//!   characters the model can afford. An accessibility tree is larger than a DOM:
//!   the text is cut on a line boundary with an explicit narrowing hint.
//! * **Policy.** Some apps are never addressable (Passwords, System Settings,
//!   Halo itself), some fields are never readable (`AXSecureTextField`), and
//!   physical input is off unless the user turned it on. See [`policy`].
//! * **One writer.** Two chats driving one desktop fight over focus, so a lease
//!   names the chat that owns it. It is a UX guard, not a correctness lock — the
//!   engine already isolates per-session refstores.
//!
//! The desktop itself sits behind [`Engine`]. `AgentDesktopEngine` shells out to
//! the `agent-desktop` binary; everything above the trait is shared, so a test
//! double inherits the guard, the budget and the policy for free.

pub mod agent_desktop;
pub mod desktop;
pub mod engine;
pub mod lease;
pub mod permissions;
pub mod policy;
pub mod render;
pub mod types;

pub use desktop::{cache_dir, engine_home, Desktop};
pub use engine::Engine;
pub use types::{
    ActAction, ActOutcome, ActRequest, EngineStatus, FindRequest, PermissionState, RawSnapshot,
    Snapshot, SnapshotRequest, Truncation,
};

/// Default rendering budget for one snapshot, in characters.
pub const DEFAULT_MAX_CHARS: usize = 4_000;

/// No caller may exceed this, whatever it asks for.
pub const HARD_MAX_CHARS: usize = 16_000;

/// Every failure in this crate is a message the calling agent can act on.
pub type ToolResult<T> = Result<T, String>;

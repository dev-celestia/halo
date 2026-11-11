//! The single door from the surface to a PTY.
//!
//! Kept deliberately tiny so this crate can be shared by Halo's Coder dock and
//! the Terminal app without depending on `halo-bridge`.

use gpui::App;

/// One non-consuming read of a session's output stream.
///
/// Mirrors `halo_bridge::TerminalRead`; adapters convert between them so the
/// widget crate never links the bridge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalRead {
    /// Session epoch — a change means the reader must reset its grid.
    pub epoch: u64,
    /// Absolute offset to pass as `cursor` on the next read.
    pub seq: u64,
    pub alive: bool,
    /// The cursor fell out of the retained window; `data` starts at the oldest
    /// retained byte, so reset the grid before feeding it.
    pub truncated: bool,
    pub data: String,
}

/// PTY access used by [`crate::TerminalSurface`].
pub trait TerminalHost: 'static {
    /// Everything the surface hasn't seen yet. `None` when the session is gone.
    fn read(&self, session: &str, cursor: u64, cx: &App) -> Option<TerminalRead>;

    /// Raw bytes to the shell (keys, paste, mouse reports).
    fn write(&self, session: &str, data: &[u8], cx: &mut App);

    /// Cell grid changed; tell the PTY so full-screen apps redraw.
    fn resize(&self, session: &str, rows: u16, cols: u16, cx: &mut App);

    /// The surface gained keyboard focus — pages use this to track the focused
    /// pane of a split layout.
    fn focused(&self, _session: &str, _cx: &mut App) {}
}

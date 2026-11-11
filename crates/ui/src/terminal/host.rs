//! Bridge-backed [`TerminalHost`] for [`halo_terminal_ui::TerminalSurface`].
//!
//! The widget crate has no `halo-bridge` dependency on purpose: it only needs
//! four operations, and mirroring the read struct here keeps the two crates
//! independent. Everything else (layout, tabs, liveness) is the page's job.

use gpui::App;
use halo_bridge::Bridge;
use halo_terminal_ui::{TerminalHost, TerminalRead};

/// Routes surface traffic to `BridgeState::terminal_sessions`.
pub struct BridgeTerminalHost;

impl TerminalHost for BridgeTerminalHost {
    fn read(&self, session: &str, cursor: u64, cx: &App) -> Option<TerminalRead> {
        Bridge::terminal_read(session, cursor, cx).map(|read| TerminalRead {
            epoch: read.epoch,
            seq: read.seq,
            alive: read.alive,
            truncated: read.truncated,
            data: read.data,
        })
    }

    fn write(&self, session: &str, data: &[u8], cx: &mut App) {
        // Typing into a session that died between frames is a no-op, not a
        // failure: the surface keeps accepting keys until the exit event lands.
        let _ = Bridge::terminal_write(session, data, cx);
    }

    fn resize(&self, session: &str, rows: u16, cols: u16, cx: &mut App) {
        let _ = Bridge::terminal_resize(session, rows, cols, cx);
    }

    /// A click focused the surface: select the matching Coder dock tab when
    /// the session belongs to that dock.
    fn focused(&self, session: &str, cx: &mut App) {
        let belongs = Bridge::state(cx)
            .read(cx)
            .coder
            .terminals
            .iter()
            .any(|t| t.id == session);
        if belongs {
            let _ = Bridge::coder_terminal_select(session, cx);
        }
    }
}

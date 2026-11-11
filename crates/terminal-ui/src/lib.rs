//! Reusable GPUI terminal surface.
//!
//! One entity renders one PTY session. Stacking entities (rather than one
//! surface with an internal tab map) is what makes split panes work: each pane
//! owns its own focus handle, cursor, selection, link map, and mouse capture.
//!
//! ```ignore
//! let surface = cx.new(|cx| TerminalSurface::new(host.clone(), cx));
//! surface.update(cx, |s, cx| {
//!     s.bind(Some("pane-session-id".into()));
//!     s.set_settings(13, 5_000);
//!     s.sync(cx);
//! });
//! ```
//!
//! The crate deliberately knows nothing about Halo's bridge or work tabs: the
//! host trait below is the only door to a PTY.

mod color;
mod grid;
mod host;
mod keys;
mod link;
mod surface;

pub use grid::{
    CellStyle, GRID_PAD_X, GRID_PAD_Y, RenderedLink, SELECTION_BG_DARK, SELECTION_BG_LIGHT,
    SessionVt, TermSize, cell_metrics, render_grid,
};
pub use host::{TerminalHost, TerminalRead};
pub use link::{LinkSpan, find_links};
pub use surface::TerminalSurface;

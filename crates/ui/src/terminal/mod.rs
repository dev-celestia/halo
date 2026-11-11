//! Window-local terminal surfaces for the pages that host them.
//!
//! [`TerminalSurfaces`] owns one [`TerminalSurface`] entity per PTY session id
//! and survives page/tab switches, so a shell's scrollback is parsed once and
//! every view (Coder dock, Terminal page) draws the same entity. Runtime bytes
//! stay in the bridge store; this module only keeps the VT grids.

mod host;
mod surfaces;

pub use host::BridgeTerminalHost;
pub use surfaces::TerminalSurfaces;

//! Shared DTOs that cross the UI ↔ backend boundary.
//!
//! Split by feature area: HTTP history ([`history`]), repeater/collections
//! ([`collections`]), intruder ([`intruder`]), crawler ([`crawler`]), mock
//! forge ([`forge`], [`mock_forge`]), notes ([`notes`]), port scanner
//! ([`port_scanner`]), plus shared enums and events.

mod collections;
mod crawler;
mod debug_log;
mod events;
mod forge;
mod history;
mod intruder;
mod mock_forge;
mod notes;
mod port_scanner;
mod settings;

pub use collections::*;
pub use crawler::*;
pub use debug_log::*;
pub use events::*;
pub use forge::*;
pub use history::*;
pub use intruder::*;
pub use mock_forge::*;
pub use notes::*;
pub use port_scanner::*;
pub use settings::*;

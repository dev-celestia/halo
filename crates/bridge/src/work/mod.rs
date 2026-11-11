//! App-wide work tabs. Each tab is one tool and owns that tool's working data.
//!
//! Split by theme: tab kinds and per-tool data ([`kind`], [`tab`]), Coder
//! settings and workspace state (`coder_*`), Git snapshots ([`git_types`]),
//! and agent transcript types ([`agent_types`]).

mod agent_types;
mod coder_layout;
mod coder_panes;
mod coder_sessions;
mod coder_settings;
mod coder_work;
mod diagram_types;
mod git_types;
mod kind;
mod tab;

pub use agent_types::*;
pub use coder_layout::*;
pub use coder_settings::*;
pub use coder_work::*;
pub use diagram_types::*;
pub use git_types::*;
pub use kind::*;
pub use tab::*;

pub(crate) use agent_types::HaloAgentRunState;

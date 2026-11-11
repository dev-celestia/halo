//! Mock Forge view-state enums.

use serde::{Deserialize, Serialize};

/// Which canvas is visible in the Forge panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ForgeActiveView {
    #[default]
    Request,
    Response,
}

/// Sub-tab inside the request canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ForgeRequestTab {
    #[default]
    Headers,
    Body,
    Scripts,
}

/// Sub-tab inside the response canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ForgeResponseTab {
    #[default]
    Pretty,
    Raw,
    Headers,
    Request,
}

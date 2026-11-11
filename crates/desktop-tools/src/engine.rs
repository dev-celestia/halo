//! The one seam. Everything below this line is a window server; everything
//! above it is the contract.
//!
//! An engine is deliberately dumb: it dispatches and returns what the OS said.
//! It never decides whether a ref is current, how much output to keep, or
//! whether an app is allowed — [`crate::Desktop`] owns all of that, so every
//! engine inherits the same behaviour.

use serde_json::Value;

use crate::types::{ActRequest, EngineStatus, FindRequest, RawSnapshot, SnapshotRequest};
use crate::ToolResult;

pub trait Engine: Send {
    /// Permissions, version, surfaces. Never needs Accessibility.
    fn status(&mut self) -> ToolResult<EngineStatus>;

    /// Running GUI applications. Raw: the shape is the engine's, not ours.
    fn apps(&mut self, filter: Option<&str>) -> ToolResult<Value>;

    /// Visible windows, optionally narrowed to one app.
    fn windows(&mut self, app: Option<&str>) -> ToolResult<Value>;

    /// Accessibility tree. Raw: no rendering budget is applied here.
    fn snapshot_raw(&mut self, request: &SnapshotRequest) -> ToolResult<RawSnapshot>;

    fn find_raw(&mut self, request: &FindRequest) -> ToolResult<Value>;

    /// One property of one element: `text`, `value`, `title`, `bounds`,
    /// `role`, `states`.
    fn read_raw(&mut self, reference: &str, property: Option<&str>) -> ToolResult<Value>;

    /// One headless action. The engine must not receive `--headed`.
    fn perform(&mut self, request: &ActRequest) -> ToolResult<Value>;
}

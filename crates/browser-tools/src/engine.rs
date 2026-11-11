//! The one seam. Everything below this line is a browser; everything above it is
//! the contract.
//!
//! An engine is deliberately dumb: it dispatches and returns what the page said.
//! It never decides whether a ref is current, how much output to keep, or
//! whether an action worked — [`crate::Browser`] owns all of that so every engine
//! inherits the same behaviour.

use crate::ToolResult;

pub trait Engine: Send {
    /// Navigate and wait for the document to be usable.
    fn navigate(&mut self, url: &str) -> ToolResult<()>;

    /// Accessibility tree as text, refs included. Raw: no truncation here.
    fn snapshot_raw(&mut self, scope: Option<&str>, interactive: bool) -> ToolResult<String>;

    fn click(&mut self, reference: &str) -> ToolResult<()>;

    fn fill(&mut self, reference: &str, value: &str) -> ToolResult<()>;

    /// Current value of a field. `fill` cannot be verified from body text —
    /// input values are not part of it — so it is read back explicitly.
    fn value(&mut self, reference: &str) -> ToolResult<String>;

    /// Move focus without typing, so `press` targets the right field.
    fn focus(&mut self, reference: &str) -> ToolResult<()>;

    /// Global key press (Enter, Tab, Escape, Control+a).
    fn press(&mut self, key: &str) -> ToolResult<()>;

    /// Visible text. `None` means the whole body — used for the fingerprint.
    fn text(&mut self, selector: Option<&str>) -> ToolResult<String>;

    fn url(&mut self) -> ToolResult<String>;

    fn title(&mut self) -> ToolResult<String>;

    fn close(&mut self) -> ToolResult<()>;
}

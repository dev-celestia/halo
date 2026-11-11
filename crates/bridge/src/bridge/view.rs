//! Cross-page view state — overlays and modals that are not backend data.
//!
//! These flags live on [`BridgeState`] because every window renders from the
//! same store, and the pages are pure render functions that cannot hold state
//! of their own.

use super::*;

impl Bridge {
    /// Clear the in-app debugger log ring.
    pub fn clear_debug_logs(cx: &mut App) {
        Self::state(cx).update(cx, |s| s.debug_logs.clear());
    }

    /// Append a log entry for the debugger Logs tab.
    pub fn push_debug_log(
        level: DebugLogLevel,
        source: impl Into<String>,
        message: impl Into<String>,
        cx: &mut App,
    ) {
        let entry = DebugLogEntry::new(level, source, message);
        Self::state(cx).update(cx, |s| s.push_debug_log(entry));
    }
}

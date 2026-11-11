//! Split View — multi-pane workspace that hosts other tools side by side.

use super::*;
use crate::work::SplitLayoutKind;

impl Bridge {
    pub fn split_view_set_layout(layout: SplitLayoutKind, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.split_view.layout = layout;
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn split_view_set_slot(
        slot: usize,
        page_id: Option<&str>,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if slot >= 4 {
            return Err(BridgeError::message("Invalid split slot"));
        }
        Self::state(cx).update(cx, |s| {
            s.split_view.slots[slot] = page_id.map(str::to_string);
            s.commit_active_tab();
        });
        Ok(())
    }

    pub fn split_view_clear_slot(slot: usize, cx: &mut App) -> BridgeResult<()> {
        Self::split_view_set_slot(slot, None, cx)
    }

    pub fn split_view_reset(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.split_view.slots = [None, None, None, None];
            s.commit_active_tab();
        });
        Ok(())
    }
}

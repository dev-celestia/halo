//! Agent transcript row builders shared by the transcript view and subagent drawer.

//! Agent transcript — conversation shell plus per-message rendering.
//!
//! The empty state lives in the landing surface (`mod.rs`), so this module only
//! ever renders messages. While a run is live it also renders the one thing the
//! messages cannot say for themselves: that the agent is still working (see
//! [`live_status`]).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::composer::{attachment_label, attachment_preview_tooltip};
use super::transcript_state::{ChatKey, is_live_part, preview_end};
use celestia_ui::components::Tooltip;
use celestia_ui::components::TraceState;
use celestia_ui::components::composite::message::{
    Message, MessageAlignment, MessageContent, MessageFooter,
};
use celestia_ui::components::primitive::bubble::{Bubble, BubbleContent, BubbleVariant};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::components::primitive::shimmer::{ShimmerStyle, ShimmerText};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::{ScrollableMask, Scrollbar, ScrollbarAxis};
use gpui_component::text::{MarkdownExtensions, TextViewState};
use halo_bridge::{
    Bridge, BridgeState, HaloAgentContextUsage, HaloAgentPart, HaloAgentRole, HaloAgentSource,
};

use super::subagent::{SpawnCard, render_spawn_card};
use super::{BLOCK_GAP, CARD_TITLE_LINE, LINE_HEIGHT, META_LINE_HEIGHT};

/// Which transcript owns a part — the root chat or one delegated child.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PartScope {
    Root,
    Subagent(SharedString),
}

/// How long the copy control stays in the “Copied” state.
const COPY_FLASH_MS: u64 = 1400;
/// Cap on the work group's step list. Past this height the steps scroll on
/// their own, and the chat transcript keeps a separate scroll.
const WORK_LIST_MAX_H: f32 = 320.;
static COPY_FLASH_GEN: AtomicU64 = AtomicU64::new(0);

/// Ephemeral “which copy buttons are flashing” map — keyed by button id.
#[derive(Default)]
struct CopyFlash {
    active: HashMap<SharedString, u64>,
}

impl Global for CopyFlash {}

fn copy_is_flashing(id: &SharedString, cx: &App) -> bool {
    cx.try_global::<CopyFlash>()
        .is_some_and(|flash| flash.active.contains_key(id))
}

/// Monotonic epoch observed by the transcript view so a copy flash re-renders.
pub(super) fn copy_flash_epoch() -> u64 {
    COPY_FLASH_GEN.load(Ordering::SeqCst)
}

fn copy_flash(id: SharedString, cx: &mut App) {
    let token = COPY_FLASH_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    cx.default_global::<CopyFlash>()
        .active
        .insert(id.clone(), token);
    // Transcript observes Bridge — bump it so the checkmark paints.
    Bridge::state(cx).update(cx, |_| {});
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(COPY_FLASH_MS))
            .await;
        let _ = cx.update(|cx| {
            let clear = cx
                .try_global::<CopyFlash>()
                .is_some_and(|flash| flash.active.get(&id) == Some(&token));
            if clear {
                cx.global_mut::<CopyFlash>().active.remove(&id);
                Bridge::state(cx).update(cx, |_| {});
            }
        });
    })
    .detach();
}

mod cards;
mod messages;
mod reasoning;
mod rows;
mod sources;
mod status;
#[cfg(test)]
mod tests;
mod work;

pub(crate) use cards::*;
pub(crate) use messages::*;
pub(crate) use reasoning::*;
pub(crate) use rows::*;
pub(crate) use sources::*;
pub(crate) use status::*;
pub(crate) use work::*;

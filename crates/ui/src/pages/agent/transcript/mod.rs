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
/// Cap on in-flight copy flashes. Oldest ids drop so a burst, or a timer that
/// never lands, cannot grow the process-global map.
const COPY_FLASH_CAP: usize = 32;
/// Cap on the work group's step list. Past this height the steps scroll on
/// their own, and the chat transcript keeps a separate scroll.
const WORK_LIST_MAX_H: f32 = 320.;
static COPY_FLASH_GEN: AtomicU64 = AtomicU64::new(0);

/// Ephemeral “which copy buttons are flashing” map — keyed by button id.
///
/// Transcript entities register here so a flash repaints those views. The
/// shell snapshot stays put.
#[derive(Default)]
struct CopyFlash {
    active: HashMap<SharedString, u64>,
    transcripts: Vec<WeakEntity<super::AgentTranscript>>,
    subagents: Vec<WeakEntity<super::SubagentTranscript>>,
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

pub(super) fn register_transcript(view: WeakEntity<super::AgentTranscript>, cx: &mut App) {
    let flash = cx.default_global::<CopyFlash>();
    retain_live(&mut flash.transcripts);
    flash.transcripts.push(view);
}

pub(super) fn register_subagent(view: WeakEntity<super::SubagentTranscript>, cx: &mut App) {
    let flash = cx.default_global::<CopyFlash>();
    retain_live(&mut flash.subagents);
    flash.subagents.push(view);
}

fn retain_live<T: 'static>(views: &mut Vec<WeakEntity<T>>) {
    views.retain(|view| view.upgrade().is_some());
}

fn remember_copy(id: SharedString, token: u64, cx: &mut App) {
    let flash = cx.default_global::<CopyFlash>();
    flash.active.insert(id, token);
    while flash.active.len() > COPY_FLASH_CAP {
        let oldest = flash
            .active
            .iter()
            .min_by_key(|(_, token)| *token)
            .map(|(id, _)| id.clone());
        let Some(oldest) = oldest else {
            break;
        };
        flash.active.remove(&oldest);
    }
}

/// Repaint the views that were registered when this started, then keep any
/// registration that arrived during the call. Released windows are dropped.
fn repaint_copy_views(cx: &mut App) {
    let (transcripts, subagents) = {
        let flash = cx.global::<CopyFlash>();
        (flash.transcripts.clone(), flash.subagents.clone())
    };
    let live_transcripts = repaint_live(transcripts.clone(), cx, |view, cx| {
        view.update(cx, |this, cx| this.repaint_copy(cx))
            .is_ok()
    });
    let live_subagents = repaint_live(subagents.clone(), cx, |view, cx| {
        view.update(cx, |this, cx| this.repaint_copy(cx)).is_ok()
    });
    let flash = cx.global_mut::<CopyFlash>();
    let current_transcripts = std::mem::take(&mut flash.transcripts);
    let current_subagents = std::mem::take(&mut flash.subagents);
    flash.transcripts = merge_arrived(current_transcripts, &transcripts, live_transcripts);
    flash.subagents = merge_arrived(current_subagents, &subagents, live_subagents);
}

fn repaint_live<T: 'static>(
    views: Vec<WeakEntity<T>>,
    cx: &mut App,
    mut repaint: impl FnMut(&WeakEntity<T>, &mut App) -> bool,
) -> Vec<WeakEntity<T>> {
    views
        .into_iter()
        .filter(|view| repaint(view, cx))
        .collect()
}

fn merge_arrived<T: 'static>(
    current: Vec<WeakEntity<T>>,
    snapshot: &[WeakEntity<T>],
    mut kept: Vec<WeakEntity<T>>,
) -> Vec<WeakEntity<T>> {
    for view in current {
        if snapshot.iter().any(|old| old == &view) {
            continue;
        }
        if view.upgrade().is_some() && !kept.iter().any(|live| live == &view) {
            kept.push(view);
        }
    }
    kept
}

fn copy_flash(id: SharedString, cx: &mut App) {
    let token = COPY_FLASH_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    remember_copy(id.clone(), token, cx);
    repaint_copy_views(cx);
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
                repaint_copy_views(cx);
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

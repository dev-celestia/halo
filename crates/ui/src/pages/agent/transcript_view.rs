//! Persistent, per-window transcript. Celestia owns virtualization, anchors,
//! overscan, wheel dispatch and jump-to-latest; this view owns only row data.
use std::{collections::HashMap, sync::Arc, time::Duration};

use celestia_ui::components::TraceState;
use celestia_ui::components::composite::message_scroller::{MessageScroller, MessageScrollerState};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{
    Bridge, BridgeState, HaloAgentMessages, HaloAgentPart, HaloAgentRole, WorkData, WorkKind,
};

use super::{
    subagent::{SpawnCard, spawn_cards},
    text_cache::{PartKey, TextCache},
    transcript::{self, PartScope},
    transcript_state::{ChatKey, is_live_part, row_splice},
};

struct ChatList {
    scroller: Entity<MessageScrollerState>,
    // Cheap bridge snapshots share every unchanged row. Pointer identity detects
    // all corrections (including same-length replacements), without text hashes.
    messages: HaloAgentMessages,
    ids: Vec<String>,
    timer: Option<transcript::WorkTimer>,
    timer_expanded: bool,
    /// Scroll offset for the thought-process list, independent of the chat.
    steps_scroll: ScrollHandle,
    /// Assistant message whose tool rows fold under the work-group row.
    fold: Option<String>,
    active: Option<(String, usize)>,
    running: bool,
    /// Delegation cards joined to child sessions (rerender on status change).
    spawns: Vec<SpawnCard>,
    /// Tracks [`transcript::copy_flash_epoch`] so a Copy → Check flash re-renders
    /// without a message revision change.
    copy_epoch: u64,
    _scroll_subscription: Subscription,
}

struct Snapshot {
    key: ChatKey,
    messages: HaloAgentMessages,
    timer: Option<transcript::WorkTimer>,
    fold: Option<String>,
    active: Option<(String, usize)>,
    running: bool,
    spawns: Vec<SpawnCard>,
    copy_epoch: u64,
}

pub struct AgentTranscript {
    chats: HashMap<ChatKey, ChatList>,
    current: Option<ChatKey>,
    visible: bool,
    /// True while a 1s tick loop is refreshing the work timer.
    work_tick_active: bool,
    pub(super) cache: TextCache,
    _subscription: Subscription,
    pub rendered_rows: usize,
    pub remeasured_rows: usize,
}

impl AgentTranscript {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = Bridge::state(cx);
        let subscription = cx.observe(store.entity(), |this, _, cx| {
            let store = Bridge::state(cx);
            // Capture only transcript handles/metadata, NOT a BridgeState clone.
            let snapshot = this.capture(store.read(cx));
            if let Some(snapshot) = snapshot {
                this.sync(snapshot, cx);
            }
        });
        Self {
            chats: HashMap::new(),
            current: None,
            visible: false,
            work_tick_active: false,
            cache: TextCache::default(),
            _subscription: subscription,
            rendered_rows: 0,
            remeasured_rows: 0,
        }
    }

    fn ensure_work_tick(&mut self, running: bool, cx: &mut Context<Self>) {
        if !running || self.work_tick_active {
            return;
        }
        self.work_tick_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        if !this.visible {
                            this.work_tick_active = false;
                            return false;
                        }
                        let running = this
                            .current
                            .as_ref()
                            .and_then(|key| this.chats.get(key))
                            .is_some_and(|chat| chat.running);
                        // Refresh the timer label even while idle so the final
                        // second sticks; stop the loop once the run settles.
                        let store = Bridge::state(cx);
                        Bridge::halo_agent_poll_desktop_lease(cx);
                        if let Some(snapshot) = this.capture(store.read(cx)) {
                            this.sync(snapshot, cx);
                        } else {
                            cx.notify();
                        }
                        if !running {
                            this.work_tick_active = false;
                        }
                        running
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
        })
        .detach();
    }

    /// Called by the workspace at mount; never changes composer/focus/IME state.
    ///
    /// Reads the live store instead of the caller's snapshot: `HaloApp` keeps a
    /// frozen `BridgeState` while nothing but the transcript revision changed
    /// ([`BridgeState::suppresses_shell_refresh`]), so syncing from that copy
    /// rolled every disclosure flag back to its pre-click value on the next
    /// shell repaint — a thought/tool toggle looked dead until a session switch
    /// (a non-suppressed change) refreshed the snapshot.
    pub fn mount(&mut self, cx: &mut Context<Self>) {
        self.visible = true;
        let store = Bridge::state(cx);
        let snapshot = {
            let state = store.read(cx);
            self.capture(state)
        };
        if let Some(snapshot) = snapshot {
            self.sync(snapshot, cx);
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    fn key(state: &BridgeState) -> ChatKey {
        ChatKey {
            workspace: state.active_work_tab_id.clone().unwrap_or_default(),
            chat: state.coder.agent_active_session_id.clone(),
            subagent: None,
        }
    }

    fn capture(&mut self, state: &BridgeState) -> Option<Snapshot> {
        let exists = |key: &ChatKey| {
            let coder = if state.active_work_tab_id.as_deref().unwrap_or_default() == key.workspace
            {
                Some(&state.coder)
            } else {
                state
                    .work_tabs
                    .iter()
                    .find(|tab| tab.id == key.workspace)
                    .and_then(|tab| match &tab.data {
                        WorkData::Coder(coder) => Some(coder),
                        _ => None,
                    })
            };
            coder.is_some_and(|coder| {
                coder.agent_active_session_id == key.chat
                    || coder
                        .agent_sessions
                        .iter()
                        .any(|session| session.id == key.chat)
            })
        };
        self.chats.retain(|key, _| exists(key));
        self.cache.entries.retain(|key, _| exists(&key.chat));
        if !self.visible
            || state
                .active_work_kind()
                .is_some_and(|kind| kind != WorkKind::Agent)
        {
            return None;
        }
        let expanded = self
            .chats
            .get(&Self::key(state))
            .is_some_and(|chat| chat.timer_expanded);
        let timer = transcript::work_timer(state, expanded);
        // The last assistant turn hosts the work group: its process rows fold
        // into the group above the answer, the way the reference lays a run
        // out. Older turns keep their inline chips.
        let fold = timer.as_ref().and_then(|_| {
            let last = state.coder.agent_messages.iter().last()?;
            (last.role == HaloAgentRole::Assistant).then(|| last.id.clone())
        });
        Some(Snapshot {
            key: Self::key(state),
            messages: state.coder.agent_messages.clone(),
            timer,
            fold,
            active: state.coder.agent_streaming_part.clone(),
            running: state.coder.agent_running,
            spawns: spawn_cards(
                &state.coder.agent_subagents,
                state.coder.agent_open_subagent.as_deref(),
            ),
            copy_epoch: transcript::copy_flash_epoch(),
        })
    }

    fn sync(&mut self, next: Snapshot, cx: &mut Context<Self>) {
        let Snapshot {
            key,
            messages,
            timer,
            fold,
            active,
            running,
            spawns,
            copy_epoch,
        } = next;
        let switched = self.current.as_ref() != Some(&key);
        self.current = Some(key.clone());
        let chat = self.chats.entry(key.clone()).or_insert_with(|| {
            let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
            let watched_key = key.clone();
            let subscription = cx.observe(&scroller, move |this, _, cx| {
                if this.visible && this.current.as_ref() == Some(&watched_key) {
                    cx.notify();
                }
            });
            ChatList {
                scroller,
                messages: Default::default(),
                ids: Vec::new(),
                timer: None,
                timer_expanded: false,
                steps_scroll: ScrollHandle::new(),
                fold: None,
                active: None,
                running: false,
                spawns: Vec::new(),
                copy_epoch: 0,
                _scroll_subscription: subscription,
            }
        });
        let copy_flashed = copy_epoch != chat.copy_epoch;
        chat.copy_epoch = copy_epoch;
        // Belt and braces for the same hazard: a caller holding an older copy of
        // this chat's rows must never overwrite what the live observer already
        // installed, or expand/tool flags snap shut behind the user's click.
        if !switched && !messages.is_empty() && messages.revision() < chat.messages.revision() {
            return;
        }
        // The group opens itself while a run is live and folds back to its
        // settled summary the moment the run ends. A manual toggle wins until
        // the next run transition.
        if chat.running != running {
            chat.timer_expanded = running;
        }
        let mut timer = timer;
        if let Some(timer) = timer.as_mut() {
            timer.expanded = chat.timer_expanded;
        }
        let revision_changed = messages.revision() != chat.messages.revision();
        let mut changed = Vec::new();
        let mut structural = messages.len() != chat.messages.len();
        if revision_changed {
            // A notification may combine several backend batches, or a hidden
            // window may skip many revisions. Compare shared row pointers rather
            // than trusting only the bridge's latest changed_rows range.
            for (index, (old, new)) in chat.messages.iter().zip(messages.iter()).enumerate() {
                if !Arc::ptr_eq(old, new) {
                    changed.push(index);
                    structural |= old.id != new.id;
                }
            }
            changed.extend(chat.messages.len()..messages.len());
            // In-place Arc::make_mut (refcount 1) bumps revision but keeps the
            // same row pointer — fall back so expand/collapse still invalidates.
            if changed.is_empty() {
                if let Some(range) = messages.changed_rows() {
                    changed.extend(range);
                } else {
                    changed.extend(0..messages.len());
                }
            }
        }
        let ids = if structural {
            messages.iter().map(|m| m.id.clone()).collect()
        } else {
            Vec::new()
        };
        let splice = if structural {
            row_splice(&chat.ids, &ids)
        } else {
            None
        };
        let next_ids = if structural {
            ids.as_slice()
        } else {
            chat.ids.as_slice()
        };
        if active != chat.active || running != chat.running {
            for (id, _) in active.iter().chain(chat.active.iter()) {
                if let Some(index) = next_ids.iter().position(|candidate| candidate == id) {
                    changed.push(index);
                }
            }
            if running != chat.running {
                // Retry controls are rare; only their rows need invalidation.
                changed.extend(messages.iter().enumerate().filter_map(|(i, message)| {
                    message
                        .parts
                        .iter()
                        .any(|p| matches!(p, HaloAgentPart::Interrupted))
                        .then_some(i)
                }));
            }
        }
        let fold_changed = chat.fold != fold;
        if fold_changed {
            // Folding moves tool rows between the message body and the group
            // row, so the affected message row's measured height changes.
            for id in fold.iter().chain(chat.fold.iter()) {
                let index = next_ids
                    .iter()
                    .position(|candidate| candidate == id)
                    .or_else(|| chat.ids.iter().position(|candidate| candidate == id));
                changed.extend(index);
            }
        }
        let timer_changed = chat.timer != timer;
        let spawns_changed = chat.spawns != spawns;
        let dirty = switched
            || splice.is_some()
            || !changed.is_empty()
            || timer_changed
            || fold_changed
            || spawns_changed
            || copy_flashed;
        // Scroller splice math needs the pre-update list shape. Only a turn
        // without a host message (a fresh user prompt) trails its group.
        let trailing_before = chat.timer.is_some() && chat.fold.is_none();
        let old_ids_len = chat.ids.len();
        let message_count = messages.len();
        let trailing_after = timer.is_some() && fold.is_none();
        changed.sort_unstable();
        changed.dedup();
        let remeasure: Vec<usize> = changed
            .iter()
            .copied()
            .filter(|i| *i < message_count)
            .collect();

        // Install the snapshot before remeasure/notify so a re-entrant paint
        // (scroller observe → cx.notify) reads the toggled expanded flags.
        if structural {
            chat.ids = ids;
        }
        chat.messages = messages;
        chat.timer = timer;
        chat.fold = fold;
        chat.active = active;
        chat.running = running;
        chat.spawns = spawns;

        if revision_changed {
            // Drop deleted parts even on same-row-count updates. For changed
            // cached text, replace its revision in place now; the existing
            // parser coalesces updates off-thread and preserves an unchanged
            // prefix/selection.
            let ids = &chat.ids;
            let messages = &chat.messages;
            self.cache.entries.retain(|part, entry| {
                if part.chat != key {
                    return true;
                }
                let Some(index) = ids.iter().position(|id| id == &part.message) else {
                    return false;
                };
                let Some(HaloAgentPart::Text(source)) = messages[index].parts.get(part.part) else {
                    return false;
                };
                if entry.source.as_ref() != source.as_str() {
                    entry.state.update(cx, |text, cx| text.set_text(source, cx));
                    entry.source = source.clone().into();
                    entry.cost = super::transcript_state::text_cost(source.len());
                    self.cache.updated += 1;
                }
                true
            });
        }

        if dirty {
            chat.scroller.update(cx, |scroller, cx| {
                if splice.is_some() && trailing_before {
                    scroller.splice(old_ids_len..old_ids_len + 1, 0, cx);
                }
                if let Some(splice) = &splice {
                    if splice.old.start == old_ids_len && splice.old.is_empty() {
                        scroller.append(splice.inserted, cx);
                    } else {
                        scroller.splice(splice.old.clone(), splice.inserted, cx);
                    }
                    if trailing_after {
                        scroller.append(1, cx);
                    }
                } else if trailing_before != trailing_after {
                    if trailing_after {
                        scroller.append(1, cx);
                    } else {
                        scroller.splice(message_count..message_count + 1, 0, cx);
                    }
                }
                for index in remeasure {
                    scroller.remeasure_items(index..index + 1, cx);
                    self.remeasured_rows += 1;
                }
                if timer_changed {
                    if trailing_after {
                        scroller.remeasure_items(message_count..message_count + 1, cx);
                    }
                    if let Some(id) = chat.fold.as_deref() {
                        if let Some(index) = chat.ids.iter().position(|candidate| candidate == id) {
                            scroller.remeasure_items(index..index + 1, cx);
                            self.remeasured_rows += 1;
                        }
                    }
                }
            });
            cx.notify();
        }

        let pinned = chat.active.as_ref().map(|(message, part)| PartKey {
            chat: key,
            message: message.clone(),
            part: *part,
        });
        self.cache.trim(pinned.as_ref(), cx);
        self.ensure_work_tick(running, cx);
    }

    pub fn test_scroller(&self) -> Entity<MessageScrollerState> {
        self.chats[self.current.as_ref().unwrap()].scroller.clone()
    }

    pub fn test_chat_count(&self) -> usize {
        self.chats.len()
    }

    pub fn active_part(&self) -> Option<(String, usize)> {
        self.chats.get(self.current.as_ref()?)?.active.clone()
    }

    /// Test helper: `expanded` for a Reasoning or ToolCall part in the synced chat.
    pub fn test_part_expanded(&self, message_id: &str, part_index: usize) -> Option<bool> {
        let chat = self.chats.get(self.current.as_ref()?)?;
        let message = chat.messages.iter().find(|m| m.id == message_id)?;
        match message.parts.get(part_index)? {
            HaloAgentPart::Reasoning { expanded, .. }
            | HaloAgentPart::ToolCall { expanded, .. } => Some(*expanded),
            _ => None,
        }
    }

    /// Test helper: element id the virtual row assigns to a message part.
    pub fn test_part_id(&self, message_id: &str, part_index: usize) -> Option<String> {
        let key = self.current.as_ref()?;
        Some(key.part_id(message_id, part_index))
    }

    /// Test helper: message id whose tool rows fold under the work-group row.
    pub fn test_fold_id(&self) -> Option<String> {
        self.chats.get(self.current.as_ref()?)?.fold.clone()
    }

    pub fn cache_stats(&self) -> (usize, usize, usize, usize) {
        (
            self.cache.entries.len(),
            self.cache.created,
            self.cache.updated,
            self.cache.evicted,
        )
    }

    /// Fill the text cache past its budget and evict everything except `pin`.
    pub fn warm_text_cache(&mut self, count: usize, pin: bool, cx: &mut Context<Self>) -> bool {
        let chat = super::transcript_state::ChatKey {
            workspace: "w".into(),
            chat: "c".into(),
            subagent: None,
        };
        let mut pinned = None;
        for index in 0..count {
            let key = super::text_cache::PartKey {
                chat: chat.clone(),
                message: format!("m{index}"),
                part: 0,
            };
            if pin && index == 0 {
                pinned = Some(key.clone());
            }
            let _ = self.cache.get(key, &"x".repeat(8192), cx);
        }
        self.cache.trim(pinned.as_ref(), cx);
        pinned
            .as_ref()
            .is_none_or(|key| self.cache.entries.contains_key(key))
            && self.cache.entries.len() <= super::transcript_state::CACHE_ENTRIES
            && self.cache.bytes()
                <= super::transcript_state::CACHE_BYTES.saturating_add(8192 * 4 + 4096)
    }

    /// Select-all covers the original source, and eviction keeps that entry.
    pub fn selection_covers_and_survives(
        &mut self,
        source: &str,
        cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        let chat = super::transcript_state::ChatKey {
            workspace: "w".into(),
            chat: "selected".into(),
            subagent: None,
        };
        let key = super::text_cache::PartKey {
            chat: chat.clone(),
            message: "selected".into(),
            part: 0,
        };
        let (state, _, _) = self.cache.get(key.clone(), source, cx);
        state.update(cx, |text, cx| text.select_all(cx));
        let range = state.read(cx).selected_source_range();
        for index in 0..200 {
            let other = super::text_cache::PartKey {
                chat: chat.clone(),
                message: format!("other-{index}"),
                part: 0,
            };
            let _ = self.cache.get(other, &"y".repeat(8192), cx);
        }
        self.cache.trim(None, cx);
        range.filter(|_| self.cache.entries.contains_key(&key))
    }

    fn invalidate(&mut self, key: &PartKey, cx: &mut Context<Self>) {
        if !self.visible || self.current.as_ref() != Some(&key.chat) {
            return;
        }
        if let Some(chat) = self.chats.get(&key.chat) {
            if let Some(index) = chat.ids.iter().position(|id| id == &key.message) {
                chat.scroller.update(cx, |state, cx| {
                    state.remeasure_items(index..index + 1, cx);
                });
                cx.notify();
            }
        }
    }

    fn render_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(key) = self.current.clone() else {
            return Empty.into_any_element();
        };
        let chat = &self.chats[&key];
        let steps_scroll = chat.steps_scroll.clone();
        self.rendered_rows += 1;
        let Some(message) = chat.messages.get(index).cloned() else {
            // The trailing row only carries the group while no message hosts
            // it (a freshly submitted prompt, before the reply row exists).
            let active = chat.active.clone();
            let Some(timer) = chat.timer.clone().filter(|_| chat.fold.is_none()) else {
                return Empty.into_any_element();
            };
            return work_group(timer, None, &key, active.as_ref(), &steps_scroll, cx);
        };
        let active = chat.active.clone();
        let running = chat.running;
        // Explicit entities are created ONLY inside this virtual row callback.
        let mut texts = HashMap::new();
        for (part, body) in message.parts.iter().enumerate() {
            if let HaloAgentPart::Text(source) = body {
                if message.role != halo_bridge::HaloAgentRole::Assistant {
                    continue;
                }
                let part_key = PartKey {
                    chat: key.clone(),
                    message: message.id.clone(),
                    part,
                };
                let (text, extensions, created) = self.cache.get(part_key.clone(), source, cx);
                if created {
                    let watched_key = part_key.clone();
                    let subscription = cx.observe(&text, move |this, _, cx| {
                        // Parse completion may change a measured row's height.
                        // Observers run after layout releases the list borrow.
                        this.invalidate(&watched_key, cx);
                    });
                    self.cache.entries.get_mut(&part_key).unwrap()._subscription =
                        Some(subscription);
                }
                texts.insert(part, (text, extensions));
            }
        }
        let live_part = active.as_ref().map(|(id, part)| (id.as_str(), *part));
        let fold_process = chat.fold.as_deref() == Some(message.id.as_str());
        let host_timer = fold_process.then(|| chat.timer.clone()).flatten();
        let element = transcript::render_message(
            &message,
            &key,
            &PartScope::Root,
            &chat.spawns,
            running,
            live_part,
            fold_process,
            &texts,
            window,
            cx,
        )
        .into_any_element();
        // The host turn paints its process group above the answer, so the run
        // reads top-down: what the agent did, then what it said.
        let element = if let Some(timer) = host_timer {
            let group = work_group(
                timer,
                Some(&message),
                &key,
                active.as_ref(),
                &steps_scroll,
                cx,
            );
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(group)
                .child(element)
                .into_any_element()
        } else {
            element
        };
        let pinned = active.map(|(message, part)| PartKey {
            chat: key,
            message,
            part,
        });
        self.cache.trim(pinned.as_ref(), cx);
        element
    }
}

/// The host turn's process rows in part order — thoughts and tool calls read
/// as one run under the group header.
fn work_rows(
    host: &halo_bridge::HaloAgentMessage,
    key: &ChatKey,
    active: Option<&(String, usize)>,
    cx: &mut Context<AgentTranscript>,
) -> Vec<AnyElement> {
    let mut rows = Vec::new();
    for (part_index, part) in host.parts.iter().enumerate() {
        let part_id: SharedString = key.part_id(&host.id, part_index).into();
        let live = is_live_part(active.map(|(m, p)| (m.as_str(), *p)), &host.id, part_index);
        let mid = host.id.clone();
        let slot = part_index;
        let row = match part {
            HaloAgentPart::ToolCall {
                id,
                label,
                detail,
                expanded,
                is_error,
                done,
                sources,
            } => {
                let (state, _) =
                    transcript::tool_trace_state(detail.as_deref(), *is_error, *done, live);
                let tid = id.clone();
                transcript::process_row(
                    part_id,
                    state,
                    label.clone(),
                    detail.clone(),
                    sources.as_slice(),
                    true,
                    true,
                    *expanded,
                    move |_, _, cx| {
                        let _ = Bridge::halo_agent_toggle_tool_chip(&mid, &tid, cx);
                    },
                    cx,
                )
            }
            HaloAgentPart::Reasoning { text, expanded } => {
                let state = if live {
                    TraceState::Running
                } else {
                    TraceState::Done
                };
                transcript::process_row(
                    part_id,
                    state,
                    "Thought".into(),
                    Some(text.clone()),
                    &[],
                    false,
                    true,
                    *expanded,
                    move |_, _, cx| {
                        let _ = Bridge::halo_agent_toggle_reasoning(&mid, part_index, cx);
                    },
                    cx,
                )
            }
            _ => continue,
        };
        // Static per-slot id so integration tests can query the revealed rows
        // through `debug_bounds`.
        rows.push(
            div()
                .id(SharedString::from(format!(
                    "coder-agent-work-row-{part_index}"
                )))
                .debug_selector(move || format!("coder-agent-work-row-{slot}"))
                .w_full()
                .min_w_0()
                .child(row)
                .into_any_element(),
        );
    }
    rows
}

/// The turn's work group — header, folded process rows, live activity line.
fn work_group(
    timer: transcript::WorkTimer,
    host: Option<&halo_bridge::HaloAgentMessage>,
    key: &ChatKey,
    active: Option<&(String, usize)>,
    steps_scroll: &ScrollHandle,
    cx: &mut Context<AgentTranscript>,
) -> AnyElement {
    let rows = host
        .map(|host| work_rows(host, key, active, cx))
        .unwrap_or_default();
    let force_scroll = host.is_some_and(|message| {
        message.parts.iter().any(|part| match part {
            HaloAgentPart::ToolCall { expanded, .. }
            | HaloAgentPart::Reasoning { expanded, .. } => *expanded,
            _ => false,
        })
    });
    let chat_key = key.clone();
    let this = cx.entity().downgrade();
    transcript::render_work_group(
        timer,
        rows,
        steps_scroll,
        force_scroll,
        move |_, _, cx| {
            let Some(entity) = this.upgrade() else {
                return;
            };
            entity.update(cx, |this, cx| {
                if let Some(chat) = this.chats.get_mut(&chat_key) {
                    chat.timer_expanded = !chat.timer_expanded;
                }
                let store = Bridge::state(cx);
                if let Some(snapshot) = this.capture(store.read(cx)) {
                    this.sync(snapshot, cx);
                }
            });
        },
        cx,
    )
    .into_any_element()
}

impl Render for AgentTranscript {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(chat) = self.current.as_ref().and_then(|key| self.chats.get(key)) else {
            return Empty.into_any_element();
        };
        let view = cx.entity().downgrade();
        let mut row_style = StyleRefinement::default();
        // Inline gutters match COLUMN_MAX breathing room; 20px between messages
        // (≥ 2× BLOCK_GAP) so turns group without a hard divider.
        row_style.padding.left = Some(px(12.).into());
        row_style.padding.right = Some(px(12.).into());
        row_style.padding.bottom = Some(px(20.).into());
        let mut list_style = StyleRefinement::default();
        list_style.padding.top = Some(px(12.).into());
        list_style.padding.bottom = Some(px(20.).into());
        let surface = cx.theme().background;
        MessageScroller::new(
            "coder-agent-scroller",
            chat.scroller.clone(),
            move |index, window, cx| {
                view.update(cx, |this, cx| this.render_row(index, window, cx))
                    .unwrap_or_else(|_| Empty.into_any_element())
            },
        )
        .with_row_style(row_style)
        .with_list_style(list_style)
        .with_top_fade(surface)
        .with_bottom_fade(surface)
        .into_any_element()
    }
}

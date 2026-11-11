//! Right-side drawer: the live transcript of one delegated child session.
//!
//! The parent transcript stays mounted and never receives child rows; this
//! entity owns the child's own scroll, selection and text cache. Children are
//! keyed by `ChatKey::child`, so root and child part ids never collide.

use std::{collections::HashMap, sync::Arc};

use celestia_ui::components::composite::message_scroller::{MessageScroller, MessageScrollerState};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, HaloAgentMessages, HaloAgentPart, HaloAgentSubagentStatus};

use crate::pages::editors::ToolEditors;

use super::{
    CARD_TITLE_LINE, META_LINE_HEIGHT,
    subagent::status_label,
    text_cache::{PartKey, TextCache},
    transcript::{self, PartScope},
    transcript_state::{ChatKey, row_splice},
};

/// Width of the drawer when open.
const DRAWER_WIDTH: Pixels = px(320.);

struct DrawerChat {
    key: ChatKey,
    scroller: Entity<MessageScrollerState>,
    messages: HaloAgentMessages,
    ids: Vec<String>,
    active: Option<(String, usize)>,
    running: bool,
    title: String,
    agent: Option<String>,
    status: HaloAgentSubagentStatus,
    _scroll_subscription: Subscription,
}

/// Everything one sync needs from the open child.
struct DrawerSnapshot {
    key: ChatKey,
    messages: HaloAgentMessages,
    active: Option<(String, usize)>,
    running: bool,
    title: String,
    agent: Option<String>,
    status: HaloAgentSubagentStatus,
}

/// Window-local drawer transcript. Exists once per window; swaps its chat when
/// the bridge changes [`BridgeState::coder.agent_open_subagent`].
pub struct SubagentTranscript {
    chat: Option<DrawerChat>,
    cache: TextCache,
    pub rendered_rows: usize,
    pub remeasured_rows: usize,
    _subscription: Subscription,
}

impl SubagentTranscript {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = Bridge::state(cx);
        let subscription = cx.observe(store.entity(), |this, _, cx| {
            let store = Bridge::state(cx);
            if let Some(next) = Self::capture(store.read(cx)) {
                this.sync(next, cx);
            }
        });
        Self {
            chat: None,
            cache: TextCache::default(),
            rendered_rows: 0,
            remeasured_rows: 0,
            _subscription: subscription,
        }
    }

    /// Copy only the open child's handles/metadata (the message list is COW).
    fn capture(state: &BridgeState) -> Option<DrawerSnapshot> {
        let session = state.coder.agent_open_subagent.as_deref()?;
        let child = state
            .coder
            .agent_subagents
            .iter()
            .find(|child| child.session_id == session)?;
        Some(DrawerSnapshot {
            key: ChatKey::child(
                state.active_work_tab_id.clone().unwrap_or_default(),
                state.coder.agent_active_session_id.clone(),
                child.session_id.clone(),
            ),
            messages: child.messages.clone(),
            active: child.streaming_part.clone(),
            running: child.is_running(),
            title: child.label().to_string(),
            agent: child.agent.clone(),
            status: child.status,
        })
    }

    /// Sync the drawer with the open child. Never touches root transcript state.
    ///
    /// Reads the live store, not the caller's snapshot — the shell freezes its
    /// copy while only transcript revisions changed, so a frozen snapshot would
    /// undo a disclosure the user just toggled.
    pub fn mount(&mut self, cx: &mut Context<Self>) {
        let store = Bridge::state(cx);
        let next = {
            let state = store.read(cx);
            Self::capture(state)
        };
        if let Some(next) = next {
            self.sync(next, cx);
        }
    }

    fn sync(&mut self, next: DrawerSnapshot, cx: &mut Context<Self>) {
        let DrawerSnapshot {
            key,
            messages,
            active,
            running,
            title,
            agent,
            status,
        } = next;
        let switched = self.chat.as_ref().map(|chat| &chat.key) != Some(&key);
        if switched {
            // Only the visible child keeps cached text; stale children would
            // otherwise hold parsed markdown for the whole window lifetime.
            self.cache.entries.retain(|part, _| part.chat == key);
            let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
            let watched = key.clone();
            let subscription = cx.observe(&scroller, move |this, _, cx| {
                if this.chat.as_ref().is_some_and(|chat| chat.key == watched) {
                    cx.notify();
                }
            });
            let chat = DrawerChat {
                key: key.clone(),
                scroller,
                messages: Default::default(),
                ids: Vec::new(),
                active: None,
                running: false,
                title: String::new(),
                agent: None,
                status: HaloAgentSubagentStatus::Running,
                _scroll_subscription: subscription,
            };
            self.chat = Some(chat);
        }
        let Some(chat) = self.chat.as_mut() else {
            return;
        };

        // Same guard as the root transcript: never let an older copy of this
        // child's rows overwrite the live observer's state.
        if !switched && !messages.is_empty() && messages.revision() < chat.messages.revision() {
            return;
        }

        let meta_changed = chat.title != title || chat.agent != agent || chat.status != status;
        chat.title = title;
        chat.agent = agent;
        chat.status = status;

        let revision_changed = messages.revision() != chat.messages.revision();
        let mut changed = Vec::new();
        let mut structural = messages.len() != chat.messages.len();
        if revision_changed {
            for (index, (old, new)) in chat.messages.iter().zip(messages.iter()).enumerate() {
                if !Arc::ptr_eq(old, new) {
                    changed.push(index);
                    structural |= old.id != new.id;
                }
            }
            changed.extend(chat.messages.len()..messages.len());
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
        }
        let dirty = switched || splice.is_some() || !changed.is_empty() || meta_changed;
        let old_ids_len = chat.ids.len();
        let message_count = messages.len();
        changed.sort_unstable();
        changed.dedup();
        let remeasure: Vec<usize> = changed
            .into_iter()
            .filter(|index| *index < message_count)
            .collect();

        if structural {
            chat.ids = ids;
        }
        chat.messages = messages;
        chat.active = active;
        chat.running = running;

        if revision_changed {
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
                if let Some(splice) = &splice {
                    if splice.old.start == old_ids_len && splice.old.is_empty() {
                        scroller.append(splice.inserted, cx);
                    } else {
                        scroller.splice(splice.old.clone(), splice.inserted, cx);
                    }
                }
                for index in remeasure {
                    scroller.remeasure_items(index..index + 1, cx);
                    self.remeasured_rows += 1;
                }
            });
            cx.notify();
        }

        let pinned = chat.active.as_ref().map(|(message, part)| PartKey {
            chat: key,
            message: message.clone(),
            part: *part,
        });
        // Cache is drawer-local, so this trims only child entries.
        self.cache.trim(pinned.as_ref(), cx);
    }

    fn invalidate(&mut self, key: &PartKey, cx: &mut Context<Self>) {
        let Some(chat) = self.chat.as_ref() else {
            return;
        };
        let Some(index) = chat.ids.iter().position(|id| id == &key.message) else {
            return;
        };
        chat.scroller.update(cx, |state, cx| {
            state.remeasure_items(index..index + 1, cx);
        });
        cx.notify();
    }

    fn render_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(key) = self.chat.as_ref().map(|chat| chat.key.clone()) else {
            return Empty.into_any_element();
        };
        let chat = &self.chat.as_ref().unwrap();
        self.rendered_rows += 1;
        let Some(message) = chat.messages.get(index).cloned() else {
            return Empty.into_any_element();
        };
        let active = chat.active.clone();
        let running = chat.running;
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
                        this.invalidate(&watched_key, cx);
                    });
                    self.cache.entries.get_mut(&part_key).unwrap()._subscription =
                        Some(subscription);
                }
                texts.insert(part, (text, extensions));
            }
        }
        let live_part = active.as_ref().map(|(id, part)| (id.as_str(), *part));
        let scope = key
            .subagent
            .clone()
            .map(|session| PartScope::Subagent(session.into()))
            .unwrap_or(PartScope::Root);
        let element = transcript::render_message(
            &message,
            &key,
            &scope,
            &[],
            running,
            live_part,
            false,
            &texts,
            window,
            cx,
        )
        .into_any_element();
        let pinned = active.map(|(message, part)| PartKey {
            chat: key,
            message,
            part,
        });
        self.cache.trim(pinned.as_ref(), cx);
        element
    }
}

impl Render for SubagentTranscript {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(chat) = self.chat.as_ref() else {
            return Empty.into_any_element();
        };
        let view = cx.entity().downgrade();
        let mut row_style = StyleRefinement::default();
        row_style.padding.left = Some(px(10.).into());
        row_style.padding.right = Some(px(10.).into());
        row_style.padding.bottom = Some(px(16.).into());
        let mut list_style = StyleRefinement::default();
        list_style.padding.top = Some(px(10.).into());
        list_style.padding.bottom = Some(px(12.).into());
        let id = SharedString::from(format!(
            "coder-agent-subagent-scroller-{}",
            chat.key.subagent.as_deref().unwrap_or_default()
        ));
        let surface = cx.theme().background;
        MessageScroller::new(id, chat.scroller.clone(), move |index, window, cx| {
            view.update(cx, |this, cx| this.render_row(index, window, cx))
                .unwrap_or_else(|_| Empty.into_any_element())
        })
        .with_row_style(row_style)
        .with_list_style(list_style)
        .with_top_fade(surface)
        .with_bottom_fade(surface)
        .into_any_element()
    }
}

/// The drawer panel itself — `Empty` unless a child is open and still known.
///
/// Replaces the Artifacts rail slot while open: 320px + 260px + a 760px column
/// does not fit most windows, and the child transcript is the focused surface.
pub(super) fn render_subagent_drawer(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &mut App,
) -> AnyElement {
    let Some(session) = state.coder.agent_open_subagent.clone() else {
        return Empty.into_any_element();
    };
    let Some(child) = state
        .coder
        .agent_subagents
        .iter()
        .find(|child| child.session_id == session)
    else {
        return Empty.into_any_element();
    };
    editors
        .halo_agent_subagent
        .update(cx, |view, cx| view.mount(cx));

    let theme = cx.theme();
    let border = theme.border;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let waiting = child.pending_input.is_some();
    let chip_color = if waiting {
        theme.warning
    } else {
        match child.status {
            HaloAgentSubagentStatus::Running => theme.warning,
            HaloAgentSubagentStatus::Completed => theme.success,
            HaloAgentSubagentStatus::Interrupted => muted,
            HaloAgentSubagentStatus::Errored => theme.danger,
        }
    };
    let chip_bg = chip_color.opacity(0.14);
    let status = if waiting {
        "APPROVAL"
    } else {
        status_label(child.status)
    };
    let meta = match child
        .agent
        .as_deref()
        .filter(|agent| !agent.trim().is_empty())
    {
        Some(agent) => format!("{agent} · {}", child.messages.len()),
        None => format!("{} events", child.messages.len()),
    };

    celestia_ui::motion::fade_in(
        "coder-agent-subagent-drawer-in",
        div()
            .id("coder-agent-subagent-drawer")
            .h_full()
            .w(DRAWER_WIDTH)
            .flex_none()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(border)
            .bg(theme.background)
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .flex_none()
                    .px(px(10.))
                    .py(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                Phosphor::new(PhosphorIcon::GitBranch)
                                    .size(px(12.))
                                    .color(muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .line_height(CARD_TITLE_LINE)
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(foreground)
                                    .child(child.label().to_string()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .px(px(6.))
                                    .py(px(2.))
                                    .rounded(px(6.))
                                    .bg(chip_bg)
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(chip_color)
                                    .child(status),
                            )
                            .child(
                                Button::icon("coder-agent-subagent-close", PhosphorIcon::X)
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::IconSm)
                                    .accessibility_label("Close subagent")
                                    .tooltip("Close subagent")
                                    .on_click(|_, _, cx| {
                                        let _ = Bridge::halo_agent_close_subagent(cx);
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .text_color(muted)
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(editors.halo_agent_subagent.clone()),
            ),
    )
    .into_any_element()
}

//! Bounded, wakeable backend ingress. Adjacent token deltas coalesce; nothing
//! else is dropped. Producers wait for space without holding the UI lock.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use tokio::sync::Notify;

use super::AgentEvent;
use crate::types::BackendEvent;

/// Keep batches small so apply + transcript remasure cannot monopolize a frame.
pub(super) const DRAIN_EVENTS: usize = 16;
pub(super) const DRAIN_BUDGET: Duration = Duration::from_millis(2);
const MAX_EVENTS: usize = 256;
const MAX_BYTES: usize = 2 * 1024 * 1024;
const COALESCE_LIMIT: usize = 256 * 1024;

struct Inner {
    queue: VecDeque<BackendEvent>,
    bytes: usize,
    closed: bool,
}

struct Shared {
    inner: Mutex<Inner>,
    space: Condvar,
    wake: Notify,
}

#[derive(Clone)]
pub(super) struct Ingress(Arc<Shared>);

impl Ingress {
    pub(super) fn new() -> Self {
        Self(Arc::new(Shared {
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                bytes: 0,
                closed: false,
            }),
            space: Condvar::new(),
            wake: Notify::new(),
        }))
    }

    /// Block until the queue accepts `event`. Coalescing does not count as a drop.
    pub(super) fn send(&self, event: BackendEvent) -> Result<(), ()> {
        let mut inner = self.0.inner.lock();
        if coalesce(&mut inner, &event) {
            drop(inner);
            self.0.wake.notify_one();
            return Ok(());
        }
        let cost = event_cost(&event);
        while !inner.closed && !fits(&inner, cost) {
            self.0.space.wait(&mut inner);
        }
        if inner.closed {
            return Err(());
        }
        inner.bytes += cost;
        inner.queue.push_back(event);
        drop(inner);
        self.0.wake.notify_one();
        Ok(())
    }

    pub(super) fn drain_budget(&self) -> Vec<BackendEvent> {
        let start = Instant::now();
        let mut out = Vec::new();
        let mut inner = self.0.inner.lock();
        while out.len() < DRAIN_EVENTS && start.elapsed() < DRAIN_BUDGET {
            let Some(event) = inner.queue.pop_front() else {
                break;
            };
            inner.bytes = inner.bytes.saturating_sub(event_cost(&event));
            out.push(event);
        }
        let pending = !inner.queue.is_empty();
        drop(inner);
        if !out.is_empty() {
            self.0.space.notify_all();
        }
        let _ = pending;
        out
    }

    pub(super) fn pending(&self) -> bool {
        !self.0.inner.lock().queue.is_empty()
    }

    pub(super) fn wait(&self) -> impl std::future::Future<Output = ()> + '_ {
        self.0.wake.notified()
    }
}

fn fits(inner: &Inner, cost: usize) -> bool {
    inner.queue.is_empty() || (inner.queue.len() < MAX_EVENTS && inner.bytes + cost <= MAX_BYTES)
}

fn event_cost(event: &BackendEvent) -> usize {
    match event {
        BackendEvent::HaloAgentEvent { event, .. } => agent_cost(event).saturating_add(64),
        BackendEvent::CoderPtyOutput { data, .. } => data.len().saturating_add(32),
        _ => 64,
    }
}

fn agent_cost(event: &AgentEvent) -> usize {
    match event {
        AgentEvent::TextDelta { text }
        | AgentEvent::ReasoningDelta { text }
        | AgentEvent::ContentUpdated { text, .. } => text.len(),
        AgentEvent::SubagentEvent { event, .. } => agent_cost(event),
        _ => 0,
    }
}

fn coalesce(inner: &mut Inner, event: &BackendEvent) -> bool {
    let Some(tail) = inner.queue.back_mut() else {
        return false;
    };
    match (tail, event) {
        (
            BackendEvent::HaloAgentEvent {
                run_id,
                chat_id,
                event:
                    AgentEvent::ContentUpdated {
                        message_id,
                        content_index,
                        text,
                        reasoning,
                        append: true,
                        done: false,
                    },
            },
            BackendEvent::HaloAgentEvent {
                run_id: next_run,
                chat_id: next_chat,
                event:
                    AgentEvent::ContentUpdated {
                        message_id: next_message,
                        content_index: next_index,
                        text: next_text,
                        reasoning: next_reasoning,
                        append: true,
                        done: false,
                    },
            },
        ) if run_id == next_run
            && chat_id == next_chat
            && message_id == next_message
            && content_index == next_index
            && reasoning == next_reasoning
            && text.len() + next_text.len() <= COALESCE_LIMIT =>
        {
            text.push_str(next_text);
            inner.bytes += next_text.len();
            true
        }
        (
            BackendEvent::HaloAgentEvent {
                run_id,
                chat_id,
                event: AgentEvent::TextDelta { text },
            },
            BackendEvent::HaloAgentEvent {
                run_id: next_run,
                chat_id: next_chat,
                event: AgentEvent::TextDelta { text: next_text },
            },
        ) if run_id == next_run
            && chat_id == next_chat
            && text.len() + next_text.len() <= COALESCE_LIMIT =>
        {
            text.push_str(next_text);
            inner.bytes += next_text.len();
            true
        }
        (
            BackendEvent::HaloAgentEvent {
                run_id,
                chat_id,
                event: AgentEvent::ReasoningDelta { text },
            },
            BackendEvent::HaloAgentEvent {
                run_id: next_run,
                chat_id: next_chat,
                event: AgentEvent::ReasoningDelta { text: next_text },
            },
        ) if run_id == next_run
            && chat_id == next_chat
            && text.len() + next_text.len() <= COALESCE_LIMIT =>
        {
            text.push_str(next_text);
            inner.bytes += next_text.len();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo_backend::commands::halo_agent::AgentEvent;

    fn content(text: &str, append: bool, done: bool) -> BackendEvent {
        BackendEvent::HaloAgentEvent {
            run_id: "run".into(),
            chat_id: "chat".into(),
            event: AgentEvent::ContentUpdated {
                message_id: "m".into(),
                content_index: 1,
                text: text.into(),
                reasoning: false,
                append,
                done,
            },
        }
    }

    #[test]
    fn coalesces_only_adjacent_compatible_deltas() {
        let ingress = Ingress::new();
        ingress.send(content("a", true, false)).unwrap();
        ingress.send(content("b", true, false)).unwrap();
        ingress
            .send(BackendEvent::HaloAgentEvent {
                run_id: "run".into(),
                chat_id: "chat".into(),
                event: AgentEvent::ToolCall {
                    id: "t".into(),
                    call: halo_backend::commands::halo_agent::ToolCall::ReadFile {
                        path: "a.rs".into(),
                    },
                },
            })
            .unwrap();
        ingress.send(content("c", true, false)).unwrap();
        ingress.send(content("final", false, true)).unwrap();
        let batch = ingress.drain_budget();
        assert_eq!(batch.len(), 4);
        match &batch[0] {
            BackendEvent::HaloAgentEvent {
                event: AgentEvent::ContentUpdated { text, append, .. },
                ..
            } => {
                assert_eq!(text, "ab");
                assert!(*append);
            }
            _ => panic!("expected merged delta"),
        }
        assert!(matches!(
            batch[1],
            BackendEvent::HaloAgentEvent {
                event: AgentEvent::ToolCall { .. },
                ..
            }
        ));
        match &batch[3] {
            BackendEvent::HaloAgentEvent {
                event:
                    AgentEvent::ContentUpdated {
                        text, append, done, ..
                    },
                ..
            } => {
                assert_eq!(text, "final");
                assert!(!*append && *done);
            }
            _ => panic!("authoritative snapshot must stay intact"),
        }
    }
}

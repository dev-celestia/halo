//! Run identity, weak event callbacks, and non-blocking process handle ownership.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use halo_core::tauri::{AppHandle, Emitter, Manager};
use halo_harness::{AntigravityRun, OpenCodeRun, PiRun};

use super::cursor::CursorRun;

use super::types::{AgentEvent, AgentEventPayload, HarnessId, UserInputAnswer, UserInputQuestion};

#[derive(Clone)]
pub(crate) enum LiveHandle {
    Pi(PiRun),
    OpenCode(OpenCodeRun),
    Antigravity(AntigravityRun),
    Cursor(CursorRun),
}

impl LiveHandle {
    pub(crate) fn request_interrupt(&self) {
        match self {
            Self::Pi(run) => run.request_interrupt(),
            Self::OpenCode(run) => run.request_interrupt(),
            Self::Antigravity(run) => run.request_interrupt(),
            Self::Cursor(run) => run.request_interrupt(),
        }
    }

    /// Push a composer approval change into the process that is already running.
    pub(crate) fn set_approval_mode(&self, mode: &str) {
        match self {
            Self::Pi(run) => run.set_approval_mode(mode),
            Self::OpenCode(run) => run.set_approval_mode(mode),
            Self::Antigravity(run) => run.set_approval_mode(mode),
            // Cursor's SDK executes tools in-process and does not send approval
            // requests back to Halo. The chip is still stored for this session.
            Self::Cursor(_) => {}
        }
    }

    pub(crate) fn respond_input(
        &self,
        request_id: &str,
        confirmed: bool,
        label: Option<&str>,
    ) -> Result<(), String> {
        match self {
            Self::Pi(run) => run.respond_input(request_id, confirmed),
            Self::OpenCode(run) => run.respond_input(request_id, confirmed),
            Self::Antigravity(run) => run.respond_input_with_label(request_id, confirmed, label),
            Self::Cursor(_) => {
                let _ = (request_id, confirmed, label);
                Err("Cursor runs do not use Halo approval prompts".into())
            }
        }
    }

    pub(crate) fn steer_with_images(
        &self,
        command_id: &str,
        prompt: &str,
        images: Vec<halo_harness::PiImage>,
    ) -> Result<(), String> {
        match self {
            Self::Pi(run) => run.steer_with_images(command_id, prompt, images),
            Self::OpenCode(run) => run.steer_with_images(command_id, prompt, images),
            Self::Antigravity(run) => {
                let _ = command_id;
                run.steer_with_images(prompt, images)
            }
            Self::Cursor(run) => {
                let _ = command_id;
                run.steer_with_images(prompt, images)
            }
        }
    }

    /// Whether this harness can take a mid-run steer right now. The
    /// queue-first flow consults this so a follow-up stays Pending for the
    /// next drain instead of being dispatched into a turn that just ended.
    pub(crate) fn accepts_steers(&self) -> bool {
        match self {
            Self::Pi(run) => run.accepts_steers(),
            Self::OpenCode(run) => run.accepts_steers(),
            Self::Antigravity(run) => run.accepts_steers(),
            Self::Cursor(run) => run.accepts_steers(),
        }
    }
}

pub(crate) struct ActiveRun {
    pub(crate) run_id: String,
    /// Registered before startup. A stopped startup can never install its handle.
    pub(crate) handle: Option<LiveHandle>,
}

pub type RequestInputFn = Arc<dyn Fn(Vec<UserInputQuestion>) -> Vec<UserInputAnswer> + Send + Sync>;
pub(crate) type EventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;
type InputKey = (String, String, String);

#[derive(Default)]
pub struct HaloAgentState {
    pub(crate) active: Mutex<HashMap<String, ActiveRun>>,
    pub(crate) harness_sessions: Mutex<HashMap<String, (HarnessId, String)>>,
    /// Approval transport is asynchronous; never park the stdout reader on UI input.
    pub(crate) pending_inputs: Mutex<HashMap<InputKey, ()>>,
}

impl Drop for HaloAgentState {
    fn drop(&mut self) {
        for (_, run) in self.active.get_mut().drain() {
            if let Some(handle) = run.handle {
                handle.request_interrupt();
            }
        }
    }
}

pub fn ensure_managed(app: &AppHandle) {
    if app.try_state::<HaloAgentState>().is_some() {
        return;
    }
    // AppHandle::manage replaces an existing value, so concurrent first calls
    // must not replace a state that already owns another chat's live process.
    static INITIALIZE: Mutex<()> = Mutex::new(());
    let _guard = INITIALIZE.lock();
    if app.try_state::<HaloAgentState>().is_none() {
        app.manage(HaloAgentState::default());
    }
}

pub(crate) fn input_key(chat: &str, run: &str, request: &str) -> InputKey {
    (chat.to_owned(), run.to_owned(), request.to_owned())
}

pub(crate) fn clear_run_inputs(state: &HaloAgentState, chat: &str, run: &str) {
    state
        .pending_inputs
        .lock()
        .retain(|(c, r, _), _| c != chat || r != run);
}

pub(crate) fn is_current(app: &AppHandle, chat: &str, run: &str) -> bool {
    app.try_state::<HaloAgentState>().is_some_and(|state| {
        state
            .active
            .lock()
            .get(chat)
            .is_some_and(|active| active.run_id == run)
    })
}

/// Register nested approval traffic without touching run retirement.
fn observe_nested(state: &HaloAgentState, chat_id: &str, run_id: &str, event: &AgentEvent) {
    match event {
        AgentEvent::InputRequested { request_id, .. } => {
            state
                .pending_inputs
                .lock()
                .insert(input_key(chat_id, run_id, request_id), ());
        }
        AgentEvent::InputResolved { request_id } => {
            state
                .pending_inputs
                .lock()
                .remove(&input_key(chat_id, run_id, request_id));
        }
        AgentEvent::SubagentEvent { event, .. } => observe_nested(state, chat_id, run_id, event),
        _ => {}
    }
}

/// Validate before every side effect. A weak app prevents the cycle
/// AppHandle -> active process -> event callback -> AppHandle.
pub(crate) fn event_sink(app: &AppHandle, chat_id: &str, run_id: &str) -> EventSink {
    let weak_app = app.downgrade();
    let chat_id = chat_id.to_owned();
    let run_id = run_id.to_owned();
    Arc::new(move |event| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        let Some(state) = app.try_state::<HaloAgentState>() else {
            return;
        };
        let retired = {
            let mut active = state.active.lock();
            if !active.get(&chat_id).is_some_and(|run| run.run_id == run_id) {
                return;
            }
            match &event {
                AgentEvent::SessionStarted {
                    harness,
                    session_id,
                    ..
                } if !session_id.is_empty() => {
                    let harness_id = HarnessId::parse(harness).unwrap_or(HarnessId::Pi);
                    state
                        .harness_sessions
                        .lock()
                        .insert(chat_id.clone(), (harness_id, session_id.clone()));
                }
                AgentEvent::Done {
                    session_id: Some(session_id),
                    ..
                } if !session_id.is_empty() => {
                    // Keep whatever harness started this chat; only refresh id.
                    let mut sessions = state.harness_sessions.lock();
                    let harness = sessions
                        .get(&chat_id)
                        .map(|(h, _)| *h)
                        .unwrap_or(HarnessId::Pi);
                    sessions.insert(chat_id.clone(), (harness, session_id.clone()));
                }
                AgentEvent::InputRequested { request_id, .. } => {
                    state
                        .pending_inputs
                        .lock()
                        .insert(input_key(&chat_id, &run_id, request_id), ());
                }
                AgentEvent::InputResolved { request_id } => {
                    state
                        .pending_inputs
                        .lock()
                        .remove(&input_key(&chat_id, &run_id, request_id));
                }
                // Child approvals are answerable through the same panel.
                AgentEvent::SubagentEvent { event, .. } => {
                    observe_nested(&state, &chat_id, &run_id, event)
                }
                _ => {}
            }
            if matches!(event, AgentEvent::Done { .. }) {
                clear_run_inputs(&state, &chat_id, &run_id);
                active.remove(&chat_id)
            } else {
                None
            }
        };
        // Handle teardown and subscriber callbacks must never run under active.lock().
        drop(retired);
        let _ = app.emit(
            "coder-agent-event",
            &AgentEventPayload {
                run_id: run_id.clone(),
                chat_id: chat_id.clone(),
                event,
            },
        );
    })
}

#[cfg(test)]
mod tests {
    use super::super::types::DoneStatus;
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn register(app: &AppHandle, chat: &str, run: &str) {
        ensure_managed(app);
        app.state::<HaloAgentState>().active.lock().insert(
            chat.into(),
            ActiveRun {
                run_id: run.into(),
                handle: None,
            },
        );
    }

    fn done(session: &str) -> AgentEvent {
        AgentEvent::Done {
            status: DoneStatus::Completed,
            result: None,
            error: None,
            session_id: Some(session.into()),
        }
    }

    #[test]
    fn concurrent_initialization_reuses_one_managed_state() {
        let app = AppHandle::new();
        let gate = Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let app = app.clone();
                let gate = gate.clone();
                std::thread::spawn(move || {
                    gate.wait();
                    ensure_managed(&app);
                    app.state::<HaloAgentState>().arc()
                })
            })
            .collect();
        let states: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(states.iter().all(|state| Arc::ptr_eq(state, &states[0])));
    }

    #[test]
    fn stale_and_duplicate_completion_cannot_mutate_current_run() {
        let app = AppHandle::new();
        register(&app, "chat", "new");
        let count = Arc::new(AtomicUsize::new(0));
        let events = count.clone();
        app.on_event(move |_, _| {
            events.fetch_add(1, Ordering::SeqCst);
        });
        let old = event_sink(&app, "chat", "old");
        old(done("stale-session"));
        assert!(is_current(&app, "chat", "new"));
        assert!(
            app.state::<HaloAgentState>()
                .harness_sessions
                .lock()
                .is_empty()
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let current = event_sink(&app, "chat", "new");
        current(done("current-session"));
        current(done("duplicate-session"));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            app.state::<HaloAgentState>().harness_sessions.lock()["chat"].1,
            "current-session"
        );
    }

    #[test]
    fn finishing_one_chat_preserves_another_chats_approval() {
        let app = AppHandle::new();
        register(&app, "a", "a-run");
        register(&app, "b", "b-run");
        let state = app.state::<HaloAgentState>();
        state
            .pending_inputs
            .lock()
            .insert(input_key("a", "a-run", "same-id"), ());
        state
            .pending_inputs
            .lock()
            .insert(input_key("b", "b-run", "same-id"), ());
        event_sink(&app, "a", "a-run")(done("a-session"));
        assert!(
            !state
                .pending_inputs
                .lock()
                .contains_key(&input_key("a", "a-run", "same-id"))
        );
        assert!(
            state
                .pending_inputs
                .lock()
                .contains_key(&input_key("b", "b-run", "same-id"))
        );
        assert!(is_current(&app, "b", "b-run"));
    }

    #[test]
    fn event_callback_does_not_keep_app_alive() {
        let app = AppHandle::new();
        register(&app, "a", "run");
        let weak = app.downgrade();
        let sink = event_sink(&app, "a", "run");
        drop(app);
        assert!(weak.upgrade().is_none());
        sink(done("ignored"));
    }

    #[test]
    fn listeners_can_reenter_run_state_without_locking_deadlock() {
        let app = AppHandle::new();
        register(&app, "a", "run");
        let weak = app.downgrade();
        app.on_event(move |_, _| {
            let app = weak.upgrade().unwrap();
            assert!(app.state::<HaloAgentState>().active.try_lock().is_some());
        });
        event_sink(&app, "a", "run")(done("session"));
    }
}

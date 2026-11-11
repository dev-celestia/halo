//! One worker owns Child, writer, deadlines, protocol correlation and shutdown.
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::ExitStatus;
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use uuid::Uuid;

use super::process::{Transport, is_permission_failure, signal_group};
use super::protocol::{self, Frame, Message, Model, Models, State, Transcript};
use super::run::{COMMAND_TIMEOUT, Control, Mailbox, STARTUP_TIMEOUT, Shared};
use super::types::{PiDoneStatus, PiEvent};

pub(crate) struct Config {
    pub run_id: String,
    pub cwd: String,
    pub model: Option<String>,
    pub initial: Vec<u8>,
    pub approval_nonce: Option<String>,
    // Must outlive child shutdown and reaping.
    pub extension: Option<tempfile::TempDir>,
}

#[derive(Clone, Copy)]
enum Purpose {
    State,
    Models,
    SetModel,
    Prompt,
    Steer,
}
impl Purpose {
    fn command(self) -> &'static str {
        match self {
            Self::State => "get_state",
            Self::Models => "get_available_models",
            Self::SetModel => "set_model",
            Self::Prompt => "prompt",
            Self::Steer => "steer",
        }
    }
    fn delivery(self) -> bool {
        matches!(self, Self::Prompt | Self::Steer)
    }
}
struct Pending {
    purpose: Purpose,
    deadline: Instant,
}

struct Supervisor {
    io: Transport,
    config: Config,
    shared: Arc<Shared>,
    mailbox: Arc<Mailbox>,
    controls: mpsc::Receiver<Control>,
    ready: Option<mpsc::SyncSender<Result<(), String>>>,
    pending: HashMap<String, Pending>,
    state: Option<State>,
    models: Option<Vec<Model>>,
    selected: Option<Model>,
    set_model_sent: bool,
    approval_ready: bool,
    approval_ids: HashSet<String>,
    started: bool,
    agent_seen: bool,
    settled: Option<bool>,
    /// From `get_state` / selected model; used to pair with message usage.
    context_window: Option<u64>,
    transcript: Transcript,
    out: Vec<PiEvent>,
    exit: Option<ExitStatus>,
}

pub(crate) fn supervise(
    io: Transport,
    config: Config,
    shared: Arc<Shared>,
    mailbox: Arc<Mailbox>,
    controls: mpsc::Receiver<Control>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let approval_ready = config.approval_nonce.is_none();
    let mut supervisor = Supervisor {
        io,
        config,
        shared,
        mailbox,
        controls,
        ready: Some(ready),
        pending: HashMap::new(),
        state: None,
        models: None,
        selected: None,
        set_model_sent: false,
        approval_ready,
        approval_ids: HashSet::new(),
        started: false,
        agent_seen: false,
        settled: None,
        context_window: None,
        transcript: Transcript::default(),
        out: Vec::new(),
        exit: None,
    };
    let result = supervisor.run();
    let (status, error) = match result {
        Ok(status) => (status, None),
        Err(error) => (PiDoneStatus::Errored, Some(error)),
    };
    supervisor.finish(status, error);
}

impl Supervisor {
    fn query(&mut self, purpose: Purpose, mut data: Value) -> Result<(), String> {
        let id = format!("halo-internal-{}", Uuid::new_v4());
        data["id"] = json!(id);
        data["type"] = json!(purpose.command());
        self.io
            .enqueue(serde_json::to_vec(&data).map_err(|_| "Cannot encode Pi query")?)?;
        self.pending.insert(
            id,
            Pending {
                purpose,
                deadline: Instant::now() + COMMAND_TIMEOUT,
            },
        );
        Ok(())
    }

    fn run(&mut self) -> Result<PiDoneStatus, String> {
        self.query(Purpose::State, json!({}))?;
        self.query(Purpose::Models, json!({}))?;
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            if self.shared.interrupted.load(Ordering::Acquire)
                || self.shared.shutdown.load(Ordering::Acquire)
            {
                return Ok(PiDoneStatus::Interrupted);
            }
            if !self.started && Instant::now() >= deadline {
                return Err(if !self.approval_ready {
                    "Pi approval hook did not become ready; no prompt was sent".into()
                } else {
                    "Pi startup handshake timed out; no prompt was sent".into()
                });
            }
            if self.exit.is_none() {
                self.exit = self.io.try_wait()?;
            }
            self.io.drain_diagnostics();
            self.controls()?;
            self.expire_approvals()?;
            self.io.write_pending()?;
            // Backpressure the pipe, not lifecycle management or cancellation.
            if self.mailbox.try_push(&mut self.out) {
                if let Some(bytes) = self.io.read_frame().map_err(|e| self.exit_error(e))? {
                    if !bytes.is_empty() {
                        self.frame(protocol::parse(&bytes)?)?;
                    }
                }
            }
            if let Some((id, _)) = self
                .pending
                .iter()
                .find(|(_, p)| Instant::now() >= p.deadline)
            {
                let id = id.clone();
                let error = if self.started {
                    "Pi command acknowledgement timed out; delivery is unknown. Do not resend automatically."
                } else {
                    "Pi capability query timed out; verify @earendil-works/pi-coding-agent 1.4.0. No prompt was sent."
                };
                if self.pending[&id].purpose.delivery() {
                    self.reject(&id, error.into());
                }
                return Err(error.into());
            }
            if !self.started {
                self.start_if_ready()?;
            }
            if self.started
                && self.settled.is_some()
                && !self.pending.values().any(|p| p.purpose.delivery())
            {
                if self.settled == Some(true) || self.transcript.aborted {
                    return Ok(PiDoneStatus::Interrupted);
                }
                if let Some(error) = self.transcript.error.take() {
                    return Err(error);
                }
                return Ok(PiDoneStatus::Completed);
            }
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }

    fn exit_error(&self, error: String) -> String {
        let mut message = match self.exit {
            // `ExitStatus` Display is already "exit status: N" / "signal: N".
            Some(status) => format!("{error} ({status}); completion was not confirmed"),
            None => error,
        };
        // A child that dies before the handshake failed on its environment, not
        // on the model. Quote the CLI's own line when it is one we recognise —
        // otherwise the bubble can only offer a Resend that fails identically.
        if !self.started {
            if let Some(cause) = self.io.startup_diagnosis() {
                message.push_str(&format!(". Pi said: {cause}"));
                if is_permission_failure(cause) {
                    message.push_str(
                        " — Pi can't write its state directory, so retrying will fail the same \
                         way. Open Doctor → Fix Ownership (nothing is deleted).",
                    );
                }
            }
        }
        message
    }

    fn start_if_ready(&mut self) -> Result<(), String> {
        let (Some(state), Some(models)) = (&self.state, &self.models) else {
            return Ok(());
        };
        if !self.approval_ready {
            return Ok(());
        }
        if self.selected.is_none() {
            if models.is_empty() {
                return Err("Pi reported no available models. Configure a provider API key in Agent settings; no prompt was sent.".into());
            }
            let matches: Vec<_> = models
                .iter()
                .filter(|m| match &self.config.model {
                    Some(requested) => *requested == m.key() || *requested == m.id,
                    None => state.model.as_ref().is_some_and(|s| s.key() == m.key()),
                })
                .cloned()
                .collect();
            if matches.len() != 1 {
                let requested = self
                    .config
                    .model
                    .as_deref()
                    .unwrap_or("(Pi session default)");
                return Err(format!(
                    "Selected Pi model `{requested}` is unavailable or ambiguous. Choose an exact provider/model from get_available_models; no prompt was sent."
                ));
            }
            self.selected = matches.into_iter().next();
            if let Some(window) = self.selected.as_ref().and_then(|m| m.context_window) {
                self.context_window = Some(window);
            }
        }
        let selected = self.selected.as_ref().unwrap();
        if state.model.as_ref().map(Model::key) != Some(selected.key()) {
            if self.set_model_sent {
                return Err("Pi did not select the requested model; no prompt was sent".into());
            }
            let data = json!({"provider": selected.provider, "modelId": selected.id});
            self.state = None;
            self.set_model_sent = true;
            return self.query(Purpose::SetModel, data);
        }
        if state.is_streaming || state.is_compacting {
            return Err("Pi was already busy during startup; no prompt was sent".into());
        }
        let session = state.session_file.as_ref().filter(|s| Path::new(s).is_absolute() && !s.is_empty())
            .ok_or("Pi get_state did not provide an absolute sessionFile; cannot safely resume this run")?.clone();
        if state.session_id.is_empty() {
            return Err("Pi get_state omitted its sessionId".into());
        }
        *self.shared.session.lock() = Some(session.clone());
        self.io.enqueue(std::mem::take(&mut self.config.initial))?;
        self.pending.insert(
            self.config.run_id.clone(),
            Pending {
                purpose: Purpose::Prompt,
                deadline: Instant::now() + COMMAND_TIMEOUT,
            },
        );
        {
            let mut commands = self.shared.commands.lock();
            commands.seen.insert(self.config.run_id.clone());
            commands.pending.insert(self.config.run_id.clone());
        }
        self.out.push(PiEvent::SessionStarted {
            session_id: session,
            model: selected.key(),
            cwd: self.config.cwd.clone(),
        });
        self.started = true;
        self.shared.accepting.store(true, Ordering::Release);
        if let Some(ready) = self.ready.take() {
            let _ = ready.send(Ok(()));
        }
        Ok(())
    }

    fn response(
        &mut self,
        id: Option<String>,
        command: String,
        success: bool,
        data: Option<Value>,
        error: Option<String>,
    ) -> Result<(), String> {
        let Some(id) = id else {
            return Err("Pi returned an uncorrelated response; delivery is unknown".into());
        };
        let Some(pending) = self.pending.remove(&id) else {
            return Ok(());
        };
        if pending.purpose.command() != command {
            return Err("Pi response command did not match its request ID".into());
        }
        if !success {
            let error = protocol::brief(error.unwrap_or_else(|| format!("Pi rejected {command}")));
            if pending.purpose.delivery() {
                self.reject(&id, error.clone());
            }
            if matches!(pending.purpose, Purpose::Steer) {
                return Ok(());
            }
            return Err(error);
        }
        let data = data.unwrap_or(Value::Null);
        match pending.purpose {
            Purpose::State => {
                let state: State = serde_json::from_value(data).map_err(
                    |_| "Pi get_state is missing required 1.4.0 fields; no prompt was sent",
                )?;
                if let Some(window) = state.model.as_ref().and_then(|m| m.context_window) {
                    self.context_window = Some(window);
                }
                self.state = Some(state);
            }
            Purpose::Models => {
                self.models = Some(
                    serde_json::from_value::<Models>(data)
                        .map_err(|_| "Pi get_available_models is unsupported; no prompt was sent")?
                        .models,
                )
            }
            Purpose::SetModel => self.query(Purpose::State, json!({}))?,
            Purpose::Prompt | Purpose::Steer => {
                let disposition = data["disposition"].as_str().unwrap_or("");
                let valid = matches!(disposition, "queued" | "handled")
                    || (matches!(pending.purpose, Purpose::Prompt) && disposition == "started");
                if !valid {
                    let error = "Pi omitted a supported command disposition; delivery is unknown (do not replay)";
                    self.reject(&id, error.into());
                    return Err(error.into());
                }
                self.shared.commands.lock().pending.remove(&id);
                self.out.push(PiEvent::CommandAccepted { command_id: id });
                if matches!(pending.purpose, Purpose::Prompt)
                    && disposition == "handled"
                    && !self.agent_seen
                {
                    self.settled = Some(false);
                }
            }
        }
        Ok(())
    }

    fn frame(&mut self, frame: Frame) -> Result<(), String> {
        match frame {
            Frame::Response {
                id,
                command,
                success,
                data,
                error,
            } => self.response(id, command, success, data, error),
            Frame::AgentStart => {
                if !self.started {
                    return Err("Pi began agent work before startup approval; no prompt was sent by the host".into());
                }
                self.agent_seen = true;
                self.settled = None;
                Ok(())
            }
            Frame::AgentSettled { aborted } => {
                if self.started {
                    self.settled = Some(aborted);
                }
                Ok(())
            }
            Frame::MessageEnd { message } => {
                self.emit_usage_from_message(&message);
                self.transcript
                    .handle(Frame::MessageEnd { message }, &mut self.out)
            }
            Frame::ExtensionError => {
                Err("Pi extension enforcement failed; the run was stopped".into())
            }
            Frame::ExtensionUiRequest {
                id,
                method,
                title,
                message,
                timeout,
                status_key,
                status_text,
            } => {
                if method == "setStatus"
                    && status_key.as_deref() == Some("halo-approval-v1")
                    && status_text == self.config.approval_nonce
                    && self.config.approval_nonce.is_some()
                {
                    self.approval_ready = true;
                    return Ok(());
                }
                if matches!(
                    method.as_str(),
                    "notify" | "setStatus" | "setWidget" | "setTitle" | "set_editor_text"
                ) {
                    return Ok(());
                }
                if method != "confirm" || !self.started || !self.approval_ready {
                    self.io.enqueue(
                        serde_json::to_vec(
                            &json!({"type":"extension_ui_response","id":id,"cancelled":true}),
                        )
                        .unwrap(),
                    )?;
                    return Err("Pi requested an unsupported or premature approval dialog; execution was denied".into());
                }
                if id.is_empty()
                    || id.len() > 256
                    || self.approval_ids.len() >= 1024
                    || !self.approval_ids.insert(id.clone())
                {
                    return Err("Pi sent an invalid or reused approval request ID".into());
                }
                let ttl = timeout.unwrap_or(60000).clamp(1, 60000);
                // Expire on the host before Pi, so delayed/stale affirmative answers cannot authorize a later call.
                let deadline =
                    Instant::now() + Duration::from_millis(ttl.saturating_sub(100).max(ttl / 2));
                {
                    let mut approvals = self.shared.approvals.lock();
                    if approvals.len() >= protocol::MAX_PENDING {
                        return Err("Too many pending Pi approvals".into());
                    }
                    approvals.insert(id.clone(), deadline);
                }
                self.out.push(PiEvent::InputRequested {
                    request_id: id,
                    title: protocol::brief(title.unwrap_or_else(|| "Allow tool execution?".into())),
                    message: protocol::brief(message.unwrap_or_default()),
                });
                Ok(())
            }
            other => self.transcript.handle(other, &mut self.out),
        }
    }

    fn reject(&mut self, id: &str, error: String) {
        if self.shared.commands.lock().pending.remove(id) {
            self.out.push(PiEvent::CommandRejected {
                command_id: id.into(),
                error,
            });
        }
    }

    /// Pair assistant message usage with the model window for the composer meter.
    fn emit_usage_from_message(&mut self, message: &Message) {
        if message.role != "assistant" {
            return;
        }
        let Some(usage) = message.usage.as_ref() else {
            return;
        };
        let used_tokens = usage.context_tokens();
        if used_tokens == 0 {
            return;
        }
        let model_key = self
            .selected
            .as_ref()
            .map(Model::key)
            .or_else(|| {
                self.state
                    .as_ref()
                    .and_then(|s| s.model.as_ref().map(Model::key))
            })
            .unwrap_or_default();
        let window_tokens = self
            .context_window
            .or_else(|| self.selected.as_ref().and_then(|m| m.context_window))
            .unwrap_or_else(|| crate::usage::context_window_for(&model_key));
        if window_tokens == 0 {
            return;
        }
        self.out.push(PiEvent::Usage {
            used_tokens: used_tokens.min(window_tokens.saturating_mul(2)),
            window_tokens,
            input_tokens: usage.input,
            output_tokens: usage.output,
        });
    }

    fn controls(&mut self) -> Result<(), String> {
        for _ in 0..8 {
            let Ok(control) = self.controls.try_recv() else {
                break;
            };
            match control {
                Control::Steer { id, bytes } => {
                    if self.settled.is_some() {
                        self.reject(
                            &id,
                            "Pi already settled; steering command was not sent".into(),
                        );
                    } else if let Err(error) = self.io.enqueue(bytes) {
                        self.reject(&id, error);
                    } else {
                        self.pending.insert(
                            id,
                            Pending {
                                purpose: Purpose::Steer,
                                deadline: Instant::now() + COMMAND_TIMEOUT,
                            },
                        );
                    }
                }
                Control::Input {
                    id,
                    confirmed,
                    deadline,
                } => {
                    let confirmed = confirmed && Instant::now() < deadline;
                    self.io.enqueue(
                        serde_json::to_vec(
                            &json!({"type":"extension_ui_response","id":id,"confirmed":confirmed}),
                        )
                        .unwrap(),
                    )?;
                    self.out.push(PiEvent::InputResolved { request_id: id });
                }
            }
        }
        Ok(())
    }

    fn expire_approvals(&mut self) -> Result<(), String> {
        let expired: Vec<_> = {
            let mut approvals = self.shared.approvals.lock();
            let ids: Vec<_> = approvals
                .iter()
                .filter(|(_, end)| Instant::now() >= **end)
                .map(|(id, _)| id.clone())
                .collect();
            for id in &ids {
                approvals.remove(id);
            }
            ids
        };
        for id in expired {
            self.io.enqueue(
                serde_json::to_vec(
                    &json!({"type":"extension_ui_response","id":id,"confirmed":false}),
                )
                .unwrap(),
            )?;
            self.out.push(PiEvent::InputResolved { request_id: id });
        }
        Ok(())
    }

    fn finish(&mut self, mut status: PiDoneStatus, mut error: Option<String>) {
        self.shared.accepting.store(false, Ordering::Release);
        if let Some(ready) = self.ready.take() {
            let _ = ready.send(Err(error
                .clone()
                .unwrap_or_else(|| "Pi startup was cancelled; no prompt was sent".into())));
        }
        let ids: Vec<_> = self
            .shared
            .commands
            .lock()
            .pending
            .iter()
            .cloned()
            .collect();
        for id in ids {
            self.reject(&id, "Pi stopped before acknowledgement; delivery is unknown. Do not replay automatically.".into());
        }
        let approvals: Vec<_> = self
            .shared
            .approvals
            .lock()
            .drain()
            .map(|(id, _)| id)
            .collect();
        for id in approvals {
            self.out.push(PiEvent::InputResolved { request_id: id });
        }
        // Answers removed by the handle but not yet consumed still need a resolution event.
        while let Ok(control) = self.controls.try_recv() {
            if let Control::Input { id, .. } = control {
                self.out.push(PiEvent::InputResolved { request_id: id });
            }
        }
        self.transcript.finish_blocks(&mut self.out);
        let since = Instant::now();
        if status == PiDoneStatus::Completed {
            self.io.close_stdin();
        } else {
            self.io.abort();
        }
        let mut signalled = false;
        loop {
            if self.exit.is_none() {
                self.exit = self.io.try_wait().ok().flatten();
            }
            if let Some(exit) = self.exit {
                if !exit.success() && !signalled && status == PiDoneStatus::Completed {
                    status = PiDoneStatus::Errored;
                    error = Some(format!("Pi exited unsuccessfully after settling: {exit}"));
                }
                break;
            }
            self.io.drain_diagnostics();
            let _ = self.io.write_pending();
            // Drain without retaining protocol data while stopping.
            let _ = self.io.read_frame();
            if since.elapsed() >= Duration::from_millis(250) {
                self.io.close_stdin();
            }
            if since.elapsed() >= Duration::from_millis(750) && !signalled {
                signal_group(&mut self.io.child, false);
                signalled = true;
            }
            if since.elapsed() >= Duration::from_millis(1500) {
                signal_group(&mut self.io.child, true);
                let _ = self.io.child.wait();
                break;
            }
            std::thread::park_timeout(Duration::from_millis(10));
        }
        // Reap the leader and kill any remaining owned tool descendants, even if Pi exited first.
        signal_group(&mut self.io.child, true);
        let _ = self.io.child.wait();
        self.shared.alive.store(false, Ordering::Release);
        self.config.extension.take();
        if let Some(error) = &error {
            self.out.push(PiEvent::Error {
                message: error.clone(),
            });
        }
        let session_id = self.shared.session.lock().clone();
        self.out.push(PiEvent::Done {
            status,
            session_id,
            error,
        });
        self.mailbox.finish(std::mem::take(&mut self.out));
    }
}


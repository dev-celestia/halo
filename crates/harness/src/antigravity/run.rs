//! Antigravity ACP harness: spawn `agy_acp_server`, drive JSON-RPC over stdio.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use parking_lot::Mutex as ParkingMutex;
use serde_json::{Value, json};

use super::jsonrpc::{Incoming, RpcClient};
use super::normalize::{map_update, preferred_allow_option, preferred_reject_option, usage_from};
use super::probe;
use super::types::{
    AntigravityDoneStatus, AntigravityEvent, AntigravityProbeResult, AntigravityRunOptions,
};

enum Control {
    Interrupt,
    Steer {
        text: String,
        images: Vec<crate::PiImage>,
    },
    Input {
        id: String,
        confirmed: bool,
        label: Option<String>,
    },
}

struct Shared {
    shutdown: AtomicBool,
    interrupted: AtomicBool,
    alive: AtomicBool,
    accepting: AtomicBool,
    /// Composer approval mode, readable while this process is already running.
    approval_mode: ParkingMutex<String>,
    /// Read-only plan mode flag.
    plan_mode: AtomicBool,
    /// Set when the composer changes mode; the supervisor re-checks every
    /// tool prompt that is already on screen.
    flush_allows: AtomicBool,
    session_id: ParkingMutex<Option<String>>,
    /// Mid-turn user text + images. Delivered as the next `session/prompt` on this
    /// process when the current one returns, without ending the Halo run.
    pending_steers: ParkingMutex<Vec<(String, Vec<crate::PiImage>)>>,
    /// ACP permission request id → options and the tool the card is for.
    pending_permissions: ParkingMutex<HashMap<String, PendingPermission>>,
}

struct PendingPermission {
    rpc_id: Value,
    options: Vec<Value>,
    tool: String,
    input: String,
    question: bool,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            shutdown: AtomicBool::new(false),
            interrupted: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            accepting: AtomicBool::new(true),
            approval_mode: ParkingMutex::new("ask".into()),
            plan_mode: AtomicBool::new(false),
            flush_allows: AtomicBool::new(false),
            session_id: ParkingMutex::new(None),
            pending_steers: ParkingMutex::new(Vec::new()),
            pending_permissions: ParkingMutex::new(HashMap::new()),
        }
    }
}

struct Handle {
    shared: Arc<Shared>,
    control: mpsc::SyncSender<Control>,
    worker: thread::Thread,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.worker.unpark();
    }
}

#[derive(Clone)]
pub struct AntigravityRun {
    handle: Arc<Handle>,
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn as_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("child guard still owns process")
    }

    fn into_inner(mut self) -> Child {
        self.0.take().expect("child guard still owns process")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl AntigravityRun {
    pub fn spawn(
        options: AntigravityRunOptions,
        on_event: impl Fn(AntigravityEvent) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let executable = match probe::probe_environment(options.custom_executable.as_deref()) {
            AntigravityProbeResult::Ready { path, .. } => path,
            AntigravityProbeResult::Missing => {
                // Best-effort install of the pinned registry archive.
                super::install::ensure_installed().map_err(|e| {
                    format!(
                        "Antigravity ACP server missing and install failed: {e}. Set ANTIGRAVITY_ACP_EXECUTABLE or install from settings."
                    )
                })?
            }
            AntigravityProbeResult::Failed { message } => return Err(message),
        };

        let cwd = options
            .cwd
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .ok_or_else(|| {
                "Antigravity working directory is missing or not a directory".to_string()
            })?;

        // `session/new` is refused until a provider is selected. Sign in first
        // when nothing is stored so the browser flow finishes before the prompt.
        if !super::auth::is_signed_in() {
            super::auth::run_login(options.custom_executable.as_deref(), |_| {})
                .map_err(|e| format!("Google sign-in for Antigravity failed: {e}"))?;
        }

        let mut child = ChildGuard(Some(spawn_server(&executable, &cwd, &options.extra_env)?));
        let stdin = child
            .as_mut()
            .stdin
            .take()
            .ok_or_else(|| "agy_acp_server has no stdin".to_string())?;
        let stdout = child
            .as_mut()
            .stdout
            .take()
            .ok_or_else(|| "agy_acp_server has no stdout".to_string())?;
        if let Some(stderr) = child.as_mut().stderr.take() {
            thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    tracing::debug!(target: "antigravity.stderr", "{line}");
                }
            });
        }

        let (client, incoming) = RpcClient::start(stdin, stdout);
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientInfo": {
                    "name": "halo",
                    "title": "Halo",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false,
                },
            }),
        )?;

        // Select the Google method on this process. Credentials from the gate
        // above make this return immediately; without them the server starts a
        // loopback wait, so keep the timeout short and treat it as non-fatal.
        if init_has_auth_methods(&init) {
            let method = super::auth::preferred_auth_method(&init);
            let _ = client.request_timeout(
                "authenticate",
                json!({ "methodId": method }),
                Duration::from_secs(45),
            );
        }

        let resume = options
            .resume
            .as_deref()
            .map(str::trim)
            .filter(|s| is_valid_antigravity_session_id(s));
        let (session_id, resumed) = match open_session(&client, resume, &cwd) {
            Ok(opened) => opened,
            Err(err)
                if err.contains("Authentication required")
                    && !err.contains("Permission denied")
                    && !err.contains("could not be read") =>
            {
                // Stored tokens vanished or expired between probe and session.
                super::auth::run_login(options.custom_executable.as_deref(), |_| {})
                    .map_err(|e| format!("Google sign-in for Antigravity failed: {e}"))?;
                let _ = client.request_timeout(
                    "authenticate",
                    json!({ "methodId": super::auth::preferred_auth_method(&init) }),
                    Duration::from_secs(45),
                );
                open_session(&client, resume, &cwd)?
            }
            Err(err) => return Err(err),
        };

        if let Some(model) = options.model.as_deref().filter(|m| !m.is_empty()) {
            // Best-effort model selection via config option (ids vary by account).
            let _ = client.request(
                "session/set_config_option",
                json!({
                    "sessionId": session_id,
                    "configId": "model",
                    "value": model,
                }),
            );
        }

        let shared = Arc::new(Shared::default());
        *shared.session_id.lock() = Some(session_id.clone());

        let on_event = Arc::new(on_event);
        on_event(AntigravityEvent::SessionStarted {
            session_id: session_id.clone(),
            model: options.model.clone(),
            cwd: Some(cwd.display().to_string()),
        });
        on_event(AntigravityEvent::CommandAccepted {
            command_id: options.run_id.clone(),
        });

        let effective_prompt = match options
            .fallback_context
            .as_deref()
            .map(str::trim)
            .filter(|ctx| !resumed && !ctx.is_empty())
        {
            Some(ctx) => format!("{ctx}\n\n{}", options.prompt),
            None => options.prompt.clone(),
        };
        let mut prompt = vec![json!({ "type": "text", "text": effective_prompt })];
        prompt.extend(options.images.iter().map(|image| {
            json!({
                "type": "image",
                "data": image.data,
                "mimeType": image.mime_type,
            })
        }));
        let prompt_params = json!({
            "sessionId": session_id,
            "prompt": prompt,
        });

        let (tx, rx) = mpsc::sync_channel(16);
        // The chip can change after this process is spawned. The supervisor
        // reads this on every permission, not the mode from startup.
        *shared.approval_mode.lock() =
            crate::approval::canonical_mode(&options.approval_mode).to_string();
        shared
            .plan_mode
            .store(options.plan_mode, Ordering::Release);
        let worker_shared = Arc::clone(&shared);
        let on_event_worker = Arc::clone(&on_event);
        let client_worker = client.clone();
        let session_for_worker = session_id.clone();
        let owned_child = child.into_inner();
        let worker = thread::Builder::new()
            .name("antigravity-acp".into())
            .spawn(move || {
                supervise(
                    owned_child,
                    client_worker,
                    incoming,
                    session_for_worker,
                    prompt_params,
                    worker_shared,
                    rx,
                    on_event_worker,
                );
            })
            .map_err(|e| format!("Cannot start Antigravity supervisor: {e}"))?;

        Ok(Self {
            handle: Arc::new(Handle {
                shared,
                control: tx,
                worker: worker.thread().clone(),
            }),
        })
    }

    pub fn steer_text(&self, prompt: &str) -> Result<(), String> {
        self.steer_with_images(prompt, Vec::new())
    }

    pub fn steer_with_images(
        &self,
        prompt: &str,
        images: Vec<crate::PiImage>,
    ) -> Result<(), String> {
        if !self.handle.shared.accepting.load(Ordering::Acquire) {
            return Err("Antigravity is no longer accepting commands".into());
        }
        self.handle
            .control
            .try_send(Control::Steer {
                text: prompt.to_string(),
                images,
            })
            .map_err(|_| "Antigravity command queue is full or closed")?;
        self.handle.worker.unpark();
        Ok(())
    }

    pub fn respond_input(&self, request_id: &str, confirmed: bool) -> Result<(), String> {
        self.respond_input_with_label(request_id, confirmed, None)
    }

    pub fn respond_input_with_label(
        &self,
        request_id: &str,
        confirmed: bool,
        label: Option<&str>,
    ) -> Result<(), String> {
        if !self.handle.shared.accepting.load(Ordering::Acquire)
            || self.handle.shared.interrupted.load(Ordering::Acquire)
        {
            return Err("Antigravity approval is no longer active".into());
        }
        self.handle
            .control
            .try_send(Control::Input {
                id: request_id.into(),
                confirmed,
                label: label.map(str::to_string),
            })
            .map_err(|_| "Antigravity approval queue is full or closed")?;
        self.handle.worker.unpark();
        Ok(())
    }

    /// Apply a composer approval change to this already-running process.
    pub fn set_approval_mode(&self, mode: &str) {
        *self.handle.shared.approval_mode.lock() =
            crate::approval::canonical_mode(mode).to_string();
        self.handle
            .shared
            .flush_allows
            .store(true, Ordering::Release);
        self.handle.worker.unpark();
    }

    pub fn request_interrupt(&self) {
        self.handle.shared.accepting.store(false, Ordering::Release);
        self.handle
            .shared
            .interrupted
            .store(true, Ordering::Release);
        let _ = self.handle.control.try_send(Control::Interrupt);
        self.handle.worker.unpark();
    }

    pub fn is_alive(&self) -> bool {
        self.handle.shared.alive.load(Ordering::Acquire)
    }

    /// Whether a steer could still reach this run. `accepting` closes when the
    /// turn is ending (before Done) or the process is being torn down, so a
    /// late follow-up is refused instead of silently swallowed.
    pub fn accepts_steers(&self) -> bool {
        self.handle.shared.accepting.load(Ordering::Acquire)
            && !self.handle.shared.interrupted.load(Ordering::Acquire)
    }

    pub fn session_id(&self) -> Option<String> {
        self.handle.shared.session_id.lock().clone()
    }
}

pub(super) fn spawn_server(
    executable: &Path,
    cwd: &Path,
    extra_env: &HashMap<String, String>,
) -> Result<Child, String> {
    let mut cmd = Command::new(executable);
    cmd.current_dir(cwd)
        .args(probe::launch_args())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GEMINI_HOME", probe::gemini_home())
        .env("PYTHONUNBUFFERED", "1");
    if let Some(parent) = executable.parent() {
        #[cfg(windows)]
        let sibling = parent.join("localharness_external.exe");
        #[cfg(not(windows))]
        let sibling = parent.join("localharness_external");
        if sibling.is_file() {
            cmd.env("ANTIGRAVITY_HARNESS_PATH", &sibling);
        }
    }
    // Python's webbrowser honors `BROWSER`. Point it at the real opener.
    // `/usr/bin/true` exits 0 without a window, so sign-in reports success
    // and the Google page never appears.
    #[cfg(target_os = "macos")]
    {
        cmd.env("BROWSER", "/usr/bin/open");
    }
    #[cfg(target_os = "linux")]
    {
        cmd.env("BROWSER", "xdg-open");
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|e| format!("failed to spawn agy_acp_server: {e}"))
}

fn init_has_auth_methods(init: &Value) -> bool {
    init.get("authMethods")
        .and_then(Value::as_array)
        .is_some_and(|methods| !methods.is_empty())
}

fn is_valid_antigravity_session_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.ends_with(".jsonl")
        && !id.starts_with("ses_")
}

/// Resume the saved ACP session when possible, else start a fresh one.
///
/// `session/load` and `session/resume` answer with modes and model options.
/// They do not repeat the session id — the request already carried it.
/// Only `session/new` returns `sessionId`.
fn open_session(
    client: &RpcClient,
    resume: Option<&str>,
    cwd: &Path,
) -> Result<(String, bool), String> {
    let Some(id) = resume else {
        return new_session_id(client, cwd).map(|id| (id, false));
    };
    let params = json!({ "sessionId": id, "cwd": cwd, "mcpServers": [] });
    if client.request("session/load", params.clone()).is_ok() {
        return Ok((id.to_string(), true));
    }
    // Some ACP builds advertise resume instead of load.
    if client.request("session/resume", params).is_ok() {
        return Ok((id.to_string(), true));
    }
    new_session_id(client, cwd).map(|id| (id, false))
}

fn new_session_id(client: &RpcClient, cwd: &Path) -> Result<String, String> {
    let session = client.request("session/new", json!({ "cwd": cwd, "mcpServers": [] }))?;
    if let Some(id) = extract_session_id(&session) {
        return Ok(id);
    }
    Err(format!(
        "Antigravity did not return a session id for the new chat. Got: {}",
        brief_session_payload(&session)
    ))
}

fn extract_session_id(session: &Value) -> Option<String> {
    session
        .get("sessionId")
        .or_else(|| session.get("session_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn spawn_session_prompt(
    client: RpcClient,
    params: Value,
    slot: Arc<ParkingMutex<Option<Result<Value, String>>>>,
) {
    thread::spawn(move || {
        let result = client.request_timeout("session/prompt", params, Duration::from_secs(600));
        *slot.lock() = Some(result);
    });
}

fn brief_session_payload(session: &Value) -> String {
    let text = session.to_string();
    if text.chars().count() <= 240 {
        return text;
    }
    // Truncate on a char boundary: byte slicing at 240 panics when the
    // payload carries multi-byte text.
    let truncated: String = text.chars().take(240).collect();
    format!("{truncated}…")
}

fn supervise(
    mut child: Child,
    client: RpcClient,
    incoming: mpsc::Receiver<Incoming>,
    session_id: String,
    prompt_params: Value,
    shared: Arc<Shared>,
    control: mpsc::Receiver<Control>,
    on_event: Arc<dyn Fn(AntigravityEvent) + Send + Sync>,
) {
    // Send prompt on a side thread so the main loop can drain permissions.
    let prompt_done = Arc::new(ParkingMutex::new(None::<Result<Value, String>>));
    spawn_session_prompt(client.clone(), prompt_params, Arc::clone(&prompt_done));

    loop {
        while let Ok(msg) = control.try_recv() {
            match msg {
                Control::Interrupt => {
                    let _ = client.notify("session/cancel", json!({ "sessionId": session_id }));
                }
                Control::Steer { text, images } => {
                    let text = text.trim();
                    if !text.is_empty() || !images.is_empty() {
                        shared
                            .pending_steers
                            .lock()
                            .push((text.to_string(), images));
                    }
                }
                Control::Input {
                    id,
                    confirmed,
                    label,
                } => {
                    if let Some(pending) = shared.pending_permissions.lock().remove(&id) {
                        let option_id = if pending.question {
                            label
                                .as_deref()
                                .filter(|l| !matches!(*l, "Block" | "Deny"))
                                .and_then(|chosen| {
                                    pending.options.iter().find_map(|o| {
                                        let matches = ["name", "label", "optionId"].iter().any(|k| {
                                            o.get(*k)
                                                .and_then(Value::as_str)
                                                .is_some_and(|v| v.trim() == chosen)
                                        });
                                        matches
                                            .then(|| {
                                                o.get("optionId")
                                                    .and_then(Value::as_str)
                                                    .map(str::to_string)
                                            })
                                            .flatten()
                                    })
                                })
                                .or_else(|| {
                                    if confirmed {
                                        preferred_allow_option(&pending.options)
                                    } else {
                                        preferred_reject_option(&pending.options)
                                    }
                                })
                        } else if confirmed {
                            preferred_allow_option(&pending.options)
                        } else {
                            preferred_reject_option(&pending.options)
                        };
                        let result = match option_id {
                            Some(option_id) => json!({
                                "outcome": { "outcome": "selected", "optionId": option_id }
                            }),
                            None => json!({ "outcome": { "outcome": "cancelled" } }),
                        };
                        let _ = client.respond(&pending.rpc_id, result);
                        on_event(AntigravityEvent::InputResolved { request_id: id });
                    }
                }
            }
        }

        if shared.flush_allows.swap(false, Ordering::AcqRel) {
            allow_pending_tools(&client, &shared, &on_event);
        }

        match incoming.try_recv() {
            Ok(Incoming::Notification { method, params }) => {
                if method == "session/update" {
                    if let Some(update) = params.get("update") {
                        for event in map_update(update) {
                            on_event(event);
                        }
                    }
                }
            }
            Ok(Incoming::Request { id, method, params }) => {
                if method == "session/request_permission" {
                    handle_permission(&client, &shared, &on_event, id, params);
                } else {
                    // Decline unknown server→client requests.
                    let _ = client.respond(&id, json!({ "outcome": { "outcome": "cancelled" } }));
                }
            }
            Ok(Incoming::Eof) => break,
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        }

        if let Some(result) = prompt_done.lock().take() {
            // Same-session steer: the current step finishes, then the queued
            // user text + images are the next prompt. Do not emit Done — that would
            // paint "You stopped this reply" and idle the chat.
            let follow_up = if shared.interrupted.load(Ordering::Acquire) {
                shared.pending_steers.lock().clear();
                None
            } else {
                let queued = std::mem::take(&mut *shared.pending_steers.lock());
                if queued.is_empty() {
                    None
                } else {
                    let mut texts = Vec::new();
                    let mut images = Vec::new();
                    for (t, imgs) in queued {
                        if !t.is_empty() {
                            texts.push(t);
                        }
                        images.extend(imgs);
                    }
                    let mut prompt_blocks = Vec::new();
                    if !texts.is_empty() {
                        prompt_blocks.push(json!({ "type": "text", "text": texts.join("\n\n") }));
                    }
                    prompt_blocks.extend(images.into_iter().map(|image| {
                        json!({
                            "type": "image",
                            "data": image.data,
                            "mimeType": image.mime_type,
                        })
                    }));
                    (!prompt_blocks.is_empty()).then_some(prompt_blocks)
                }
            };
            if let (Ok(_), Some(prompt_blocks)) = (&result, follow_up) {
                spawn_session_prompt(
                    client.clone(),
                    json!({
                        "sessionId": session_id,
                        "prompt": prompt_blocks,
                    }),
                    Arc::clone(&prompt_done),
                );
                continue;
            }
            // The run is ending (finished, errored, or interrupted) and no
            // follow-up will spawn. Close the steer gate NOW — before Done —
            // so a steer submitted from here on is refused at the source
            // instead of being accepted into a loop that is about to exit.
            // Teardown's own store only covers the kill/wait window.
            shared.accepting.store(false, Ordering::Release);
            match result {
                Ok(value) => {
                    if let Some(event) = usage_from(&value) {
                        on_event(event);
                    }
                    let stop = value
                        .get("stopReason")
                        .and_then(Value::as_str)
                        .unwrap_or("end_turn");
                    let (status, error) = match stop {
                        "cancelled" => (AntigravityDoneStatus::Interrupted, None),
                        "refusal" => (AntigravityDoneStatus::Errored, Some("model refused".into())),
                        _ => (AntigravityDoneStatus::Completed, None),
                    };
                    on_event(AntigravityEvent::Done {
                        status,
                        error,
                        session_id: Some(session_id.clone()),
                    });
                }
                Err(err) => {
                    let status = if shared.interrupted.load(Ordering::Acquire) {
                        AntigravityDoneStatus::Interrupted
                    } else {
                        AntigravityDoneStatus::Errored
                    };
                    on_event(AntigravityEvent::Done {
                        status,
                        error: Some(err),
                        session_id: Some(session_id.clone()),
                    });
                }
            }
            break;
        }

        if shared.shutdown.load(Ordering::Acquire) || shared.interrupted.load(Ordering::Acquire) {
            let _ = client.notify("session/cancel", json!({ "sessionId": session_id }));
            on_event(AntigravityEvent::Done {
                status: AntigravityDoneStatus::Interrupted,
                error: None,
                session_id: Some(session_id.clone()),
            });
            break;
        }

        if let Ok(Some(_)) = child.try_wait() {
            on_event(AntigravityEvent::Done {
                status: AntigravityDoneStatus::Errored,
                error: Some("agy_acp_server exited".into()),
                session_id: Some(session_id.clone()),
            });
            break;
        }

        thread::park_timeout(Duration::from_millis(20));
    }

    let _ = child.kill();
    let _ = child.wait();
    shared.alive.store(false, Ordering::Release);
    shared.accepting.store(false, Ordering::Release);
}

/// Release tool prompts the current mode no longer wants confirmed.
/// Real questions stay on screen.
fn allow_pending_tools(
    client: &RpcClient,
    shared: &Shared,
    on_event: &Arc<dyn Fn(AntigravityEvent) + Send + Sync>,
) {
    let mode = shared.approval_mode.lock().clone();
    let plan_mode = shared.plan_mode.load(Ordering::Acquire);
    let pending: Vec<_> = shared.pending_permissions.lock().drain().collect();
    for (request_id, pending) in pending {
        if !pending.question && plan_mode && !crate::approval::is_read_only(&pending.tool) {
            let result = match preferred_reject_option(&pending.options) {
                Some(option_id) => json!({ "outcome": { "outcome": "selected", "optionId": option_id } }),
                None => json!({ "outcome": { "outcome": "cancelled" } }),
            };
            let _ = client.respond(&pending.rpc_id, result);
            on_event(AntigravityEvent::InputResolved { request_id });
            continue;
        }
        if pending.question || crate::approval::should_ask(&mode, &pending.tool, &pending.input) {
            shared
                .pending_permissions
                .lock()
                .insert(request_id, pending);
            continue;
        }
        let Some(option_id) = preferred_allow_option(&pending.options) else {
            shared
                .pending_permissions
                .lock()
                .insert(request_id, pending);
            continue;
        };
        let _ = client.respond(
            &pending.rpc_id,
            json!({ "outcome": { "outcome": "selected", "optionId": option_id } }),
        );
        on_event(AntigravityEvent::InputResolved { request_id });
    }
}

fn tool_input(tool_call: &Value, title: &str) -> String {
    let raw = tool_call.get("rawInput").cloned().unwrap_or(Value::Null);
    match raw {
        Value::String(text) if !text.is_empty() => text,
        Value::Object(map) => {
            for key in ["command", "cmd", "script"] {
                if let Some(Value::String(text)) = map.get(key) {
                    if !text.is_empty() {
                        return text.clone();
                    }
                }
            }
            if map.is_empty() {
                title.to_string()
            } else {
                Value::Object(map).to_string()
            }
        }
        _ => title.to_string(),
    }
}

fn handle_permission(
    client: &RpcClient,
    shared: &Shared,
    on_event: &Arc<dyn Fn(AntigravityEvent) + Send + Sync>,
    rpc_id: Value,
    params: Value,
) {
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
    let title = tool_call
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("permission")
        .to_string();
    let kind = tool_call
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    // Question-shaped interactions (interaction_*) need a UI answer — never auto.
    let tool_call_id = tool_call
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_question = tool_call_id.starts_with("interaction_");
    let input = tool_input(&tool_call, &title);
    let mode = shared.approval_mode.lock().clone();

    if !is_question
        && shared.plan_mode.load(Ordering::Acquire)
        && !crate::approval::is_read_only(kind)
    {
        let result = match preferred_reject_option(&options) {
            Some(option_id) => json!({ "outcome": { "outcome": "selected", "optionId": option_id } }),
            None => json!({ "outcome": { "outcome": "cancelled" } }),
        };
        let _ = client.respond(&rpc_id, result);
        return;
    }

    if !is_question && !crate::approval::should_ask(&mode, kind, &input) {
        if let Some(option_id) = preferred_allow_option(&options) {
            let _ = client.respond(
                &rpc_id,
                json!({ "outcome": { "outcome": "selected", "optionId": option_id } }),
            );
            return;
        }
    }

    let question_options = if is_question {
        options
            .iter()
            .filter_map(|o| {
                o.get("name")
                    .or_else(|| o.get("label"))
                    .or_else(|| o.get("optionId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
            .collect()
    } else {
        Vec::new()
    };

    let request_id = if tool_call_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        tool_call_id
    };
    shared.pending_permissions.lock().insert(
        request_id.clone(),
        PendingPermission {
            rpc_id,
            options,
            tool: kind.to_string(),
            input,
            question: is_question,
        },
    );
    on_event(AntigravityEvent::InputRequested {
        request_id,
        title: title.clone(),
        message: if is_question {
            title
        } else {
            format!("Allow `{kind}`: {title}?")
        },
        options: question_options,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The supervisor loop is not needed for the gate test: unpark on a
    /// finished thread is a no-op, and the receiver stays alive so `try_send`
    /// never sees a closed channel.
    #[test]
    fn steer_text_is_refused_once_the_gate_closes() {
        let (tx, rx) = mpsc::sync_channel(8);
        let worker = thread::Builder::new()
            .name("test-antigravity-gate".into())
            .spawn(|| {})
            .unwrap();
        let run = AntigravityRun {
            handle: Arc::new(Handle {
                shared: Arc::new(Shared::default()),
                control: tx,
                worker: worker.thread().clone(),
            }),
        };
        assert!(run.accepts_steers());
        // supervise stores this before Done when no follow-up will spawn.
        run.handle.shared.accepting.store(false, Ordering::Release);
        let error = run.steer_text("hello").unwrap_err();
        assert!(error.contains("no longer accepting"), "{error}");
        assert!(!run.accepts_steers());
        drop(rx);
    }
}

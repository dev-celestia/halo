//! OpenCode **v2** harness: spawn `opencode serve`, drive `/api/*` over HTTP + SSE.
//! Requires OpenCode ≥ 2.2.0.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use parking_lot::Mutex as ParkingMutex;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use uuid::Uuid;

use super::probe::{self, MIN_VERSION};
use super::sse::read_sse;
use super::types::{
    OpenCodeDoneStatus, OpenCodeEvent, OpenCodeProbeResult, OpenCodeRunOptions, model_ref,
};
use crate::PiImage;

const BASIC_USER: &str = "opencode";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
/// Silence that makes the supervisor ask the server whether it is still there.
/// Silence alone is not failure: a long tool run, a model call, or a question
/// waiting on the human all look identical on the wire.
const SSE_IDLE: Duration = Duration::from_secs(180);
/// Ceiling on *unexplained* silence — the server answers but the turn emits
/// nothing. Without it a wedged turn would keep the chat "Working" forever.
const SSE_SILENCE_LIMIT: Duration = Duration::from_secs(30 * 60);
/// The liveness probe must never block an interrupt behind a hung socket.
const LIVENESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

enum Control {
    Interrupt,
    Steer {
        id: String,
        text: String,
        images: Vec<PiImage>,
    },
    Input { id: String, confirmed: bool },
}

/// State the supervisor and the SSE reader share: lifecycle flags plus the
/// bookkeeping both sides need (pending approvals, child sessions, last frame).
pub(super) struct PendingPermission {
    pub session_id: String,
    pub action: String,
    pub detail: String,
}

pub(super) struct Shared {
    pub(super) shutdown: AtomicBool,
    pub(super) interrupted: AtomicBool,
    alive: AtomicBool,
    accepting: AtomicBool,
    /// Set when the SSE reader sees the root turn's terminal
    /// `session.execution.*` frame. A steer after this can never be
    /// delivered — the supervisor refuses instead of POSTing into a
    /// finished turn.
    pub(super) settled: AtomicBool,
    /// False when `/api/model` says the selected model cannot take images.
    images_supported: AtomicBool,
    /// Last frame the *turn* produced. Keep-alive pings and comments do not
    /// count — they prove the socket, not the work.
    pub(super) last_event: ParkingMutex<Instant>,
    session_id: ParkingMutex<Option<String>>,
    /// Composer approval mode. The SSE reader and the supervisor both use it.
    pub(super) approval_mode: ParkingMutex<String>,
    /// Read-only plan mode flag.
    pub(super) plan_mode: AtomicBool,
    /// Set when the composer changes mode so cards already on screen are re-checked.
    pub(super) flush_allows: AtomicBool,
    /// Permissions the policy already answered `(id, session_id, allow)`; the supervisor posts the reply.
    pub(super) auto_replies: ParkingMutex<Vec<(String, String, bool)>>,
    /// Permission request id → the session and the tool the card is for.
    pub(super) pending_permissions: ParkingMutex<HashMap<String, PendingPermission>>,
    /// Child sessions spawned by the root chat, mirrored for interrupts.
    pub(super) child_sessions: ParkingMutex<Vec<String>>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            shutdown: AtomicBool::new(false),
            interrupted: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            accepting: AtomicBool::new(true),
            settled: AtomicBool::new(false),
            images_supported: AtomicBool::new(true),
            last_event: ParkingMutex::new(Instant::now()),
            session_id: ParkingMutex::new(None),
            approval_mode: ParkingMutex::new("ask".into()),
            plan_mode: AtomicBool::new(false),
            flush_allows: AtomicBool::new(false),
            auto_replies: ParkingMutex::new(Vec::new()),
            pending_permissions: ParkingMutex::new(HashMap::new()),
            child_sessions: ParkingMutex::new(Vec::new()),
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

/// Live OpenCode v2 turn handle (mirrors [`crate::pi::PiRun`] surface).
#[derive(Clone)]
pub struct OpenCodeRun {
    handle: Arc<Handle>,
}

impl OpenCodeRun {
    /// Spawn `opencode serve`, open `/api/event`, create a session, send the prompt.
    pub fn spawn(
        options: OpenCodeRunOptions,
        on_event: impl Fn(OpenCodeEvent) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let executable = match probe::probe_environment(options.custom_executable.as_deref()) {
            OpenCodeProbeResult::Ready { path, .. } => path,
            OpenCodeProbeResult::WrongVersion {
                found,
                required,
                path,
            } => {
                return Err(format!(
                    "OpenCode {found} at {} is too old — Halo needs ≥ {required} (v2 API). Upgrade OpenCode in Halo Agent.",
                    path.display()
                ));
            }
            OpenCodeProbeResult::Missing => {
                return Err(
                    "OpenCode is not installed. Install OpenCode ≥ 2.2.0 from Halo Agent or https://opencode.ai."
                        .into(),
                );
            }
            OpenCodeProbeResult::StateBlocked { message, .. }
            | OpenCodeProbeResult::Failed { message } => return Err(message),
        };

        let cwd = options
            .cwd
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .ok_or_else(|| {
                "OpenCode working directory is missing or not a directory".to_string()
            })?;

        let password = Uuid::new_v4().simple().to_string();
        let mut child = ChildGuard(Some(spawn_serve(
            &executable,
            &cwd,
            &password,
            &options.extra_env,
        )?));
        let base = wait_for_listen_url(child.as_mut(), Instant::now() + STARTUP_TIMEOUT)?;
        soft_check_version(&base, &password)?;

        let client = http_client(&password)?;
        // Fail loudly instead of letting a text-only model silently ignore the
        // image. Older servers without capability metadata pass through.
        let image_support = if options.images.is_empty() {
            None
        } else {
            model_image_support(&client, &base, options.model.as_deref())
        };
        if let Some(capability) = image_support.as_ref().filter(|c| !c.supported) {
            let hint = capability.alternative.as_deref().map_or_else(
                || "Pick a vision-capable model in Coder Settings, then resend the message.".to_string(),
                |alternative| {
                    format!(
                        "Switch to {alternative} (or another vision-capable model) in Coder Settings, then resend."
                    )
                },
            );
            return Err(format!("{} can't read images. {hint}", capability.name));
        }
        // Subscribe before session create so early events are not missed.
        let sse = client
            .get(format!("{base}/api/event"))
            .header("Accept", "text/event-stream")
            .send()
            .map_err(|e| format!("opencode /api/event: {e}"))?;
        if !sse.status().is_success() {
            return Err(format!("opencode /api/event: HTTP {}", sse.status()));
        }

        let (session_id, resumed) = create_or_resume_session(
            &client,
            &base,
            &cwd,
            options.model.as_deref(),
            options.resume.as_deref(),
        )?;

        let shared = Arc::new(Shared::default());
        *shared.approval_mode.lock() =
            crate::approval::canonical_mode(&options.approval_mode).to_string();
        shared
            .plan_mode
            .store(options.plan_mode, Ordering::Release);
        shared.images_supported.store(
            image_support
                .as_ref()
                .is_none_or(|capability| capability.supported),
            Ordering::Release,
        );
        *shared.session_id.lock() = Some(session_id.clone());

        let on_event = Arc::new(on_event);
        on_event(OpenCodeEvent::SessionStarted {
            session_id: session_id.clone(),
            model: options.model.clone(),
            cwd: Some(cwd.display().to_string()),
        });
        on_event(OpenCodeEvent::CommandAccepted {
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
        let prompt_body = prompt_body(&effective_prompt, &options.skills, &options.images);
        post_json(
            &client,
            &base,
            &format!("/api/session/{session_id}/prompt"),
            Some(prompt_body),
        )?;

        let (tx, rx) = mpsc::sync_channel(16);
        let worker_shared = Arc::clone(&shared);
        let base_owned = base.clone();
        let session_for_worker = session_id.clone();
        let on_event_worker = Arc::clone(&on_event);
        let model_for_worker = options.model.clone();
        let owned_child = child.into_inner();
        let worker = thread::Builder::new()
            .name("opencode-v2".into())
            .spawn(move || {
                supervise(
                    owned_child,
                    client,
                    base_owned,
                    session_for_worker,
                    sse,
                    worker_shared,
                    rx,
                    on_event_worker,
                    model_for_worker,
                );
            })
            .map_err(|e| format!("Cannot start OpenCode supervisor: {e}"))?;

        Ok(Self {
            handle: Arc::new(Handle {
                shared,
                control: tx,
                worker: worker.thread().clone(),
            }),
        })
    }

    pub fn steer_text(&self, command_id: &str, prompt: &str) -> Result<(), String> {
        self.steer_with_images(command_id, prompt, Vec::new())
    }

    pub fn steer_with_images(
        &self,
        command_id: &str,
        prompt: &str,
        images: Vec<PiImage>,
    ) -> Result<(), String> {
        if !self.handle.shared.accepting.load(Ordering::Acquire) {
            return Err("OpenCode is no longer accepting commands".into());
        }
        if self.handle.shared.settled.load(Ordering::Acquire) {
            // The turn's terminal frame already arrived; the POST below would
            // 404 or start an orphan turn the run would never report.
            return Err("OpenCode turn already finished; steering was not sent".into());
        }
        if !images.is_empty() && !self.handle.shared.images_supported.load(Ordering::Acquire) {
            return Err(
                "The selected model can't read images. Pick a vision-capable model in Coder Settings."
                    .into(),
            );
        }
        self.handle
            .control
            .try_send(Control::Steer {
                id: command_id.to_string(),
                text: prompt.to_string(),
                images,
            })
            .map_err(|_| "OpenCode command queue is full or closed")?;
        self.handle.worker.unpark();
        Ok(())
    }

    /// Whether a steer could still reach this turn. The queue-first flow
    /// consults this to leave follow-ups Pending instead of dispatching into
    /// a turn that just ended.
    pub fn accepts_steers(&self) -> bool {
        self.handle.shared.accepting.load(Ordering::Acquire)
            && !self.handle.shared.settled.load(Ordering::Acquire)
            && !self.handle.shared.interrupted.load(Ordering::Acquire)
    }

    pub fn set_approval_mode(&self, mode: &str) {
        *self.handle.shared.approval_mode.lock() =
            crate::approval::canonical_mode(mode).to_string();
        self.handle
            .shared
            .flush_allows
            .store(true, Ordering::Release);
        self.handle.worker.unpark();
    }

    pub fn respond_input(&self, request_id: &str, confirmed: bool) -> Result<(), String> {
        if !self.handle.shared.accepting.load(Ordering::Acquire)
            || self.handle.shared.interrupted.load(Ordering::Acquire)
        {
            return Err("OpenCode approval is no longer active".into());
        }
        self.handle
            .control
            .try_send(Control::Input {
                id: request_id.into(),
                confirmed,
            })
            .map_err(|_| "OpenCode approval queue is full or closed")?;
        self.handle.worker.unpark();
        Ok(())
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

    pub fn session_id(&self) -> Option<String> {
        self.handle.shared.session_id.lock().clone()
    }
}

pub(super) fn spawn_serve(
    executable: &Path,
    cwd: &Path,
    password: &str,
    extra_env: &HashMap<String, String>,
) -> Result<Child, String> {
    let mut cmd = Command::new(executable);
    cmd.current_dir(cwd)
        .args(["serve", "--port", "0", "--hostname", "127.0.0.1"])
        .env("OPENCODE_PASSWORD", password)
        .env("OPENCODE_SERVER_PASSWORD", password)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|e| format!("failed to spawn opencode serve: {e}"))
}

pub(super) fn wait_for_listen_url(child: &mut Child, deadline: Instant) -> Result<String, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "opencode serve has no stdout".to_string())?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            tracing::debug!(target: "opencode", "{line}");
            if let Some(idx) = line.find("http://") {
                let url = line[idx..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches('/')
                    .to_string();
                if !url.is_empty() {
                    let _ = tx.send(url);
                }
            }
        }
    });
    if let Some(stderr) = child.stderr.take() {
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                tracing::debug!(target: "opencode.stderr", "{line}");
            }
        });
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    rx.recv_timeout(remaining)
        .map_err(|_| "opencode serve did not announce a listening address".into())
}

fn soft_check_version(base: &str, password: &str) -> Result<(), String> {
    let client = http_client(password)?;
    let Ok(resp) = client.get(format!("{base}/api/info")).send() else {
        return Ok(());
    };
    if !resp.status().is_success() {
        return Ok(());
    }
    let Ok(value) = resp.json::<Value>() else {
        return Ok(());
    };
    let ver = value
        .get("version")
        .or_else(|| value.pointer("/data/version"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if let Some(parsed) = probe::parse_semver(ver) {
        if parsed < MIN_VERSION {
            return Err(format!(
                "opencode serve reported {ver}; Halo requires ≥ {}.{}.{}",
                MIN_VERSION.0, MIN_VERSION.1, MIN_VERSION.2
            ));
        }
    }
    Ok(())
}

pub(super) fn http_client(password: &str) -> Result<Client, String> {
    let token =
        base64::engine::general_purpose::STANDARD.encode(format!("{BASIC_USER}:{password}"));
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Basic {token}"))
            .map_err(|e| format!("auth header: {e}"))?,
    );
    Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("http client: {e}"))
}

struct ModelImages {
    name: String,
    supported: bool,
    /// First vision-capable model in the same catalog, for the error hint.
    alternative: Option<String>,
}

/// Capability row for the model this run will use.
///
/// `None` when the server cannot describe the model (missing capability data
/// or an unknown id): callers should let the prompt through rather than guess.
fn model_image_support(client: &Client, base: &str, model: Option<&str>) -> Option<ModelImages> {
    let (info, alternative) = match model.and_then(model_ref) {
        Some(reference) => {
            let provider = reference.get("providerID")?.as_str()?;
            let id = reference.get("id")?.as_str()?;
            let listing = get_json(client, base, "/api/model").ok()?;
            let list = listing.get("data")?.as_array()?;
            let info = list.iter().find(|entry| {
                entry.get("providerID").and_then(Value::as_str) == Some(provider)
                    && entry
                        .get("modelID")
                        .or_else(|| entry.get("id"))
                        .and_then(Value::as_str)
                        == Some(id)
            })?;
            let alternative = vision_model_name(list, Some(id));
            (info.clone(), alternative)
        }
        None => {
            let value = get_json(client, base, "/api/model/default").ok()?;
            let data = value.get("data")?;
            if data.is_null() {
                return None;
            }
            (data.clone(), None)
        }
    };
    let name = info
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("This model")
        .to_string();
    let inputs = info.pointer("/capabilities/input")?.as_array()?;
    Some(ModelImages {
        name,
        supported: inputs.iter().any(|kind| kind.as_str() == Some("image")),
        alternative,
    })
}

fn vision_model_name(models: &[Value], exclude: Option<&str>) -> Option<String> {
    models
        .iter()
        .filter(|entry| {
            entry.get("enabled").and_then(Value::as_bool) != Some(false)
                && entry
                    .get("modelID")
                    .or_else(|| entry.get("id"))
                    .and_then(Value::as_str)
                    != exclude
        })
        .find(|entry| {
            entry
                .pointer("/capabilities/input")
                .and_then(Value::as_array)
                .is_some_and(|inputs| inputs.iter().any(|kind| kind.as_str() == Some("image")))
        })
        .and_then(|entry| entry.get("name").and_then(Value::as_str))
        .map(str::to_string)
}

fn is_valid_opencode_session_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.ends_with(".jsonl")
}

/// Resume `resume` when the OpenCode server still holds that session; otherwise
/// create a fresh session and report `resumed = false` so caller can attach
/// prior conversation context.
fn create_or_resume_session(
    client: &Client,
    base: &str,
    cwd: &Path,
    model: Option<&str>,
    resume: Option<&str>,
) -> Result<(String, bool), String> {
    if let Some(id) = resume
        .map(str::trim)
        .filter(|s| is_valid_opencode_session_id(s))
    {
        match get_json(client, base, &format!("/api/session/{id}")) {
            Ok(_) => {
                if let Some(reference) = model.and_then(model_ref) {
                    let _ = post_json(
                        client,
                        base,
                        &format!("/api/session/{id}/model"),
                        Some(json!({ "model": reference })),
                    );
                }
                return Ok((id.to_string(), true));
            }
            Err(err) => {
                tracing::debug!(
                    target: "opencode",
                    "stored session {id} is unavailable ({err}); creating a fresh session"
                );
            }
        }
    }

    let mut body = json!({
        "title": "halo",
        "location": { "directory": cwd.to_string_lossy() },
    });
    if let Some(reference) = model.and_then(model_ref) {
        body["model"] = reference;
    }
    let value = post_json(client, base, "/api/session", Some(body))?;
    let id = value
        .pointer("/data/id")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("session create missing id: {value}"))?;
    Ok((id, false))
}

/// Prompt body with the attached skill ids and images. An empty list attaches
/// none; OpenCode still exposes every discovered skill to the agent.
fn prompt_body(text: &str, skills: &[String], images: &[PiImage]) -> Value {
    let skills: Vec<Value> = skills.iter().map(|id| json!({ "id": id })).collect();
    json!({
        "text": text,
        "files": image_files(images),
        "skills": skills,
    })
}

/// v2 prompt files are URIs; inline data URIs are decoded and stored as image
/// parts by the server (verified against `PromptInput.FileAttachment`).
fn image_files(images: &[PiImage]) -> Vec<Value> {
    images
        .iter()
        .map(|image| {
            json!({
                "uri": format!("data:{};base64,{}", image.mime_type, image.data),
            })
        })
        .collect()
}

fn supervise(
    mut child: Child,
    client: Client,
    base: String,
    session_id: String,
    sse: reqwest::blocking::Response,
    shared: Arc<Shared>,
    control: mpsc::Receiver<Control>,
    on_event: Arc<dyn Fn(OpenCodeEvent) + Send + Sync>,
    model: Option<String>,
) {
    let done = Arc::new(AtomicBool::new(false));
    let done_flag = Arc::clone(&done);
    let shared_reader = Arc::clone(&shared);
    let session_for_reader = session_id.clone();
    let on_event_reader = Arc::clone(&on_event);
    let reader = thread::Builder::new()
        .name("opencode-sse".into())
        .spawn(move || {
            read_sse(
                sse,
                &session_for_reader,
                &shared_reader,
                model,
                on_event_reader.as_ref(),
            );
            done_flag.store(true, Ordering::Release);
        })
        .ok();

    let mut emitted_done = false;
    // When to ask the server whether silence means "gone" or "still working".
    let mut probe_at = Instant::now() + SSE_IDLE;
    let mut probe_failures = 0u32;
    loop {
        while let Ok(msg) = control.try_recv() {
            match msg {
                Control::Interrupt => {
                    interrupt_tree(&client, &base, &session_id, &shared);
                }
                Control::Steer { id, text, images } => {
                    let body = json!({
                        "text": text,
                        "files": image_files(&images),
                        "skills": [],
                        "delivery": "steer",
                    });
                    // Correlated like Pi: the POST result decides the ack, so
                    // a refused steer reaches the bridge as CommandRejected
                    // and the prompt returns to the queue instead of
                    // vanishing between the gate check and the send.
                    match post_json(
                        &client,
                        &base,
                        &format!("/api/session/{session_id}/prompt"),
                        Some(body),
                    ) {
                        Ok(_) => on_event(OpenCodeEvent::CommandAccepted { command_id: id }),
                        Err(error) => on_event(OpenCodeEvent::CommandRejected {
                            command_id: id,
                            error,
                        }),
                    }
                }
                Control::Input { id, confirmed } => {
                    let sid = shared
                        .pending_permissions
                        .lock()
                        .remove(&id)
                        .map(|pending| pending.session_id)
                        .unwrap_or_else(|| session_id.clone());
                    reply_permission(&client, &base, &sid, &id, confirmed);
                    on_event(OpenCodeEvent::InputResolved { request_id: id });
                }
            }
        }

        let replies: Vec<_> = shared.auto_replies.lock().drain(..).collect();
        for (id, sid, allow) in replies {
            reply_permission(&client, &base, &sid, &id, allow);
            on_event(OpenCodeEvent::InputResolved { request_id: id });
        }
        if shared.flush_allows.swap(false, Ordering::AcqRel) {
            let mode = shared.approval_mode.lock().clone();
            let plan_mode = shared.plan_mode.load(Ordering::Acquire);
            let pending: Vec<_> = shared.pending_permissions.lock().drain().collect();
            for (id, perm) in pending {
                if plan_mode && !crate::approval::is_read_only(&perm.action) {
                    reply_permission(&client, &base, &perm.session_id, &id, false);
                    on_event(OpenCodeEvent::InputResolved { request_id: id });
                    continue;
                }
                if crate::approval::should_ask(&mode, &perm.action, &perm.detail) {
                    shared.pending_permissions.lock().insert(id, perm);
                    continue;
                }
                reply_permission(&client, &base, &perm.session_id, &id, true);
                on_event(OpenCodeEvent::InputResolved { request_id: id });
            }
        }

        if shared.shutdown.load(Ordering::Acquire)
            || shared.interrupted.load(Ordering::Acquire)
            || done.load(Ordering::Acquire)
        {
            if shared.interrupted.load(Ordering::Acquire) {
                interrupt_tree(&client, &base, &session_id, &shared);
            }
            break;
        }

        if let Ok(Some(status)) = child.try_wait() {
            if !done.load(Ordering::Acquire) && !emitted_done {
                let code = status.code();
                on_event(OpenCodeEvent::Done {
                    status: OpenCodeDoneStatus::Errored,
                    error: code
                        .filter(|c| *c != 0)
                        .map(|c| format!("opencode serve exited with code {c}")),
                    session_id: Some(session_id.clone()),
                });
                emitted_done = true;
            }
            break;
        }

        // Silence watchdog. The reader thread cannot police this itself: a
        // blocked `read_line` never returns once the socket goes quiet, so the
        // turn would wait forever instead of failing. Only an unreachable
        // server proves the turn is gone — a silent but answering server is
        // just work in progress.
        let now = Instant::now();
        if now >= probe_at {
            let waiting_on_human = !shared.pending_permissions.lock().is_empty();
            if waiting_on_human {
                // A permission prompt can sit open for as long as the user
                // takes; that is not a stalled turn.
                *shared.last_event.lock() = now;
                probe_failures = 0;
                probe_at = now + SSE_IDLE;
            } else {
                let silent = shared.last_event.lock().elapsed();
                if silent < SSE_IDLE {
                    probe_failures = 0;
                    probe_at = now + SSE_IDLE;
                } else if server_is_alive(&client, &base) {
                    probe_failures = 0;
                    if silent >= SSE_SILENCE_LIMIT {
                        on_event(OpenCodeEvent::Done {
                            status: OpenCodeDoneStatus::Errored,
                            error: Some(format!(
                                "OpenCode sent no events for {} while its server stayed up — the turn is stuck. Stop it and resend.",
                                human_span(silent)
                            )),
                            session_id: Some(session_id.clone()),
                        });
                        emitted_done = true;
                        break;
                    }
                    // Re-arm only the *probe*: the silence span keeps growing so
                    // a wedged turn still hits `SSE_SILENCE_LIMIT`.
                    tracing::debug!(
                        target: "opencode",
                        "no events for {:?}; server still up, still waiting",
                        silent
                    );
                    probe_at = now + SSE_IDLE;
                } else {
                    // One failed probe can be a blip (a busy loop, a wake from
                    // sleep); two in a row is a dead server.
                    probe_failures += 1;
                    if probe_failures >= 2 {
                        on_event(OpenCodeEvent::Done {
                            status: OpenCodeDoneStatus::Errored,
                            error: Some(format!(
                                "OpenCode stopped responding — no events for {} and its server is unreachable. Resend to start a fresh turn.",
                                human_span(silent)
                            )),
                            session_id: Some(session_id.clone()),
                        });
                        emitted_done = true;
                        break;
                    }
                    probe_at = now + Duration::from_secs(2);
                }
            }
        }

        thread::park_timeout(Duration::from_millis(50));
    }

    let _ = child.kill();
    let _ = child.wait();
    if let Some(t) = reader {
        let _ = t.join();
    }
    // If SSE ended without a Done (e.g. interrupt killed the stream), emit one.
    if !emitted_done && !done.load(Ordering::Acquire) && shared.interrupted.load(Ordering::Acquire)
    {
        on_event(OpenCodeEvent::Done {
            status: OpenCodeDoneStatus::Interrupted,
            error: None,
            session_id: Some(session_id),
        });
    }
    shared.alive.store(false, Ordering::Release);
    shared.accepting.store(false, Ordering::Release);
}

/// Cheap "is the server still there" probe for the silence watchdog.
///
/// The SSE stream stays open while a tool or model call runs, so an empty
/// stream says nothing on its own. Only an unreachable HTTP API proves the
/// turn is gone; a slow answer proves the opposite.
fn server_is_alive(client: &Client, base: &str) -> bool {
    client
        .get(format!("{base}/api/info"))
        .timeout(LIVENESS_PROBE_TIMEOUT)
        .send()
        .map(|resp| resp.status().is_success())
        .unwrap_or(false)
}

/// "3m" / "45s" — a silence span a person can read in an error line.
fn human_span(span: Duration) -> String {
    let secs = span.as_secs();
    if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

/// Interrupt the root session and every child it spawned.
fn interrupt_tree(client: &Client, base: &str, session_id: &str, shared: &Shared) {
    let _ = post_json(
        client,
        base,
        &format!("/api/session/{session_id}/interrupt"),
        None,
    );
    let children = shared.child_sessions.lock().clone();
    for child in children {
        let _ = post_json(
            client,
            base,
            &format!("/api/session/{child}/interrupt"),
            None,
        );
    }
}

pub(super) fn get_json(client: &Client, base: &str, path: &str) -> Result<Value, String> {
    let resp = client
        .get(format!("{base}{path}"))
        .send()
        .map_err(|e| format!("GET {path}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GET {path}: HTTP {}", resp.status()));
    }
    resp.json().map_err(|e| format!("GET {path} decode: {e}"))
}

fn reply_permission(client: &Client, base: &str, sid: &str, id: &str, confirmed: bool) {
    let decision = if confirmed { "once" } else { "reject" };
    let body = json!({ "decision": decision, "message": Value::Null });
    let _ = post_json(
        client,
        base,
        &format!("/api/session/{sid}/permission/{id}/reply"),
        Some(body),
    );
}

fn post_json(
    client: &Client,
    base: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let req = client.post(format!("{base}{path}"));
    let resp = match body {
        Some(b) => req
            .header(CONTENT_TYPE, "application/json")
            .json(&b)
            .send()
            .map_err(|e| format!("POST {path}: {e}"))?,
        None => req.send().map_err(|e| format!("POST {path}: {e}"))?,
    };
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        return Err(format!("POST {path}: HTTP {status}: {text}"));
    }
    match resp.json::<Value>() {
        Ok(v) => Ok(v),
        Err(_) => Ok(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_body_attaches_skill_ids() {
        let body = prompt_body("hello", &["demo".to_string()], &[]);
        assert_eq!(body["text"], "hello");
        assert_eq!(body["files"], json!([]));
        assert_eq!(body["skills"], json!([{ "id": "demo" }]));
    }

    #[test]
    fn prompt_body_without_skills_sends_empty_list() {
        let body = prompt_body("hello", &[], &[]);
        assert_eq!(body["skills"], json!([]));
    }

    #[test]
    fn prompt_body_sends_images_as_inline_data_uris() {
        let images = vec![PiImage {
            data: "iVBORw0KGgo=".into(),
            mime_type: "image/png".into(),
        }];
        let body = prompt_body("look", &[], &images);
        assert_eq!(
            body["files"],
            json!([{ "uri": "data:image/png;base64,iVBORw0KGgo=" }])
        );
    }

    #[test]
    fn vision_hint_skips_text_only_and_selected_models() {
        let models = vec![
            json!({ "modelID": "beam-free", "name": "Beam Free",
                    "capabilities": { "input": ["text"] } }),
            json!({ "modelID": "nemotron-vl", "name": "Nemotron VL",
                    "capabilities": { "input": ["text", "image"] } }),
            json!({ "modelID": "glm", "name": "GLM",
                    "capabilities": { "input": ["text", "image"] } }),
        ];
        assert_eq!(
            vision_model_name(&models, Some("beam-free")).as_deref(),
            Some("Nemotron VL")
        );
        assert_eq!(vision_model_name(&models[..1], Some("beam-free")), None);
    }

    #[test]
    fn silence_spans_read_as_minutes() {
        assert_eq!(human_span(Duration::from_secs(45)), "45s");
        assert_eq!(human_span(Duration::from_secs(200)), "3m");
        // The hard limit must stay well above the soft one, or a live server
        // would be failed before it ever gets probed.
        assert!(SSE_SILENCE_LIMIT > SSE_IDLE);
    }

    /// The supervisor loop is not needed for the gate tests: unpark on a
    /// finished thread is a no-op, and the receiver is kept alive so
    /// `try_send` never sees a closed channel.
    fn gated_run(shared: Shared) -> (OpenCodeRun, mpsc::Receiver<Control>) {
        let (tx, rx) = mpsc::sync_channel(8);
        let worker = thread::Builder::new()
            .name("test-opencode-gate".into())
            .spawn(|| {})
            .unwrap();
        (
            OpenCodeRun {
                handle: Arc::new(Handle {
                    shared: Arc::new(shared),
                    control: tx,
                    worker: worker.thread().clone(),
                }),
            },
            rx,
        )
    }

    /// After the turn's terminal frame a steer can never be delivered — the
    /// gate must refuse it instead of letting it vanish between the check
    /// and the POST.
    #[test]
    fn steer_is_refused_after_the_turn_settles() {
        let shared = Shared::default();
        shared.settled.store(true, Ordering::Release);
        let (run, _rx) = gated_run(shared);
        let error = run.steer_text("cmd-1", "hello").unwrap_err();
        assert!(error.contains("already finished"), "{error}");
    }

    #[test]
    fn steer_is_accepted_while_the_turn_is_live() {
        let (run, rx) = gated_run(Shared::default());
        run.steer_text("cmd-1", "hello")
            .expect("a live turn takes a steer");
        match rx.try_recv().unwrap() {
            Control::Steer { id, text, images } => {
                assert_eq!(id, "cmd-1");
                assert_eq!(text, "hello");
                assert!(images.is_empty());
            }
            _ => panic!("expected a steer control"),
        }
    }

    #[test]
    fn accepts_steers_tracks_settle_and_interrupt() {
        let (run, _rx) = gated_run(Shared::default());
        assert!(run.accepts_steers());
        run.request_interrupt();
        assert!(!run.accepts_steers());
    }
}

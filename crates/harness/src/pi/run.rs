//! Cloneable, nonblocking run handles. All child I/O and reaping live on a worker.
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use uuid::Uuid;

use super::probe::Launch;
use super::process::{self, Transport};
use super::protocol::{self, MAX_PENDING};
use super::supervisor::{Config, supervise};
use super::types::{PiEvent, PiImage, PiRunOptions, REQUIRED_VERSION};

pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) enum Control {
    Steer {
        id: String,
        bytes: Vec<u8>,
    },
    Input {
        id: String,
        confirmed: bool,
        deadline: Instant,
    },
}

#[derive(Default)]
pub(crate) struct Commands {
    pub seen: HashSet<String>,
    pub pending: HashSet<String>,
}

pub(crate) struct Shared {
    pub alive: AtomicBool,
    pub accepting: AtomicBool,
    pub interrupted: AtomicBool,
    pub shutdown: AtomicBool,
    pub session: Mutex<Option<String>>,
    pub approvals: Mutex<HashMap<String, Instant>>,
    pub commands: Mutex<Commands>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            alive: AtomicBool::new(true),
            accepting: AtomicBool::new(false),
            interrupted: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            session: Mutex::new(None),
            approvals: Mutex::new(HashMap::new()),
            commands: Mutex::new(Commands::default()),
        }
    }
}

// Bounded stdout/event mailbox (8 batches, each derived from one <=4 MiB frame).
// The reserved terminal slot guarantees Done even if the callback stalls. Lifecycle
// cleanup does not depend on the callback thread making progress.
#[derive(Default)]
struct Events {
    queue: VecDeque<Vec<PiEvent>>,
    terminal: Option<Vec<PiEvent>>,
}

#[derive(Default)]
pub(crate) struct Mailbox {
    events: Mutex<Events>,
    wake: Condvar,
}

impl Mailbox {
    pub fn try_push(&self, batch: &mut Vec<PiEvent>) -> bool {
        if batch.is_empty() {
            return true;
        }
        let mut events = self.events.lock();
        if events.queue.len() >= 8 {
            return false;
        }
        events.queue.push_back(std::mem::take(batch));
        self.wake.notify_one();
        true
    }

    pub fn finish(&self, batch: Vec<PiEvent>) {
        let mut events = self.events.lock();
        events.terminal = Some(batch);
        self.wake.notify_one();
    }

    fn dispatch(&self, shared: &Shared, callback: impl Fn(PiEvent)) {
        loop {
            let (batch, terminal) = {
                let mut events = self.events.lock();
                loop {
                    if let Some(batch) = events.queue.pop_front() {
                        break (batch, false);
                    }
                    if let Some(batch) = events.terminal.take() {
                        break (batch, true);
                    }
                    self.wake.wait(&mut events);
                }
            };
            for event in batch {
                // No shared lock is held across user callbacks, including reentrant ones.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(event)))
                    .is_err()
                {
                    shared.shutdown.store(true, Ordering::Release);
                    return;
                }
            }
            if terminal {
                return;
            }
        }
    }
}

struct Handle {
    shared: Arc<Shared>,
    control: mpsc::SyncSender<Control>,
    worker: thread::Thread,
    /// Policy file the approval hook re-reads. Absent in plan mode.
    policy_path: Option<std::path::PathBuf>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Only the final public handle requests shutdown, without waiting or taking a child lock.
        self.shared.shutdown.store(true, Ordering::Release);
        self.worker.unpark();
    }
}

#[derive(Clone)]
pub struct PiRun {
    handle: Arc<Handle>,
}

fn write_policy(path: &std::path::Path, mode: &str) -> Result<(), String> {
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o600);
    }
    open.open(path)
        .and_then(|mut file| file.write_all(mode.as_bytes()))
        .map_err(|_| "Cannot write Pi approval policy".to_string())
}

impl PiRun {
    /// Call on a worker: performs a bounded version probe and startup handshake, never a model probe.
    pub fn spawn(
        mut options: PiRunOptions,
        on_event: impl Fn(PiEvent) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let initial = protocol::command(
            &options.run_id,
            "prompt",
            &options.prompt,
            std::mem::take(&mut options.images),
        )?;
        let launch = Launch::resolve(
            options.custom_executable.as_deref(),
            options.extra_env.get("PATH").map(String::as_str),
        )?;
        let version = process::version(&mut launch.options_command(&options))?;
        if version != REQUIRED_VERSION {
            return Err(format!(
                "This integration requires @earendil-works/pi-coding-agent {REQUIRED_VERSION} (correlated prompt dispositions and agent_settled). Configure that executable; no prompt was sent."
            ));
        }
        let mut cmd = launch.options_command(&options);
        cmd.args(["--mode", "rpc"]);
        if let Some(resume) = &options.resume {
            if resume.is_empty() {
                return Err("Pi resume reference cannot be empty".into());
            }
            if std::path::Path::new(resume).is_absolute() {
                cmd.arg("--session").arg(resume);
            }
        }
        // Never pass `--no-extensions` (`-ne`): Pi serves *every* model provider
        // through extension packages (`pi-antigravity`, `pi-opencode`, …), so
        // disabling extensions leaves `get_available_models` empty and turns
        // every prompt into "No API key found for the selected model". Tool
        // safety comes from the `--tools` allowlist below, which is the flag
        // that actually decides what the agent may run.
        let mut extension = None;
        let mut approval_nonce = None;
        let mut policy_path = None;
        if options.plan_mode {
            cmd.args(["--tools", "read,grep,find,ls", "--no-mcp"]);
        } else {
            // Every mode, including Full access, loads the hook. The policy
            // file is rewritten when the composer chip changes mid-run.
            let dir = tempfile::Builder::new()
                .prefix("halo-pi-approval-")
                .tempdir()
                .map_err(|_| "Cannot create private Pi approval directory")?;
            let nonce = Uuid::new_v4().to_string();
            let path = dir.path().join("approval.js");
            let policy_file = dir.path().join("policy");
            let mut open = std::fs::OpenOptions::new();
            open.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
                std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
                    .map_err(|_| "Cannot protect Pi approval directory")?;
                open.mode(0o600);
            }
            let policy = crate::approval::canonical_mode(&options.approval_mode);
            write_policy(&policy_file, policy)?;
            let source = include_str!("approval.mjs")
                .replace("__HALO_APPROVAL_NONCE__", &nonce)
                .replace("__HALO_APPROVAL_POLICY__", policy)
                .replace("__HALO_APPROVAL_POLICY_PATH__", &policy_file.display().to_string());
            open.open(&path)
                .and_then(|mut f| f.write_all(source.as_bytes()))
                .map_err(|_| "Cannot materialize Pi approval hook")?;
            cmd.args([
                "--tools",
                "read,grep,find,ls,write,edit,bash",
                "--no-mcp",
                "-e",
            ])
            .arg(path);
            approval_nonce = Some(nonce);
            policy_path = Some(policy_file);
            extension = Some(dir);
        }
        // --approve is project trust, not tool approval. Never add it in any mode.
        // Only advertise the browser when the CLI is installed: the prompt must
        // never promise a tool the agent cannot run. The CLI carries the
        // stale-ref guard, so the agent is pointed at it rather than at
        // agent-browser directly.
        if let Some(instructions) = crate::browser::instructions() {
            cmd.arg("--append-system-prompt").arg(instructions);
        }
        // Only when Settings turned computer use on. The env is the gate, so an
        // off switch never mentions `halo-desktop`.
        if options.extra_env.contains_key("HALO_DESKTOP_CHAT") {
            cmd.arg("--append-system-prompt")
                .arg(crate::desktop::instructions());
        }
        let transport = Transport::spawn(&mut cmd)?;
        let shared = Arc::new(Shared::default());
        let mailbox = Arc::new(Mailbox::default());
        let (tx, rx) = mpsc::sync_channel(8);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker_shared = shared.clone();
        let worker_mailbox = mailbox.clone();
        let config = Config {
            run_id: options.run_id,
            cwd: options.cwd,
            model: options.model,
            initial,
            approval_nonce,
            extension,
        };
        let worker = thread::Builder::new()
            .name("pi-supervisor".into())
            .spawn(move || {
                supervise(
                    transport,
                    config,
                    worker_shared,
                    worker_mailbox,
                    rx,
                    ready_tx,
                );
            })
            .map_err(|_| "Cannot start Pi supervisor")?;
        let dispatch_shared = shared.clone();
        let dispatch_mailbox = mailbox.clone();
        if thread::Builder::new()
            .name("pi-events".into())
            .spawn(move || dispatch_mailbox.dispatch(&dispatch_shared, on_event))
            .is_err()
        {
            shared.shutdown.store(true, Ordering::Release);
            worker.thread().unpark();
            return Err("Cannot start Pi event dispatcher".into());
        }
        let run = Self {
            handle: Arc::new(Handle {
                shared,
                control: tx,
                worker: worker.thread().clone(),
                policy_path,
            }),
        };
        match ready_rx.recv_timeout(STARTUP_TIMEOUT + Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(run),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("Pi startup handshake timed out; no prompt will be replayed".into()),
        }
    }

    pub fn steer_with_images(
        &self,
        command_id: &str,
        prompt: &str,
        images: Vec<PiImage>,
    ) -> Result<(), String> {
        if !self.handle.shared.accepting.load(Ordering::Acquire) {
            return Err("Pi is no longer accepting commands".into());
        }
        let bytes = protocol::command(command_id, "steer", prompt, images)?;
        let mut commands = self.handle.shared.commands.lock();
        if commands.seen.contains(command_id) {
            return Err(
                "Pi command ID has already been used; delivery must not be replayed".into(),
            );
        }
        if commands.pending.len() >= MAX_PENDING || commands.seen.len() >= 1024 {
            return Err("Pi command limit reached".into());
        }
        self.handle
            .control
            .try_send(Control::Steer {
                id: command_id.into(),
                bytes,
            })
            .map_err(|_| "Pi command queue is full or closed; command was not sent")?;
        commands.seen.insert(command_id.into());
        commands.pending.insert(command_id.into());
        drop(commands);
        self.handle.worker.unpark();
        Ok(())
    }

    pub fn steer(&self, prompt: &str) -> Result<(), String> {
        self.steer_with_images(&Uuid::new_v4().to_string(), prompt, Vec::new())
    }

    pub fn respond_input(&self, request_id: &str, confirmed: bool) -> Result<(), String> {
        let shared = &self.handle.shared;
        if !shared.accepting.load(Ordering::Acquire) || shared.interrupted.load(Ordering::Acquire) {
            return Err("Pi approval is no longer active".into());
        }
        let mut approvals = shared.approvals.lock();
        let deadline = *approvals
            .get(request_id)
            .ok_or("Unknown, stale or already answered Pi approval request")?;
        if Instant::now() >= deadline {
            return Err("Pi approval request has expired".into());
        }
        self.handle
            .control
            .try_send(Control::Input {
                id: request_id.into(),
                confirmed,
                deadline,
            })
            .map_err(|_| "Pi approval queue is full or closed; answer was not sent")?;
        approvals.remove(request_id);
        drop(approvals);
        self.handle.worker.unpark();
        Ok(())
    }

    /// Rewrite the policy file the running hook reads on the next tool call.
    pub fn set_approval_mode(&self, mode: &str) {
        let Some(path) = &self.handle.policy_path else {
            return;
        };
        let _ = write_policy(path, crate::approval::canonical_mode(mode));
    }

    pub fn request_interrupt(&self) {
        self.handle.shared.accepting.store(false, Ordering::Release);
        self.handle
            .shared
            .interrupted
            .store(true, Ordering::Release);
        self.handle.worker.unpark();
    }

    pub fn is_alive(&self) -> bool {
        self.handle.shared.alive.load(Ordering::Acquire)
    }

    /// Whether a steer could still reach this turn. The supervisor refuses
    /// again after settle (with a correlated rejection); this gate keeps the
    /// queue-first flow from dispatching follow-ups into a dying run.
    pub fn accepts_steers(&self) -> bool {
        let shared = &self.handle.shared;
        shared.accepting.load(Ordering::Acquire) && !shared.interrupted.load(Ordering::Acquire)
    }

    /// Session file path from get_state, suitable for a subsequent --session argument.
    pub fn session_id(&self) -> Option<String> {
        self.handle.shared.session.lock().clone()
    }
}

//! Agent session notification sounds — the herdr approach (state-transition
//! chimes played through the platform's own audio CLI, zero Rust audio deps):
//!
//! - embedded completion, input-request, and attention chimes;
//! - writes to a temp file and uses the system player on a background thread:
//!   `afplay` (macOS), PowerShell `Media.SoundPlayer` (Windows), first of
//!   `paplay`/`pw-play`/`aplay`/`ffplay`/`mpv` (Linux — WAV, so even bare ALSA
//!   `aplay` decodes it);
//! - `HALO_DISABLE_SOUND` env kill-switch + the `agent_sound_enabled` coder
//!   setting;
//! - failures are logged and swallowed — a missing player must never bother
//!   the session flow.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui::{App, Global};
use halo_bridge::{
    Bridge, BridgeState, HaloAgentDelivery, HaloAgentPart, HaloAgentSession,
    HaloAgentSessionStatus, WorkData,
};

const DISABLE_ENV: &str = "HALO_DISABLE_SOUND";
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

static SOUND_DONE: &[u8] = include_bytes!("../assets/sounds/done.wav");
static SOUND_REQUEST: &[u8] = include_bytes!("../assets/sounds/request.wav");
static SOUND_ATTENTION: &[u8] = include_bytes!("../assets/sounds/attention.wav");

/// Which notification chime to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// An agent turn completed successfully.
    Done,
    /// The agent is waiting on a question.
    Request,
    /// A run failed.
    Attention,
}

/// Play a chime on a background thread. Silently a no-op when disabled or no
/// player is available.
pub fn play(sound: Sound) {
    let data = match sound {
        Sound::Done => SOUND_DONE,
        Sound::Request => SOUND_REQUEST,
        Sound::Attention => SOUND_ATTENTION,
    };
    play_in_background(data);
}

fn play_in_background(data: &'static [u8]) {
    if std::env::var_os(DISABLE_ENV).is_some() {
        return;
    }
    std::thread::spawn(move || {
        // Missing players and decode failures stay quiet — never interrupt a run.
        let _ = play_bytes(data);
    });
}

fn play_bytes(data: &[u8]) -> Result<(), String> {
    // The system players want a file path. Exclusive creation prevents a
    // predictable-name collision (including a pre-planted symlink) from
    // redirecting the embedded bytes to another file.
    let (mut file, tmp) = create_temp_file().map_err(|e| e.to_string())?;
    file.write_all(data).map_err(|e| e.to_string())?;
    drop(file);
    let result = run_player(&tmp.path);
    drop(tmp);
    result
}

struct TempSoundFile {
    path: PathBuf,
}

impl Drop for TempSoundFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn create_temp_file() -> std::io::Result<(std::fs::File, TempSoundFile)> {
    for _ in 0..128 {
        let id = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("halo-sound-{}-{id}.wav", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((file, TempSoundFile { path })),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a notification sound file",
    ))
}

#[cfg(target_os = "macos")]
fn run_player(path: &Path) -> Result<(), String> {
    run_checked("afplay", &[], path)
}

#[cfg(windows)]
fn run_player(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // SoundPlayer handles WAV natively; PlaySync keeps the process alive for
    // the chime's duration. Pass the path through the child environment rather
    // than interpolating it into PowerShell source (paths may contain quotes).
    let script = "(New-Object Media.SoundPlayer $env:HALO_SOUND_PATH).PlaySync()";
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("HALO_SOUND_PATH", path)
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| format!("powershell failed: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("powershell exited with {}", output.status))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run_player(path: &Path) -> Result<(), String> {
    let players: &[(&str, &[&str])] = &[
        ("paplay", &[]),
        ("pw-play", &[]),
        ("aplay", &["-q"]),
        ("ffplay", &["-nodisp", "-autoexit", "-loglevel", "quiet"]),
        ("mpv", &["--no-video", "--really-quiet"]),
    ];
    let mut errors = Vec::new();
    for (program, args) in players {
        match run_checked(program, args, path) {
            Ok(()) => return Ok(()),
            Err(err) => errors.push(err),
        }
    }
    Err(format!("no audio player available: {}", errors.join("; ")))
}

fn run_checked(program: &str, args: &[&str], path: &Path) -> Result<(), String> {
    // Bounded wait: a wedged audio daemon must not accumulate zombie threads.
    let mut child = std::process::Command::new(program)
        .args(args)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{program} exited with {status}")),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("{program} timed out"));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program}: {err}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Session transition detector
// ---------------------------------------------------------------------------

/// Snapshot used to decide which chime (if any) a session transition earns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSoundState {
    status: HaloAgentSessionStatus,
    awaiting_input: bool,
    /// Last bubble ends on an interrupt card — Working→Idle stays silent.
    interrupted_tail: bool,
}

impl SessionSoundState {
    fn from_session(session: &HaloAgentSession) -> Self {
        Self {
            status: session.status,
            awaiting_input: session.pending_input.is_some(),
            interrupted_tail: session_interrupted_tail(session),
        }
    }

    /// Call after saving the new baseline. Suppressed pings must never replay.
    fn sound_since(&self, prev: &Self, send_pending: bool) -> Option<Sound> {
        if self.status == HaloAgentSessionStatus::Errored
            && prev.status != HaloAgentSessionStatus::Errored
        {
            return Some(Sound::Attention);
        }
        if self.awaiting_input && !prev.awaiting_input {
            return Some(Sound::Request);
        }
        if !send_pending
            && self.status == HaloAgentSessionStatus::Idle
            && prev.status == HaloAgentSessionStatus::Working
            && !self.interrupted_tail
        {
            return Some(Sound::Done);
        }
        None
    }
}

fn session_interrupted_tail(session: &HaloAgentSession) -> bool {
    session.messages.last().is_some_and(|msg| {
        msg.parts
            .last()
            .is_some_and(|part| matches!(part, HaloAgentPart::Interrupted))
    })
}

fn session_send_pending(session: &HaloAgentSession) -> bool {
    session
        .queue
        .iter()
        .any(|item| matches!(item.delivery, HaloAgentDelivery::Pending))
}

/// Coalesce attention requests delivered by independent state watches.
#[derive(Debug, Default)]
struct AttentionSoundGate {
    last_played: Option<Instant>,
}

impl AttentionSoundGate {
    const COALESCE: Duration = Duration::from_millis(250);

    fn should_play(&mut self, now: Instant) -> bool {
        if self
            .last_played
            .is_some_and(|last| now.duration_since(last) < Self::COALESCE)
        {
            return false;
        }
        self.last_played = Some(now);
        true
    }
}

#[derive(Default)]
struct SoundWatch {
    prev: HashMap<String, SessionSoundState>,
    attention_gate: AttentionSoundGate,
}

impl Global for SoundWatch {}

/// Install after [`Bridge`] is ready. One observer for the whole process so
/// satellite windows do not double-play.
pub fn install(cx: &mut App) {
    cx.set_global(SoundWatch::default());
    let store = Bridge::state(cx);
    let observed = store.clone();
    cx.observe(store.entity(), move |_entity, cx| {
        // Snapshot only sound-relevant fields — never clone BridgeState on every
        // token batch (that alone can freeze theme/clicks while agents stream).
        let (enabled, snapshots) = {
            let state = observed.read(cx);
            collect_sound_snapshots(&state)
        };
        let sounds = cx
            .global_mut::<SoundWatch>()
            .collect_sounds(enabled, &snapshots);
        for sound in sounds {
            play(sound);
        }
    })
    .detach();
}

struct SoundSnapshot {
    id: String,
    state: SessionSoundState,
    send_pending: bool,
}

impl SoundWatch {
    fn collect_sounds(&mut self, enabled: bool, snapshots: &[SoundSnapshot]) -> Vec<Sound> {
        if std::env::var_os(DISABLE_ENV).is_some() {
            return Vec::new();
        }

        if !enabled {
            // Still advance baselines so enabling mid-session does not replay.
            for snap in snapshots {
                self.prev
                    .entry(snap.id.clone())
                    .or_insert_with(|| snap.state.clone());
            }
            return Vec::new();
        }

        let now = Instant::now();
        let mut out = Vec::new();
        for snap in snapshots {
            let prev = self.prev.insert(snap.id.clone(), snap.state.clone());
            let Some(prev) = prev else {
                // First appearance seeds silently (boot / restore).
                continue;
            };
            let Some(sound) = snap.state.sound_since(&prev, snap.send_pending) else {
                continue;
            };
            if sound == Sound::Attention && !self.attention_gate.should_play(now) {
                continue;
            }
            out.push(sound);
        }
        out
    }
}

fn collect_sound_snapshots(state: &BridgeState) -> (bool, Vec<SoundSnapshot>) {
    let enabled = state.coder_settings.agent_sound_enabled;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for session in &state.coder.agent_sessions {
        seen.insert(session.id.clone());
        out.push(SoundSnapshot {
            id: session.id.clone(),
            state: SessionSoundState::from_session(session),
            send_pending: session_send_pending(session),
        });
    }
    for tab in &state.work_tabs {
        if let WorkData::Coder(work) = &tab.data {
            for session in &work.agent_sessions {
                if !seen.insert(session.id.clone()) {
                    continue;
                }
                out.push(SoundSnapshot {
                    id: session.id.clone(),
                    state: SessionSoundState::from_session(session),
                    send_pending: session_send_pending(session),
                });
            }
        }
    }
    (enabled, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline(
        status: HaloAgentSessionStatus,
        awaiting_input: bool,
        interrupted_tail: bool,
    ) -> SessionSoundState {
        SessionSoundState {
            status,
            awaiting_input,
            interrupted_tail,
        }
    }

    #[test]
    fn interrupted_activity_never_chimes() {
        let working = baseline(HaloAgentSessionStatus::Working, false, false);
        let idle = baseline(HaloAgentSessionStatus::Idle, false, true);
        assert_eq!(idle.sound_since(&working, false), None);
    }

    #[test]
    fn a_run_error_chimes_once() {
        let working = baseline(HaloAgentSessionStatus::Working, false, false);
        let errored = baseline(HaloAgentSessionStatus::Errored, false, false);
        assert_eq!(errored.sound_since(&working, false), Some(Sound::Attention));
        assert_eq!(errored.sound_since(&errored, false), None);
    }

    #[test]
    fn successful_idle_chimes_done() {
        let working = baseline(HaloAgentSessionStatus::Working, false, false);
        let idle = baseline(HaloAgentSessionStatus::Idle, false, false);
        assert_eq!(idle.sound_since(&working, false), Some(Sound::Done));
        assert_eq!(idle.sound_since(&idle, false), None);
    }

    #[test]
    fn pending_send_consumes_completion_but_preserves_input_requests() {
        let working = baseline(HaloAgentSessionStatus::Working, false, false);
        let settled = baseline(HaloAgentSessionStatus::Idle, false, false);
        assert_eq!(settled.sound_since(&working, true), None);
        let question = baseline(HaloAgentSessionStatus::Working, true, false);
        assert_eq!(question.sound_since(&settled, true), Some(Sound::Request));
    }

    #[test]
    fn temp_files_are_exclusive_and_cleaned_up() {
        let (first_file, first) = create_temp_file().expect("reserve first temp file");
        let (second_file, second) = create_temp_file().expect("reserve second temp file");
        assert_ne!(first.path, second.path);
        assert!(first.path.exists());
        assert!(second.path.exists());
        drop(first_file);
        drop(second_file);
        let first_path = first.path.clone();
        let second_path = second.path.clone();
        drop(first);
        drop(second);
        assert!(!first_path.exists());
        assert!(!second_path.exists());
    }

    #[test]
    fn embedded_chimes_are_wav() {
        for data in [SOUND_DONE, SOUND_REQUEST, SOUND_ATTENTION] {
            assert!(data.len() > 1000);
            assert_eq!(&data[..4], b"RIFF");
            assert_eq!(&data[8..12], b"WAVE");
        }
    }

    #[test]
    fn attention_gate_coalesces() {
        let t0 = Instant::now();
        let mut gate = AttentionSoundGate::default();
        assert!(gate.should_play(t0));
        assert!(!gate.should_play(t0));
        assert!(!gate.should_play(t0 + Duration::from_millis(200)));
        assert!(gate.should_play(t0 + Duration::from_millis(250)));
    }
}

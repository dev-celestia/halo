#![cfg(unix)]
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use halo_harness::pi::{
    PiDoneStatus, PiEvent, PiImage, PiProbeResult, PiRun, PiRunOptions, probe_environment,
    resolve_executable,
};
use serde_json::Value;

struct Fixture {
    dir: tempfile::TempDir,
    executable: PathBuf,
    record: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("halo fake pi with spaces ")
            .tempdir()
            .unwrap();
        let executable = dir.path().join("fake pi");
        std::fs::write(&executable, include_str!("fixtures/pi.py")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let record = dir.path().join("records.jsonl");
        Self {
            dir,
            executable,
            record,
        }
    }
    fn options(&self, case: &str) -> PiRunOptions {
        PiRunOptions {
            run_id: "test-run".into(),
            prompt: "network-free fixture prompt".into(),
            model: Some("fake/model".into()),
            cwd: self.dir.path().to_string_lossy().into_owned(),
            custom_executable: Some(self.executable.to_string_lossy().into_owned()),
            resume: None,
            images: vec![],
            approval_mode: "full_access".into(),
            plan_mode: false,
            anthropic_api_key: Some(String::new()),
            openai_api_key: Some(String::new()),
            gemini_api_key: Some(String::new()),
            deepseek_api_key: Some(String::new()),
            ollama_base_url: Some(String::new()),
            extra_env: HashMap::from([
                ("PI_FAKE_CASE".into(), case.into()),
                (
                    "PI_FAKE_RECORD".into(),
                    self.record.to_string_lossy().into_owned(),
                ),
            ]),
        }
    }
    fn records(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.record)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    fn spawn(&self, case: &str) -> (PiRun, Receiver<PiEvent>) {
        spawn(self.options(case))
    }
}
fn spawn(options: PiRunOptions) -> (PiRun, Receiver<PiEvent>) {
    let (tx, rx) = mpsc::channel();
    let run = PiRun::spawn(options, move |event| {
        let _ = tx.send(event);
    })
    .unwrap();
    (run, rx)
}
fn until_done(rx: &Receiver<PiEvent>) -> Vec<PiEvent> {
    let mut events = Vec::new();
    loop {
        let event = rx
            .recv_timeout(Duration::from_secs(12))
            .expect("terminal event");
        let done = matches!(event, PiEvent::Done { .. });
        events.push(event);
        if done {
            break;
        }
    }
    // Dispatcher closes, rather than inventing a second terminal event at EOF/Drop.
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    events
}
fn done(events: &[PiEvent]) -> PiDoneStatus {
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, PiEvent::Done { .. }))
            .count(),
        1
    );
    match events.last().unwrap() {
        PiEvent::Done { status, .. } => *status,
        _ => panic!("missing Done"),
    }
}
fn input(rx: &Receiver<PiEvent>) -> String {
    loop {
        match rx.recv_timeout(Duration::from_secs(3)).unwrap() {
            PiEvent::InputRequested { request_id, .. } => return request_id,
            PiEvent::Done { .. } => panic!("ended before approval"),
            _ => {}
        }
    }
}

#[test]
fn immediate_settle_before_ack_is_accepted_once_with_authoritative_unicode_content() {
    let fixture = Fixture::new();
    let (run, rx) = fixture.spawn("immediate");
    let events = until_done(&rx);
    assert_eq!(done(&events), PiDoneStatus::Completed);
    assert!(!run.is_alive());
    assert_eq!(
        run.session_id().as_deref(),
        Some("/tmp/fake pi session.jsonl")
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, PiEvent::SessionStarted { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(
                |e| matches!(e, PiEvent::CommandAccepted { command_id } if command_id == "test-run")
            )
            .count(),
        1
    );
    let updates: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            PiEvent::ContentUpdated {
                message_id,
                content_index,
                text,
                reasoning,
                append,
                done,
            } => Some((message_id, content_index, text, reasoning, append, done)),
            _ => None,
        })
        .collect();
    assert!(updates.iter().all(|u| u.0 == updates[0].0));
    assert!(
        updates
            .iter()
            .any(|u| *u.1 == 1 && u.2 == "你好\u{2028}\u{2029}draft" && *u.4 && !*u.5)
    );
    assert!(
        updates
            .iter()
            .any(|u| *u.1 == 1 && u.2 == "你好\u{2028}\u{2029}final" && !*u.4 && *u.5)
    );
    assert_eq!(updates.iter().filter(|u| *u.1 == 0 && *u.5).count(), 1);
    assert!(!events.iter().any(|e| matches!(
        e,
        PiEvent::TextDelta { .. } | PiEvent::ReasoningDelta { .. }
    )));
    assert!(events.iter().any(
        |e| matches!(e, PiEvent::ToolResult { output: Some(s), .. } if s == "readable result")
    ));
}

#[test]
fn rejected_and_handled_prompts_finish_visibly() {
    for (case, expected) in [
        ("reject", PiDoneStatus::Errored),
        ("handled", PiDoneStatus::Completed),
    ] {
        let fixture = Fixture::new();
        let (_run, rx) = fixture.spawn(case);
        let events = until_done(&rx);
        assert_eq!(done(&events), expected);
        if case == "reject" {
            assert!(events.iter().any(|e| matches!(e, PiEvent::CommandRejected { command_id, .. } if command_id == "test-run")));
        }
    }
}

#[test]
fn agent_end_is_not_terminal_and_provider_failure_is_not_success() {
    for (case, expected) in [
        ("retry", PiDoneStatus::Completed),
        ("provider_error", PiDoneStatus::Errored),
    ] {
        let fixture = Fixture::new();
        let (_run, rx) = fixture.spawn(case);
        let events = until_done(&rx);
        assert_eq!(done(&events), expected);
        if case == "retry" {
            assert!(events.iter().any(|e| matches!(e, PiEvent::ContentUpdated { text, done: true, .. } if text == "recovered")));
        }
    }
}

#[test]
fn malformed_oversized_split_and_unexpected_eof_fail_without_raw_diagnostics() {
    for case in [
        "malformed",
        "oversize",
        "invalid_utf8",
        "partial_eof",
        "crash",
        "eof",
    ] {
        let fixture = Fixture::new();
        let (_run, rx) = fixture.spawn(case);
        let events = until_done(&rx);
        assert_eq!(done(&events), PiDoneStatus::Errored, "{case}");
        assert!(!format!("{events:?}").contains("RAW_PROMPT_SECRET"));
    }
}

#[test]
fn attachments_resume_exact_model_and_plan_flags_reach_the_wire() {
    let fixture = Fixture::new();
    let mut options = fixture.options("immediate");
    options.resume = Some("/tmp/previous session.jsonl".into());
    options.model = Some("fake/other".into());
    options.images.push(PiImage {
        data: "aGVsbG8=".into(),
        mime_type: "image/png".into(),
    });
    options.plan_mode = true;
    options.approval_mode = "full_access".into();
    let (_run, rx) = spawn(options);
    assert_eq!(done(&until_done(&rx)), PiDoneStatus::Completed);
    let records = fixture.records();
    let args = records[0]["argv"].as_array().unwrap();
    for arg in [
        "--session",
        "/tmp/previous session.jsonl",
        "--tools",
        "read,grep,find,ls",
        "--no-mcp",
    ] {
        assert!(args.contains(&Value::from(arg)));
    }
    // Providers are extensions: `--no-extensions` would unload every model.
    assert!(!args.contains(&Value::from("--no-extensions")));
    assert!(!args.contains(&Value::from("-ne")));
    assert!(!args.contains(&Value::from("--approve")));
    let prompt = records.iter().find(|r| r["type"] == "prompt").unwrap();
    assert_eq!(prompt["id"], "test-run");
    assert_eq!(
        prompt["images"][0],
        serde_json::json!({"type":"image","data":"aGVsbG8=","mimeType":"image/png"})
    );
    assert!(
        records
            .iter()
            .any(|r| r["type"] == "set_model" && r["modelId"] == "other")
    );
}

#[test]
fn steering_preserves_images_and_clone_drop_is_nonblocking() {
    let fixture = Fixture::new();
    let (run, rx) = fixture.spawn("wait");
    let clone = run.clone();
    drop(clone);
    assert!(run.is_alive());
    run.steer_with_images(
        "steer-1",
        "look again",
        vec![PiImage {
            data: "aGVsbG8=".into(),
            mime_type: "image/jpeg".into(),
        }],
    )
    .unwrap();
    assert!(
        run.steer_with_images("steer-1", "duplicate", vec![])
            .is_err()
    );
    let events = until_done(&rx);
    assert_eq!(done(&events), PiDoneStatus::Completed);
    assert!(
        events.iter().any(
            |e| matches!(e, PiEvent::CommandAccepted { command_id } if command_id == "steer-1")
        )
    );
    let records = fixture.records();
    let steer = records.iter().find(|r| r["type"] == "steer").unwrap();
    assert_eq!(steer["images"][0]["mimeType"], "image/jpeg");
    assert!(!records.iter().any(|r| r["type"] == "queue_message"));
}

#[test]
fn rejected_steering_does_not_fake_acceptance() {
    let fixture = Fixture::new();
    let (run, rx) = fixture.spawn("steer_reject");
    run.steer_with_images("steer-1", "no", vec![]).unwrap();
    loop {
        if matches!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), PiEvent::CommandRejected { command_id, .. } if command_id == "steer-1")
        {
            break;
        }
    }
    run.request_interrupt();
    assert_eq!(done(&until_done(&rx)), PiDoneStatus::Interrupted);
}

#[test]
fn missing_capability_or_model_prevents_prompt_delivery() {
    for case in ["unsupported_models", "missing_model"] {
        let fixture = Fixture::new();
        let mut options = fixture.options(case);
        if case == "missing_model" {
            options.model = Some("absent/model".into());
        }
        assert!(PiRun::spawn(options, |_| {}).is_err());
        assert!(!fixture.records().iter().any(|r| r["type"] == "prompt"));
    }
}

#[test]
fn unsupported_version_and_invalid_explicit_executable_do_not_fall_back() {
    let fixture = Fixture::new();
    let mut options = fixture.options("immediate");
    options
        .extra_env
        .insert("PI_FAKE_VERSION".into(), "0.99.0".into());
    assert!(
        PiRun::spawn(options, |_| {})
            .err()
            .unwrap()
            .contains("1.4.0")
    );
    assert!(fixture.records().is_empty());
    let path = fixture
        .dir
        .path()
        .join("missing")
        .to_string_lossy()
        .into_owned();
    assert!(resolve_executable(Some(&path)).is_none());
    assert!(matches!(
        probe_environment(Some(&path)),
        PiProbeResult::ProbeFailed(_)
    ));
    assert!(resolve_executable(Some("")).is_none());
    assert!(resolve_executable(Some("pi")).is_none());
}

#[test]
fn sparse_path_finds_adjacent_node_and_handles_spaces() {
    let fixture = Fixture::new();
    std::fs::write(
        &fixture.executable,
        include_str!("fixtures/pi.py").replacen("#!/usr/bin/python3", "#!/usr/bin/env node", 1),
    )
    .unwrap();
    let node = fixture.dir.path().join("node");
    std::fs::write(&node, "#!/bin/sh\nexec /usr/bin/python3 \"$@\"\n").unwrap();
    std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut options = fixture.options("handled");
    options
        .extra_env
        .insert("PATH".into(), "/usr/bin:/bin".into());
    let (_run, rx) = spawn(options);
    assert_eq!(done(&until_done(&rx)), PiDoneStatus::Completed);
}

#[test]
fn child_provider_overrides_remove_empty_values_and_remain_isolated() {
    let fixture_a = Fixture::new();
    let fixture_b = Fixture::new();
    let mut a = fixture_a.options("handled");
    a.extra_env
        .insert("OPENAI_API_KEY".into(), "test-inherited".into());
    a.openai_api_key = None;
    let mut b = fixture_b.options("handled");
    b.extra_env
        .insert("OPENAI_API_KEY".into(), "test-removed".into());
    let (_a, rx_a) = spawn(a);
    let (_b, rx_b) = spawn(b);
    until_done(&rx_a);
    until_done(&rx_b);
    assert_eq!(fixture_a.records()[0]["openai"], "test-inherited");
    assert_eq!(fixture_b.records()[0]["openai_present"], false);
}

#[test]
fn abort_and_last_handle_drop_reap_even_when_stdin_and_callbacks_are_blocked() {
    for final_drop in [false, true] {
        let fixture = Fixture::new();
        let mut options = fixture.options("blocked");
        options.images.push(PiImage {
            data: "YQ==".repeat(2 * 1024 * 1024),
            mime_type: "image/png".into(),
        });
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let gate = std::sync::Mutex::new(gate_rx);
        let (done_tx, done_rx) = mpsc::channel();
        let run = PiRun::spawn(options, move |event| {
            if matches!(event, PiEvent::SessionStarted { .. }) {
                let _ = gate.lock().unwrap().recv();
            }
            if matches!(event, PiEvent::Done { .. }) {
                let _ = done_tx.send(event);
            }
        })
        .unwrap();
        let descendant = fixture
            .records()
            .iter()
            .find_map(|r| r["descendant"].as_u64())
            .unwrap() as i32;
        let start = Instant::now();
        if final_drop {
            drop(run);
        } else {
            run.request_interrupt();
            assert!(start.elapsed() < Duration::from_millis(100));
            let limit = Instant::now() + Duration::from_secs(4);
            while run.is_alive() && Instant::now() < limit {
                std::thread::park_timeout(Duration::from_millis(20));
            }
            assert!(!run.is_alive(), "callback must not block child cleanup");
        }
        assert!(start.elapsed() < Duration::from_secs(4));
        gate_tx.send(()).unwrap();
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_secs(4)).unwrap(),
            PiEvent::Done {
                status: PiDoneStatus::Interrupted,
                ..
            }
        ));
        // An orphan can briefly remain a zombie until the OS reaps it, but must not be running.
        let stat = std::process::Command::new("/bin/ps")
            .args(["-o", "stat=", "-p", &descendant.to_string()])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&stat.stdout);
        assert!(
            stat.trim().is_empty() || stat.trim().starts_with('Z'),
            "surviving tool: {stat}"
        );
    }
}

#[test]
fn approval_denial_stale_answers_and_private_extension_cleanup() {
    let fixture = Fixture::new();
    let mut options = fixture.options("approval");
    options.approval_mode = "ask".into();
    let (run, rx) = spawn(options);
    let id = input(&rx);
    let records = fixture.records();
    let args = records[0]["argv"].as_array().unwrap();
    // A provider lives in an extension package, so the launch must keep
    // extension discovery on — `-ne` left Pi with no models at all.
    assert!(!args.contains(&Value::from("-ne")));
    assert!(args.contains(&Value::from("--no-mcp")));
    assert!(!args.contains(&Value::from("--approve")));
    let i = args.iter().position(|v| v == "-e").unwrap();
    let path = PathBuf::from(args[i + 1].as_str().unwrap());
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    run.respond_input(&id, false).unwrap();
    assert!(run.respond_input(&id, true).is_err());
    assert!(run.respond_input("old-run-request", true).is_err());
    let events = until_done(&rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PiEvent::InputResolved { request_id } if request_id == &id))
    );
    assert!(events.iter().any(|e| matches!(e, PiEvent::ToolResult { is_error: true, output: Some(s), .. } if s == "denied")));
    assert!(!path.exists());
}

#[test]
fn approval_timeout_denies_without_an_answer() {
    let fixture = Fixture::new();
    let mut options = fixture.options("approval_timeout");
    options.approval_mode = "ask".into();
    let (run, rx) = spawn(options);
    let id = input(&rx);
    let events = until_done(&rx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PiEvent::InputResolved { request_id } if request_id == &id))
    );
    assert!(run.respond_input(&id, true).is_err());
    let records = fixture.records();
    assert!(
        records
            .iter()
            .any(|r| r["type"] == "extension_ui_response" && r["confirmed"] == false)
    );
}

#[test]
fn approval_readiness_and_load_failures_prevent_prompt() {
    for case in ["extension_error", "no_ready"] {
        let fixture = Fixture::new();
        let mut options = fixture.options(case);
        options.approval_mode = "ask".into();
        let error = PiRun::spawn(options, |_| {}).err().unwrap();
        assert!(
            error.contains("enforcement") || error.contains("approval hook"),
            "{error}"
        );
        assert!(!fixture.records().iter().any(|r| r["type"] == "prompt"));
    }
}

#[test]
fn unanswered_command_is_uncertain_and_never_replayed() {
    let fixture = Fixture::new();
    let (_run, rx) = fixture.spawn("no_ack");
    let events = until_done(&rx);
    assert_eq!(done(&events), PiDoneStatus::Errored);
    assert!(
        events.iter().any(
            |e| matches!(e, PiEvent::CommandRejected { error, .. } if error.contains("unknown"))
        )
    );
    assert_eq!(
        fixture
            .records()
            .iter()
            .filter(|r| r["type"] == "prompt")
            .count(),
        1
    );
}

#[test]
fn version_probe_has_time_and_output_bounds() {
    for case in ["version_timeout", "version_oversize"] {
        let fixture = Fixture::new();
        let start = Instant::now();
        assert!(PiRun::spawn(fixture.options(case), |_| {}).is_err());
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(fixture.records().is_empty());
    }
}

#[test]
fn embedded_approval_hook_uses_a_fake_pi_api_only() {
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join("node")).collect())
        .unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/node/bin/node"));
        candidates.push(home.join(".local/bin/node"));
    }
    candidates.extend([
        PathBuf::from("/usr/local/bin/node"),
        PathBuf::from("/opt/homebrew/bin/node"),
        PathBuf::from("/usr/bin/node"),
    ]);
    let node = candidates
        .into_iter()
        .find(|p| p.is_file())
        .expect("Node required for the network-free extension test");
    let output = std::process::Command::new(node)
        .arg(format!(
            "{}/tests/fixtures/approval_test.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

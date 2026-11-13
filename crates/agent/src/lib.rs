//! Halo Agent execution. Startup and image preparation run off-thread;
//! every event and command is scoped to a registered chat/run identity.
//! Pi uses JSON-L RPC; OpenCode uses `opencode serve` HTTP/SSE;
//! Antigravity uses `agy_acp_server` ACP stdio.

pub mod antigravity_bridge;
pub mod browser;
mod catalog;
mod cursor;
mod images;
pub mod opencode_bridge;
pub mod pi_bridge;
pub mod runtime;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;

use halo_core::tauri::{AppHandle, Manager};

pub use browser::{
    BrowserCommand, BrowserStatus, browser_close, browser_close_all, browser_forget,
    browser_status, classify_command,
};
pub use catalog::{FileSearchMatch, SkillInfo};
pub use cursor::{
    CursorAuthStatus, auth_status as cursor_auth_status, ensure_shim as cursor_ensure_shim,
    resolve_node as cursor_resolve_node, save_api_key as cursor_save_api_key,
    sdk_ready as cursor_sdk_ready, session_ready as cursor_session_ready,
};
pub use halo_harness::migrate_model_id;
pub use halo_harness::opencode::OpenCodeSkill;
pub use halo_harness::{McpKeyValue, McpServerConfig, McpTransport};
pub use images::is_supported_image_path;
pub use runtime::{HaloAgentState, RequestInputFn, ensure_managed};
pub use types::{
    AgentEvent, AgentEventPayload, AgentLaunchContext, AntigravityProbeStatus, DoneStatus,
    FileChangeKind, HarnessId, HarnessInfo, HarnessReadiness, ModelInfo, OpenCodeProbeStatus,
    PiProbeStatus, RunRequest, TodoItem, ToolCall, UserInputAnswer, UserInputQuestion,
};

use runtime::{ActiveRun, LiveHandle, clear_run_inputs, event_sink, input_key, is_current};

pub fn search_files(root: &str, query: &str) -> Result<Vec<FileSearchMatch>, String> {
    catalog::search_files(root, query)
}

pub fn list_skills(root: &str, harness: &str) -> Result<Vec<SkillInfo>, String> {
    catalog::list_skills(root, harness)
}

/// Prefix a prompt with the selected-skill preamble when there is one.
fn with_skills_preamble(prompt: String, preamble: &str) -> String {
    if preamble.trim().is_empty() {
        prompt
    } else {
        format!("{preamble}\n{prompt}")
    }
}

/// Skills the composer should offer.
///
/// OpenCode's own server is authoritative — it applies config, compatibility
/// roots and precedence rules a filesystem walk cannot reproduce. When the
/// server is missing, too old, or reports nothing, fall back to the local
/// walk so `/` still works offline.
pub fn discover_skills(
    root: &str,
    harness: &str,
    opencode_executable: Option<&str>,
) -> Result<Vec<SkillInfo>, String> {
    if matches!(HarnessId::parse(harness), Some(HarnessId::OpenCode)) {
        if let Ok(skills) = halo_harness::opencode::discover_skills(opencode_executable, root) {
            if !skills.is_empty() {
                return Ok(skills.into_iter().map(skill_from_opencode).collect());
            }
        }
    }
    catalog::list_skills(root, harness)
}

fn skill_from_opencode(skill: OpenCodeSkill) -> SkillInfo {
    SkillInfo {
        id: skill.id,
        name: skill.name,
        path: skill.path,
        description: skill.description,
        scope: skill.scope,
    }
}

/// Prompt preamble for harnesses that cannot attach skills natively
/// (Pi, Antigravity). Empty when no skills are selected.
pub fn skills_preamble(skills: &[SkillInfo]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "The user enabled these skills for this chat. Read a skill's file before following it:\n",
    );
    for skill in skills {
        out.push_str("- ");
        out.push_str(skill.name.trim());
        if !skill.description.trim().is_empty() {
            out.push_str(": ");
            out.push_str(skill.description.trim());
        }
        if !skill.path.trim().is_empty() {
            out.push_str(" (");
            out.push_str(skill.path.trim());
            out.push(')');
        }
        out.push('\n');
    }
    out
}

pub fn list_harnesses(
    pi_custom: Option<&str>,
    opencode_custom: Option<&str>,
    antigravity_custom: Option<&str>,
) -> Vec<HarnessInfo> {
    vec![
        HarnessInfo {
            id: HarnessId::Pi.as_str().into(),
            display_name: HarnessId::Pi.display_name().into(),
            installed: is_harness_installed("pi", pi_custom),
            available: true,
        },
        HarnessInfo {
            id: HarnessId::OpenCode.as_str().into(),
            display_name: HarnessId::OpenCode.display_name().into(),
            installed: is_harness_installed("opencode", opencode_custom),
            available: true,
        },
        HarnessInfo {
            id: HarnessId::Antigravity.as_str().into(),
            display_name: HarnessId::Antigravity.display_name().into(),
            installed: is_harness_installed("antigravity", antigravity_custom),
            available: true,
        },
        HarnessInfo {
            id: HarnessId::Cursor.as_str().into(),
            display_name: HarnessId::Cursor.display_name().into(),
            installed: is_harness_installed("cursor", None),
            available: true,
        },
    ]
}

pub fn list_models(harness: &str) -> Vec<ModelInfo> {
    list_models_with(harness, None)
}

pub fn list_models_with(harness: &str, custom_executable: Option<&str>) -> Vec<ModelInfo> {
    match HarnessId::parse(harness).unwrap_or(HarnessId::Pi) {
        // Ask Pi what it can actually serve. Its answer is credential-filtered,
        // so a model shown here is a model that can run; the curated fallback
        // only covers a CLI that cannot answer at all.
        HarnessId::Pi => halo_harness::pi_list_models(custom_executable)
            .filter(|models| !models.is_empty())
            .unwrap_or_else(halo_harness::default_models)
            .into_iter()
            .map(|m| ModelInfo {
                supports_images: pi_supports_images(&m.id),
                id: m.id,
                display_name: m.display_name,
            })
            .collect(),
        HarnessId::OpenCode => halo_harness::opencode_list_models(custom_executable)
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                display_name: m.display_name,
                supports_images: m.supports_images,
            })
            .collect(),
        HarnessId::Antigravity => halo_harness::antigravity_default_models()
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                display_name: m.display_name,
                supports_images: Some(true),
            })
            .collect(),
        HarnessId::Cursor => cursor::discover_models()
            .into_iter()
            .map(|(id, display_name)| ModelInfo {
                id,
                display_name,
                supports_images: Some(true),
            })
            .collect(),
    }
}

/// Static vision capability for Pi's curated catalog. `None` keeps unknown
/// local models (Ollama) attachable.
fn pi_supports_images(id: &str) -> Option<bool> {
    match id.split_once('/').map(|(provider, _)| provider) {
        Some("anthropic" | "google") => Some(true),
        Some("openai") => Some(id != "openai/o3-mini"),
        Some("deepseek" | "groq") => Some(false),
        _ => None,
    }
}

pub fn is_harness_installed(harness: &str, custom_executable: Option<&str>) -> bool {
    match HarnessId::parse(harness).unwrap_or(HarnessId::Pi) {
        HarnessId::Pi => halo_harness::is_installed(custom_executable),
        HarnessId::OpenCode => halo_harness::opencode_is_installed(custom_executable),
        HarnessId::Antigravity => halo_harness::antigravity_is_installed(custom_executable),
        HarnessId::Cursor => cursor::sdk_ready(),
    }
}

pub fn probe_pi(custom_executable: Option<&str>) -> PiProbeStatus {
    halo_harness::probe_environment(custom_executable).into()
}

pub fn probe_opencode(custom_executable: Option<&str>) -> OpenCodeProbeStatus {
    halo_harness::opencode_probe_environment(custom_executable).into()
}

pub fn probe_antigravity(custom_executable: Option<&str>) -> AntigravityProbeStatus {
    halo_harness::antigravity_probe_environment(custom_executable).into()
}

pub fn install_antigravity() -> Result<String, String> {
    halo_harness::antigravity_ensure_installed().map(|p| p.display().to_string())
}

pub fn install_antigravity_with_progress<F>(on_progress: F) -> Result<String, String>
where
    F: FnMut(String),
{
    halo_harness::antigravity_ensure_installed_with_progress(on_progress)
        .map(|p| p.display().to_string())
}

/// Install OpenCode ≥ 2.2.0 (v2 installer for 1.x; in-place upgrade for too-old 2.x).
pub fn install_opencode(custom_executable: Option<&str>) -> Result<(), String> {
    halo_harness::opencode_install_or_upgrade(custom_executable)
}

/// Accept startup, not delivery. The caller registers this ID before invoking
/// us; CommandAccepted confirms Pi receipt and Done is the single terminal event.
pub fn run(
    app: &AppHandle,
    request: RunRequest,
    launch: AgentLaunchContext,
) -> Result<String, String> {
    ensure_managed(app);
    if request.chat_id.trim().is_empty() || request.run_id.trim().is_empty() {
        return Err("Missing agent chat or run id".into());
    }
    let state = app.state::<HaloAgentState>();
    let old = {
        let mut active = state.active.lock();
        if active
            .get(&request.chat_id)
            .is_some_and(|run| run.run_id == request.run_id)
        {
            return Err("This agent run is already registered".into());
        }
        active.insert(
            request.chat_id.clone(),
            ActiveRun {
                run_id: request.run_id.clone(),
                handle: None,
            },
        )
    };
    if let Some(old) = old {
        clear_run_inputs(&state, &request.chat_id, &old.run_id);
        if let Some(handle) = old.handle {
            handle.request_interrupt();
        }
    }

    let run_id = request.run_id.clone();
    let chat_id = request.chat_id.clone();
    let worker_app = app.clone();
    let sink = event_sink(app, &chat_id, &run_id);
    let failed_sink = Arc::clone(&sink);
    let failed_id = run_id.clone();
    let spawn = std::thread::Builder::new()
        .name("coder-agent-start".into())
        .spawn(move || {
            let start = || -> Result<(), String> {
                let images = images::prepare_images(&request.attachments)?;
                if !is_current(&worker_app, &request.chat_id, &request.run_id) {
                    return Ok(());
                }
                // Harnesses without native skill attachment get the selected
                // skills as prompt preamble; OpenCode attaches ids instead.
                let preamble = skills_preamble(&request.skills);
                let opencode_skill_ids: Vec<String> = request
                    .skills
                    .iter()
                    .map(|skill| skill.id.clone())
                    .collect();
                let opencode_mcp = if request.harness == HarnessId::OpenCode {
                    let base = std::env::var("OPENCODE_CONFIG_CONTENT").ok();
                    halo_harness::opencode_config_with_mcp(base.as_deref(), &request.mcp_servers)?
                } else {
                    None
                };
                // One browser session per chat. The harness inherits this, so
                // every `halo-browser` it spawns lands on the same tab and the
                // same generation counter as the transcript card that shows it.
                let mut browser_env = HashMap::from([(
                    "HALO_BROWSER_SESSION".to_string(),
                    browser::session_for(&request.chat_id),
                )]);
                if request.computer_use {
                    browser_env.insert("HALO_DESKTOP_CHAT".into(), request.chat_id.clone());
                    browser_env.insert("HALO_DESKTOP_SESSION".into(), request.chat_id.clone());
                    browser_env.insert(
                        "HALO_DESKTOP_HOME".into(),
                        halo_harness::desktop::engine_home().display().to_string(),
                    );
                    browser_env.insert(
                        "HALO_DESKTOP_RUN_PID".into(),
                        std::process::id().to_string(),
                    );
                    if request.computer_use_headed {
                        browser_env.insert("HALO_DESKTOP_ALLOW_HEADED".into(), "1".into());
                    }
                }
                // Pi takes the browser block as a system prompt (`pi/run.rs`);
                // the other two have no such hook, so it rides on the prompt.
                let chat_id = request.chat_id.clone();
                let run_id = request.run_id.clone();
                let desktop_note = if request.computer_use && request.harness != HarnessId::Pi {
                    halo_harness::desktop::prompt_suffix()
                } else {
                    String::new()
                };
                let browser_note = if request.harness == HarnessId::Pi {
                    String::new()
                } else {
                    format!(
                        "{}{desktop_note}",
                        halo_harness::browser::prompt_suffix()
                    )
                };
                let live = match request.harness {
                    HarnessId::Pi => {
                        let resume = request
                            .resume
                            .filter(|r| std::path::Path::new(r).is_file());
                        let prompt = match (&resume, request.fallback_context.as_deref()) {
                            (None, Some(ctx)) if !ctx.trim().is_empty() => {
                                format!("{}\n\n{}", ctx.trim(), request.prompt)
                            }
                            _ => request.prompt,
                        };
                        let options = halo_harness::PiRunOptions {
                            run_id: request.run_id.clone(),
                            prompt: with_skills_preamble(prompt, &preamble),
                            model: request.model.map(|m| halo_harness::migrate_model_id(&m)),
                            cwd: request.cwd,
                            resume,
                            images,
                            approval_mode: request.approval_mode,
                            plan_mode: request.plan_mode,
                            custom_executable: launch.custom_executable,
                            anthropic_api_key: launch.anthropic_api_key,
                            openai_api_key: launch.openai_api_key,
                            gemini_api_key: launch.gemini_api_key,
                            deepseek_api_key: launch.deepseek_api_key,
                            ollama_base_url: launch.ollama_base_url,
                            extra_env: browser_env.clone(),
                        };
                        let handle = halo_harness::PiRun::spawn(options, move |event| {
                            sink(pi_bridge::map_pi_event(event));
                        })?;
                        LiveHandle::Pi(handle)
                    }
                    HarnessId::OpenCode => {
                        let mut extra_env = browser_env.clone();
                        if let Some(config) = opencode_mcp {
                            extra_env.insert("OPENCODE_CONFIG_CONTENT".to_string(), config);
                        }
                        let plan_prefix = if request.plan_mode {
                            "You are in Plan mode (read-only). Analyze the codebase, inspect files, and present a concrete step-by-step plan without modifying files or running mutating commands.\n\n"
                        } else {
                            ""
                        };
                        let options = halo_harness::OpenCodeRunOptions {
                            run_id: request.run_id.clone(),
                            prompt: format!("{plan_prefix}{}{browser_note}", request.prompt),
                            model: request.model,
                            cwd: Some(request.cwd),
                            resume: request.resume,
                            fallback_context: request.fallback_context,
                            // Never fall back to Pi's `custom_executable` — that path is
                            // `~/.halo/pi/bin/pi` and would surface as "OpenCode 1.4.0 too old".
                            custom_executable: launch.opencode_executable,
                            extra_env,
                            skills: opencode_skill_ids,
                            images,
                            approval_mode: request.approval_mode,
                            plan_mode: request.plan_mode,
                        };
                        let handle = halo_harness::OpenCodeRun::spawn(options, move |event| {
                            sink(opencode_bridge::map_opencode_event(event));
                        })?;
                        LiveHandle::OpenCode(handle)
                    }
                    HarnessId::Antigravity => {
                        let plan_prefix = if request.plan_mode {
                            "You are in Plan mode (read-only). Analyze the codebase, inspect files, and present a concrete step-by-step plan without modifying files or running mutating commands.\n\n"
                        } else {
                            ""
                        };
                        let options = halo_harness::AntigravityRunOptions {
                            run_id: request.run_id.clone(),
                            prompt: format!(
                                "{plan_prefix}{}{browser_note}",
                                with_skills_preamble(request.prompt, &preamble)
                            ),
                            model: request.model,
                            cwd: Some(request.cwd),
                            resume: request.resume,
                            fallback_context: request.fallback_context,
                            // Same as OpenCode: Pi's path must not leak into Antigravity.
                            custom_executable: launch.antigravity_executable,
                            approval_mode: request.approval_mode,
                            plan_mode: request.plan_mode,
                            extra_env: browser_env.clone(),
                            images,
                        };
                        let handle = halo_harness::AntigravityRun::spawn(options, move |event| {
                            sink(antigravity_bridge::map_antigravity_event(event));
                        })?;
                        LiveHandle::Antigravity(handle)
                    }
                    HarnessId::Cursor => {
                        let handle = cursor::CursorRun::spawn(request, move |event| sink(event))?;
                        LiveHandle::Cursor(handle)
                    }
                };
                let mut handle = Some(live);
                {
                    let state = worker_app.state::<HaloAgentState>();
                    let mut active = state.active.lock();
                    if let Some(run) = active.get_mut(&chat_id).filter(|run| run.run_id == run_id) {
                        run.handle = handle.take();
                    }
                }
                if handle.is_none() {
                    // Handle is live. Follow-ups queued during startup can merge now.
                    failed_sink(AgentEvent::HarnessReady);
                }
                // A fast terminal event or cancellation may have retired Starting.
                if let Some(handle) = handle {
                    handle.request_interrupt();
                }
                Ok(())
            };
            if let Err(error) = start() {
                failed_sink(AgentEvent::CommandRejected {
                    command_id: failed_id,
                    error: error.clone(),
                });
                failed_sink(AgentEvent::Done {
                    status: DoneStatus::Errored,
                    result: None,
                    error: Some(error),
                    session_id: None,
                });
            }
        });
    if let Err(error) = spawn {
        let retired = {
            let mut active = state.active.lock();
            if active.get(&chat_id).is_some_and(|run| run.run_id == run_id) {
                active.remove(&chat_id)
            } else {
                None
            }
        };
        drop(retired);
        return Err(format!("Could not start the agent worker: {error}"));
    }
    Ok(run_id)
}

/// True once this chat's harness process exists AND the turn can still take
/// a mid-run steer (not settled, not interrupted). The queue-first flow uses
/// this to leave follow-ups Pending for the next drain instead of
/// dispatch→reject→requeue churn.
pub fn can_steer(app: &AppHandle, chat_id: &str) -> bool {
    let state = app.state::<HaloAgentState>();
    let active = state.active.lock();
    active.get(chat_id).is_some_and(|run| {
        run.handle
            .as_ref()
            .is_some_and(|handle| handle.accepts_steers())
    })
}

fn live_handle(app: &AppHandle, chat_id: &str, run_id: &str) -> Result<LiveHandle, String> {
    let state = app.state::<HaloAgentState>();
    let active = state.active.lock();
    let run = active
        .get(chat_id)
        .filter(|run| run.run_id == run_id)
        .ok_or("The agent run is no longer active")?;
    run.handle
        .clone()
        .ok_or_else(|| "The agent is still starting; keep this message queued".into())
}

/// The sole positive answer is one explicit Allow for this exact question.
fn confirmed(request_id: &str, answers: &[UserInputAnswer]) -> bool {
    matches!(answers, [answer] if answer.question_id == request_id && answer.labels.as_slice() == ["Allow"])
}

fn selected_label<'a>(request_id: &str, answers: &'a [UserInputAnswer]) -> Option<&'a str> {
    match answers {
        [answer] if answer.question_id == request_id && answer.labels.len() == 1 => {
            Some(answer.labels[0].as_str())
        }
        _ => None,
    }
}

/// Tell the in-flight harness the composer approval mode changed.
/// A run that already started does not re-read the mode on its own.
pub fn set_live_approval_mode(
    app: &AppHandle,
    chat_id: &str,
    run_id: &str,
    mode: &str,
) -> Result<(), String> {
    ensure_managed(app);
    let handle = live_handle(app, chat_id, run_id)?;
    handle.set_approval_mode(mode);
    Ok(())
}

pub fn respond_input(
    app: &AppHandle,
    chat_id: &str,
    run_id: &str,
    request_id: &str,
    answers: Vec<UserInputAnswer>,
) -> Result<(), String> {
    ensure_managed(app);
    let key = input_key(chat_id, run_id, request_id);
    let state = app.state::<HaloAgentState>();
    if !state.pending_inputs.lock().contains_key(&key) {
        return Err("No pending approval for this chat and run".into());
    }
    let handle = live_handle(app, chat_id, run_id)?;
    handle.respond_input(
        request_id,
        confirmed(request_id, &answers),
        selected_label(request_id, &answers),
    )?;
    state.pending_inputs.lock().remove(&key);
    Ok(())
}

/// Keep image reads off the foreground thread; transport acceptance is reported
/// asynchronously using the caller's queue/delivery ID.
pub fn steer(
    app: &AppHandle,
    chat_id: &str,
    run_id: &str,
    command_id: &str,
    prompt: &str,
    attachments: &[String],
) -> Result<(), String> {
    ensure_managed(app);
    let handle = live_handle(app, chat_id, run_id)?;
    if command_id.is_empty() {
        return Err("Missing agent delivery id".into());
    }
    let worker_app = app.clone();
    let chat_id = chat_id.to_owned();
    let run_id = run_id.to_owned();
    let command_id = command_id.to_owned();
    let prompt = prompt.to_owned();
    let attachments = attachments.to_vec();
    let sink = event_sink(app, &chat_id, &run_id);
    std::thread::Builder::new()
        .name("coder-agent-steer".into())
        .spawn(move || {
            let result = images::prepare_images(&attachments).and_then(|images| {
                if !is_current(&worker_app, &chat_id, &run_id) {
                    return Err("The agent run ended before this message was sent".into());
                }
                handle.steer_with_images(&command_id, &prompt, images)
            });
            match result {
                Ok(()) => {
                    // Pi and OpenCode ack their steers natively (correlated
                    // command ids) on their supervisor threads — emitting a
                    // synthetic Accepted early here would clear the queue row
                    // before the supervisor actually sends the steer and hide
                    // a subsequent CommandRejected error.
                    // Antigravity and Cursor never ack a steer natively;
                    // synthesize Accepted for them so Done → settle_run does
                    // not mark the queue row Uncertain.
                    if matches!(handle, LiveHandle::Antigravity(_) | LiveHandle::Cursor(_)) {
                        sink(AgentEvent::CommandAccepted { command_id });
                    }
                    // The steered reply is a new turn. Without a boundary
                    // event the follow-up streams into the previous
                    // response's text part, and the user's message looks
                    // ignored ("…to run!hellow world"). Cursor's SDK reports
                    // its own `steered`; the other harnesses never do, so the
                    // boundary is acked here. A duplicate is a no-op — the
                    // reducer reuses the empty boundary row.
                    if !matches!(handle, LiveHandle::Cursor(_)) {
                        sink(AgentEvent::Steered);
                    }
                }
                Err(error) => sink(AgentEvent::CommandRejected { command_id, error }),
            }
        })
        .map_err(|e| format!("Could not prepare the agent message: {e}"))?;
    Ok(())
}

pub fn interrupt_chat(app: &AppHandle, chat_id: &str) {
    ensure_managed(app);
    let state = app.state::<HaloAgentState>();
    let retired = state.active.lock().remove(chat_id);
    if let Some(run) = retired {
        clear_run_inputs(&state, chat_id, &run.run_id);
        if let Some(handle) = run.handle {
            handle.request_interrupt();
        }
    }
}

pub fn forget_harness_session(app: &AppHandle, chat_id: &str) {
    ensure_managed(app);
    app.state::<HaloAgentState>()
        .harness_sessions
        .lock()
        .remove(chat_id);
}

pub fn interrupt(app: &AppHandle) {
    ensure_managed(app);
    let state = app.state::<HaloAgentState>();
    let runs: Vec<_> = state.active.lock().drain().collect();
    for (chat_id, run) in runs {
        clear_run_inputs(&state, &chat_id, &run.run_id);
        if let Some(handle) = run.handle {
            handle.request_interrupt();
        }
    }
}

pub fn active_run_id(app: &AppHandle, chat_id: &str) -> Option<String> {
    ensure_managed(app);
    app.state::<HaloAgentState>()
        .active
        .lock()
        .get(chat_id)
        .map(|run| run.run_id.clone())
}

// ── Google Antigravity ACP (`agy_acp_server` browser OAuth) ─────────────────

pub use halo_harness::AntigravityAuthEvent;

pub fn antigravity_acp_is_signed_in() -> bool {
    halo_harness::antigravity_acp_is_signed_in()
}

pub fn antigravity_acp_logout(custom_executable: Option<&str>) -> Result<(), String> {
    halo_harness::antigravity_acp_logout(custom_executable)
}

pub fn antigravity_acp_run_login(
    custom_executable: Option<&str>,
    on_event: impl FnMut(AntigravityAuthEvent) + Send,
) -> Result<(), String> {
    halo_harness::antigravity_acp_run_login(custom_executable, on_event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_is_explicit_and_question_scoped() {
        let answer = |question: &str, labels: &[&str]| UserInputAnswer {
            question_id: question.into(),
            labels: labels.iter().map(|s| (*s).into()).collect(),
        };
        assert!(confirmed("request", &[answer("request", &["Allow"])]));
        assert!(!confirmed("request", &[]));
        assert!(!confirmed("request", &[answer("other", &["Allow"])]));
        assert!(!confirmed(
            "request",
            &[answer("request", &["Allow", "Block"])]
        ));
        assert!(!confirmed("request", &[answer("request", &["Block"])]));
    }

    #[test]
    fn pi_vision_capability_tracks_known_providers() {
        assert_eq!(
            pi_supports_images("anthropic/claude-sonnet-5-5"),
            Some(true)
        );
        assert_eq!(pi_supports_images("openai/o3-mini"), Some(false));
        assert_eq!(pi_supports_images("deepseek/deepseek-v4-pro"), Some(false));
        // Local models can be vision-capable; do not block what we cannot see.
        assert_eq!(pi_supports_images("ollama"), None);
    }

    #[test]
    fn interrupt_removes_starting_registration_and_only_its_inputs() {
        let app = AppHandle::new();
        ensure_managed(&app);
        let state = app.state::<HaloAgentState>();
        for chat in ["a", "b"] {
            state.active.lock().insert(
                chat.into(),
                ActiveRun {
                    run_id: "run".into(),
                    handle: None,
                },
            );
            state
                .pending_inputs
                .lock()
                .insert(input_key(chat, "run", "request"), ());
        }
        interrupt_chat(&app, "a");
        assert_eq!(active_run_id(&app, "a"), None);
        assert_eq!(active_run_id(&app, "b").as_deref(), Some("run"));
        assert_eq!(state.pending_inputs.lock().len(), 1);
        assert!(
            state
                .pending_inputs
                .lock()
                .contains_key(&input_key("b", "run", "request"))
        );
    }
}

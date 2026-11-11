//! Bootstrap (`install`, `install_native`), the ambient state handle and proxy start/stop plumbing.

use std::time::Duration;

use super::util::*;
use super::*;

impl Bridge {
    /// Install ambient backend + [`BridgeState`] store. Call once in `main` before opening windows.
    pub fn install(backend: impl Backend + 'static, cx: &mut App) -> StoreHandle<BridgeState> {
        Self::coder_install_quit_flush(cx);
        let backend: Arc<dyn Backend> = Arc::new(backend);
        let mut state = BridgeState::default();
        // Stub fallback only: the boot error keeps "agent unavailable" from
        // looking like an agent bug. `None` for the real backend.
        state.backend_failure = backend.degraded_reason();
        state.proxy = backend.proxy_status();
        if let Ok(rows) = backend.history_recent(HISTORY_LIMIT, None) {
            state.history = rows;
        }
        if let Ok(sessions) = backend.sessions_list() {
            state.sessions = sessions;
        }
        if let Ok(enabled) = backend.intercept_enabled() {
            let paused = backend.intercept_paused().unwrap_or_default();
            state.intercept.enabled = enabled;
            state.intercept.paused_count = paused.len();
            state.paused = paused;
        }
        // Collections + folders: migrate orphans into Default before seeding state.
        let _ = Bridge::ensure_collections_migrated(&backend);
        if let Ok(collections) = backend.get_collections() {
            state.collections = collections;
            state.active_collection_id = state.collections.first().map(|c| c.id.clone());
        }
        if let Ok(stashes) = backend.get_stashes() {
            state.stashes = stashes;
        }
        if let Ok(endpoints) = backend.get_stash_endpoints() {
            state.endpoints = endpoints;
        }
        if let Ok(contexts) = backend.get_contexts() {
            state.contexts = contexts;
            if state.active_context_id.is_none() {
                state.active_context_id = state.contexts.first().map(|c| c.id.clone());
            }
        }
        if let Ok(chronicle) = backend.get_chronicle_logs(100) {
            state.chronicle = chronicle;
        }
        if let Ok(storage) = backend.get_storage_info() {
            state.storage = Some(storage);
        }
        if let Ok(projects) = backend.coder_recent_projects() {
            state.coder_projects = projects;
        }
        if let Ok(Some(persisted)) = backend.coder_load_settings() {
            state.coder_settings = coder_settings_from_persisted(persisted);
            // Template for the first empty session slot before chats load.
            state.coder.agent_approval_mode = state.coder_settings.agent_approval_mode;
        }
        // A corrupt prefs file was quarantined and reset during the read above;
        // surface it instead of letting settings silently revert to defaults.
        if let Some(message) = halo_backend::commands::coder::take_settings_load_error() {
            state.apply_notice(message);
        }
        let mut coder_seed = None;
        if let Ok(Some(root)) = backend.coder_saved_root() {
            if let Ok(entries) = backend.coder_list_dir(&root) {
                let persisted = backend.coder_load_open_tabs(&root).unwrap_or_default();
                let mut work = coder_work_from_listing(root, entries);
                coder_apply_persisted_tabs(&mut work, &persisted, backend.as_ref());
                // The listing starts at Ask. New empty chats inherit the
                // settings default; saved chats replace this when they load.
                work.agent_approval_mode = state.coder_settings.agent_approval_mode;
                state.coder = work.clone();
                coder_seed = Some(work);
            }
        }
        // Restore saved harness (Pi or OpenCode); default to Pi.
        if let Ok(models) = backend.coder_load_agent_models() {
            state.coder.agent_model_by_harness = models;
        }
        // Hydrate the per-harness model lists from disk so the first picker
        // open after relaunch does not wait on a CLI spawn.
        if let Ok((catalogs, fetched_at)) = backend.coder_load_agent_model_catalogs() {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            for (harness, models) in catalogs {
                if let Some(secs) = fetched_at.get(&harness).copied() {
                    let age = Duration::from_secs(now_secs.saturating_sub(secs));
                    if let Some(at) = std::time::Instant::now().checked_sub(age) {
                        state
                            .coder
                            .agent_catalog_fetched_at
                            .insert(harness.clone(), at);
                    }
                }
                state
                    .coder
                    .agent_model_cache
                    .insert(harness, models);
            }
        }
        if let Ok((harness, model)) = backend.coder_load_agent_selection() {
            let harness_id = harness
                .as_deref()
                .and_then(halo_backend::commands::halo_agent::HarnessId::parse)
                .unwrap_or(halo_backend::commands::halo_agent::HarnessId::Pi);
            state.coder.agent_harness = harness_id.as_str().into();
            if let Some(model) = model {
                state.coder.agent_model = Some(match harness_id {
                    halo_backend::commands::halo_agent::HarnessId::Pi
                    | halo_backend::commands::halo_agent::HarnessId::Cursor => {
                        halo_backend::commands::halo_agent::migrate_model_id(&model)
                    }
                    halo_backend::commands::halo_agent::HarnessId::OpenCode
                    | halo_backend::commands::halo_agent::HarnessId::Antigravity => model,
                });
            }
        }
        if let Some(cached) = state
            .coder
            .agent_model_cache
            .get(&state.coder.agent_harness)
            .cloned()
        {
            state.coder.agent_model_catalog = cached;
        }
        let pi_custom = state.coder_settings.pi_executable_path.as_deref();
        let oc_custom = state.coder_settings.opencode_executable_path.as_deref();
        let ag_custom = state.coder_settings.antigravity_executable_path.as_deref();
        let selected = state.coder.agent_harness.clone();
        let (installed, pi_needs_upgrade, state_blocked, setup_hint) = match selected.as_str() {
            "opencode" => {
                let probe = backend
                    .halo_agent_probe_opencode(oc_custom)
                    .unwrap_or_else(|e| {
                        halo_backend::commands::halo_agent::OpenCodeProbeStatus::Failed {
                            message: e.to_string(),
                        }
                    });
                (
                    probe.is_ready(),
                    probe.needs_upgrade(),
                    probe.is_state_blocked(),
                    if probe.is_ready() {
                        None
                    } else {
                        Some(probe.setup_hint())
                    },
                )
            }
            "cursor" => {
                let (ready, hint) = halo_backend::commands::halo_agent::cursor_session_ready();
                (ready, false, false, hint)
            }
            "antigravity" => {
                let probe = backend
                    .halo_agent_probe_antigravity(ag_custom)
                    .unwrap_or_else(|e| {
                        halo_backend::commands::halo_agent::AntigravityProbeStatus::Failed {
                            message: e.to_string(),
                        }
                    });
                (probe.is_ready(), false, false, {
                    if probe.is_ready() {
                        None
                    } else {
                        Some(probe.setup_hint())
                    }
                })
            }
            _ => {
                let pi_probe = backend.halo_agent_probe_pi(pi_custom).unwrap_or_else(|e| {
                    halo_backend::commands::halo_agent::PiProbeStatus::Failed {
                        message: e.to_string(),
                    }
                });
                (
                    pi_probe.is_ready(),
                    pi_probe.needs_upgrade(),
                    false,
                    if pi_probe.is_ready() {
                        None
                    } else {
                        Some(pi_probe.setup_hint())
                    },
                )
            }
        };
        state.coder.agent_harness_installed = installed;
        state.coder.agent_pi_needs_upgrade = pi_needs_upgrade;
        state.coder.agent_state_blocked = state_blocked;
        state.coder.agent_pi_setup_hint = setup_hint;
        if let Some(seed) = coder_seed.as_mut() {
            seed.agent_harness = state.coder.agent_harness.clone();
            seed.agent_model = state.coder.agent_model.clone();
            seed.agent_model_by_harness = state.coder.agent_model_by_harness.clone();
            seed.agent_harness_installed = state.coder.agent_harness_installed;
        }
        let mut pins_migrated = false;
        if let Ok(layout) = backend.shell_load_layout() {
            // Typography is not part of the tab layout: `apply_shell_layout`
            // bails out when there are no tabs to restore, so read it first.
            state.appearance = layout.appearance.clone();
            // Same for the desktop pins: `None` keeps the seeded default
            // (every app pinned), `Some` is an explicit user choice.
            let pinned = layout
                .pinned_apps
                .clone()
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| WorkKind::from_str(id))
                        .filter(|kind| kind.ships())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    WorkKind::ALL
                        .into_iter()
                        .filter(|kind| kind.ships())
                        .collect()
                });
            // Rewrite once when a saved pin list still named the retired
            // top-level Terminal app (filtered out by `from_str` above).
            if layout
                .pinned_apps
                .as_ref()
                .is_some_and(|ids| ids.iter().any(|id| id == "terminal"))
            {
                pins_migrated = true;
            }
            state.pinned_apps = pinned;
            // Onboarding is independent of tab restore (empty layout still
            // carries the first-run flag).
            state.onboarding_completed = layout.onboarding_completed;
            state.onboarding_step = layout.onboarding_step;
            apply_shell_layout(&mut state, layout, coder_seed.as_ref());
        }
        let restored_kind = state.active_work_kind();
        cx.set_global(BackendSlot(backend));
        let handle = StoreContext::<BridgeState>::provide(state, cx)
            .store()
            .clone();
        // After BackendSlot is live: restore durable agent chats into the
        // flat projection (and sync into Coder work tabs).
        Self::halo_agent_load_persisted(cx);
        // Drop remembered projects / chats whose folders were deleted on disk.
        let _ = Self::halo_agent_prune_missing_projects(cx);
        // One-time pin rewrite after dropping the retired Terminal app.
        if pins_migrated {
            Self::persist_shell_layout(cx);
        }
        match restored_kind {
            Some(WorkKind::HttpHistory) => {
                let _ = Self::bind_http_tab_session(cx);
            }
            Some(WorkKind::Intercept) => {
                let _ = Self::sync_active_intercept_scope(cx);
            }
            Some(WorkKind::Coder) | Some(WorkKind::Agent) => {
                let _ = Self::coder_restore_workspace(cx);
            }
            Some(WorkKind::SourceControl) => {
                let _ = Self::coder_restore_workspace(cx);
                let _ = Self::git_refresh(cx);
            }
            Some(WorkKind::Doctor) => {
                Self::doctor_refresh(cx);
            }
            _ => {}
        }
        handle
    }

    /// Install [`NativeBackend`] and pump `proxy-record` / status events into [`BridgeState`].
    pub fn install_native(backend: NativeBackend, cx: &mut App) -> StoreHandle<BridgeState> {
        let tx = super::ingress::Ingress::new();
        let ingress = tx.clone();
        backend.on_backend_event(move |event, payload| match event {
            "proxy-record" => {
                if let Ok(summary) =
                    serde_json::from_value::<halo_backend::ProxyLogSummary>(payload.clone())
                {
                    let _ = tx.send(BackendEvent::ProxyRecord(summary_to_row(summary)));
                }
            }
            "proxy-status-changed" => {
                if let Ok(status) = serde_json::from_value::<serde_json::Value>(payload.clone()) {
                    let running = status
                        .get("running")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let port = status
                        .get("port")
                        .and_then(|v| v.as_u64())
                        .map(|p| p as u16);
                    let default_port = status
                        .get("default_port")
                        .and_then(|v| v.as_u64())
                        .map(|p| p as u16)
                        .unwrap_or(8888);
                    let _ = tx.send(BackendEvent::ProxyStatusChanged(ProxyStatus {
                        running,
                        port,
                        tls_port: None,
                        default_port,
                        connections: 0,
                    }));
                }
            }
            "mock-forge-log" => {
                if let Ok(log) = serde_json::from_value::<MockLogDto>(payload.clone()) {
                    let _ = tx.send(BackendEvent::MockLog(log));
                }
            }
            "mock-server-status-changed" => {
                if let Ok(status) = serde_json::from_value::<MockServerStatusDto>(payload.clone()) {
                    let _ = tx.send(BackendEvent::MockServerStatus(status));
                }
            }
            name if name.starts_with("port-scan-result-") => {
                let scan_id = name.trim_start_matches("port-scan-result-").to_string();
                if let Ok(row) = serde_json::from_value::<PortScanResultDto>(payload.clone()) {
                    let _ = tx.send(BackendEvent::PortScanResult { scan_id, row });
                }
            }
            name if name.starts_with("port-scan-progress-") => {
                let scan_id = name.trim_start_matches("port-scan-progress-").to_string();
                let kind = payload
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Update")
                    .to_string();
                let current = payload.get("current").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let total = payload.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let _ = tx.send(BackendEvent::PortScanProgress {
                    scan_id,
                    kind,
                    current,
                    total,
                });
            }
            name if name.starts_with("invoker-result-") => {
                if let Ok(row) = parse_invoker_result(payload) {
                    let _ = tx.send(BackendEvent::InvokerResult(row));
                }
            }
            name if name.starts_with("invoker-progress-") => {
                use halo_backend::commands::intruder::InvokerProgress;
                if let Ok(progress) = serde_json::from_value::<InvokerProgress>(payload.clone()) {
                    match progress {
                        InvokerProgress::Update { current, total } => {
                            let _ = tx.send(BackendEvent::InvokerProgress { current, total });
                        }
                        InvokerProgress::Complete => {
                            let _ = tx.send(BackendEvent::InvokerComplete);
                        }
                    }
                }
            }
            "crawler:session-started" | "crawler:session-updated" | "crawler:session-finished" => {
                if let Ok(session) =
                    serde_json::from_value::<halo_backend::crawler::CrawlSession>(payload.clone())
                {
                    let _ = tx.send(BackendEvent::CrawlerSession(CrawlerSessionDto {
                        id: session.id,
                        target_url: session.target_url,
                        status: session.status,
                        strategy: session.strategy,
                        max_depth: session.max_depth,
                        max_pages: session.max_pages,
                        started_at: session.started_at,
                        finished_at: session.finished_at,
                    }));
                }
            }
            "crawler:page-discovered" | "crawler:page-updated" => {
                if let Ok(page) =
                    serde_json::from_value::<halo_backend::crawler::CrawlPage>(payload.clone())
                {
                    let _ = tx.send(BackendEvent::CrawlerPage(CrawlerPageDto {
                        id: page.id,
                        session_id: page.session_id,
                        url: page.url,
                        title: page.title,
                        status: page.status,
                        depth: page.depth,
                        parent_url: page.parent_url,
                        http_status: page.http_status,
                        links_found: page.links_found,
                        forms_found: page.forms_found,
                        discovered_at: page.discovered_at,
                        visited_at: page.visited_at,
                        interesting: page.interesting.unwrap_or(false),
                    }));
                }
            }
            "crawler:insight-created" => {
                if let Ok(insight) =
                    serde_json::from_value::<halo_backend::crawler::AIInsight>(payload.clone())
                {
                    let _ = tx.send(BackendEvent::CrawlerInsight(CrawlerInsightDto {
                        id: insight.id,
                        session_id: insight.session_id,
                        page_id: insight.page_id,
                        severity: insight.severity,
                        finding_type: insight.r#type,
                        title: insight.title,
                        description: insight.description,
                        url: insight.url,
                        ai_used: insight.ai_used_for_analysis.unwrap_or(false),
                        analysis_source: insight.analysis_source,
                        reviewed: insight.reviewed,
                        created_at: insight.created_at,
                    }));
                }
            }
            "crawler:log-created" => {
                if let Ok(log) =
                    serde_json::from_value::<halo_backend::crawler::ActivityLog>(payload.clone())
                {
                    let _ = tx.send(BackendEvent::CrawlerLog(CrawlerLogDto {
                        id: log.id,
                        session_id: log.session_id,
                        level: log.level,
                        log_type: log.r#type,
                        message: log.message,
                        url: log.url,
                        created_at: log.created_at,
                    }));
                }
            }
            "crawler:session-failed" => {
                let session_id = payload
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let message = payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Crawl failed")
                    .to_string();
                let _ = tx.send(BackendEvent::CrawlerSessionFailed {
                    session_id,
                    message,
                });
            }
            "coder-pty-out" => {
                let session_id = payload
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let data = payload
                    .get("data")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if !session_id.is_empty() && !data.is_empty() {
                    let _ = tx.send(BackendEvent::CoderPtyOutput { session_id, data });
                }
            }
            "coder-pty-exit" => {
                let session_id = payload
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let generation = payload
                    .get("generation")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if !session_id.is_empty() {
                    let _ = tx.send(BackendEvent::CoderPtyExit {
                        session_id,
                        generation,
                    });
                }
            }
            "r2-upload-progress" => {
                if let Ok(progress) = serde_json::from_value::<
                    halo_backend::commands::r2::R2UploadProgress,
                >(payload.clone())
                {
                    let _ = tx.send(BackendEvent::R2UploadProgress(progress));
                }
            }
            "coder-agent-event" => {
                if let Ok(payload) = serde_json::from_value::<
                    halo_backend::commands::halo_agent::AgentEventPayload,
                >(payload.clone())
                {
                    let _ = tx.send(BackendEvent::HaloAgentEvent {
                        run_id: payload.run_id,
                        chat_id: payload.chat_id,
                        event: payload.event,
                    });
                }
            }
            _ => {}
        });

        let handle = Self::install(backend, cx);
        Self::pump_events(ingress, cx);
        handle
    }

    pub(super) fn pump_events(ingress: super::ingress::Ingress, cx: &mut App) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            loop {
                let batch = ingress.drain_budget();
                if batch.is_empty() {
                    // Subscribe before the second pending check so a push cannot
                    // land in the gap and leave this task asleep.
                    let wake = ingress.wait();
                    if !ingress.pending() {
                        wake.await;
                    }
                    continue;
                }
                let done_chats: Vec<String> = batch
                    .iter()
                    .filter_map(|event| match event {
                        BackendEvent::HaloAgentEvent {
                            chat_id,
                            event: halo_backend::commands::halo_agent::AgentEvent::Done { .. },
                            ..
                        } => Some(chat_id.clone()),
                        _ => None,
                    })
                    .collect();
                // Apply only — never SQLite upsert or queue dispatch inside the
                // apply update, or agent streams starve theme/clicks for the
                // whole frame. Persist + drain run after apply returns.
                let drains: Vec<(String, String, bool)> = cx.update(|cx| {
                    let drains = Self::state(cx).update(cx, |s| {
                        s.apply_events(batch);
                        s.take_agent_queue_drains()
                    });
                    if !done_chats.is_empty() {
                        Self::halo_agent_persist_chats_async(&done_chats, true, cx);
                    }
                    drains
                });
                if !drains.is_empty() {
                    let _ = cx.update(|cx| {
                        for (chat_id, item_id, steer) in &drains {
                            let _ = Self::halo_agent_dispatch_queue_item(
                                chat_id, item_id, *steer, cx,
                            );
                        }
                    });
                }
                // Always yield a frame after applying. Agent streams can otherwise
                // monopolize the UI thread (theme toggle, clicks, layout) — 1ms was
                // not enough once apply/repaint cost dominates. A longer pause under
                // backlog also lets ingress coalesce more deltas before the next paint.
                let yield_ms = if ingress.pending() { 16 } else { 8 };
                executor.timer(Duration::from_millis(yield_ms)).await;
            }
        })
        .detach();
    }

    /// Reactive state handle (panics if [`Bridge::install`] was never called).
    pub fn state(cx: &App) -> StoreHandle<BridgeState> {
        StoreContext::<BridgeState>::require(cx).store().clone()
    }

    pub(super) fn backend(cx: &App) -> BridgeResult<Arc<dyn Backend>> {
        cx.try_global::<BackendSlot>()
            .map(|slot| slot.0.clone())
            .ok_or(BridgeError::NotInstalled)
    }

    pub(super) fn sync_proxy_from_backend(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let status = backend.proxy_status();
        Self::state(cx).update(cx, |s| s.proxy = status);
        Ok(())
    }

    pub fn proxy_status(cx: &App) -> ProxyStatus {
        Self::state(cx).read(cx).proxy.clone()
    }

    /// Run blocking backend work off the UI thread, then apply the result.
    pub(super) fn run_blocking<T, F, A>(cx: &App, work: F, apply: A)
    where
        T: Send + 'static,
        F: FnOnce() -> BridgeResult<T> + Send + 'static,
        A: FnOnce(BridgeResult<T>, &mut App) + 'static,
    {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let (tx, rx) = std::sync::mpsc::channel();
            let spawn_ok = std::thread::Builder::new()
                .name("halo-bridge".into())
                .spawn(move || {
                    let _ = tx.send(work());
                })
                .is_ok();

            let result = if !spawn_ok {
                Err(BridgeError::message("failed to spawn bridge worker"))
            } else {
                loop {
                    match rx.try_recv() {
                        Ok(result) => break result,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            executor.timer(Duration::from_millis(8)).await;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            break Err(BridgeError::message("bridge worker died"));
                        }
                    }
                }
            };

            let _ = cx.update(|cx| apply(result, cx));
        })
        .detach();
    }
}

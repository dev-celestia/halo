//! Backend trait — implemented by stub (Track 1) and native (Track 2+).

use crate::error::BridgeResult;
use crate::types::{
    ChronicleLogDto, CollectionDto, ContextDto, CrawlerCrawlConfig, CrawlerInsightDto,
    CrawlerLogDto, CrawlerPageDto, CrawlerSessionDto, DeleteArtifactResult, HistoryFilter,
    HttpLogDetail, HttpLogRow, HttpSessionDto, IntruderDraft, IntruderStatus, NoteRecord,
    PausedRow, ProxyStatus, RepeaterRequestDto, RepeaterResponseDto, ResetLocalDataResult,
    StashDto, StashEndpointDto, StorageInfo,
};
use crate::work::CoderDirEntry;


pub trait BackendProxy: Send + Sync {
    // ── Proxy ──────────────────────────────────────────────────────────────
    fn proxy_status(&self) -> ProxyStatus;
    fn start_proxy(&self, port: u16, tls_port: u16) -> BridgeResult<String>;
    fn stop_proxy(&self) -> BridgeResult<String>;
}

pub trait BackendHistory: Send + Sync {
    // ── HTTP History ───────────────────────────────────────────────────────
    fn history_recent(
        &self,
        limit: u32,
        filter: Option<&HistoryFilter>,
    ) -> BridgeResult<Vec<HttpLogRow>>;
    fn history_detail(&self, log_id: &str) -> BridgeResult<HttpLogDetail>;
    fn history_delete(&self, log_id: &str) -> BridgeResult<()>;
    fn history_clear(&self) -> BridgeResult<()>;

    // ── HTTP Sessions ──────────────────────────────────────────────────────
    fn sessions_list(&self) -> BridgeResult<Vec<HttpSessionDto>>;
    fn session_create(&self, name: &str) -> BridgeResult<HttpSessionDto>;
    fn session_set_active(&self, session_id: &str) -> BridgeResult<()>;
    fn session_clear_logs(&self, session_id: &str) -> BridgeResult<usize>;
}

pub trait BackendIntercept: Send + Sync {
    // ── Intercept ──────────────────────────────────────────────────────────
    fn intercept_enabled(&self) -> BridgeResult<bool>;
    fn set_intercept_enabled(&self, enabled: bool) -> BridgeResult<()>;
    fn intercept_paused(&self) -> BridgeResult<Vec<PausedRow>>;
    fn intercept_scope(&self) -> BridgeResult<Vec<String>>;
    fn intercept_set_scope(&self, tab_id: &str, patterns: &[String]) -> BridgeResult<()>;
    fn intercept_add_scope(&self, pattern: &str) -> BridgeResult<Vec<String>>;
    fn intercept_remove_scope(&self, pattern: &str) -> BridgeResult<Vec<String>>;
    fn intercept_forward(&self, id: &str) -> BridgeResult<()>;
    fn intercept_forward_modified(
        &self,
        id: &str,
        raw_text: &str,
        intercept_response: bool,
    ) -> BridgeResult<()>;
    fn intercept_drop(&self, id: &str) -> BridgeResult<()>;
    fn intercept_forward_all(&self) -> BridgeResult<()>;
    fn intercept_drop_all(&self) -> BridgeResult<()>;
    fn intercept_forward_tab(&self, tab_id: &str) -> BridgeResult<usize>;
    fn intercept_add_host_tab(&self, host: &str) -> BridgeResult<()>;
    fn intercept_open_browser(&self) -> BridgeResult<()>;
    fn intercept_trust_ca(&self) -> BridgeResult<String>;
}

pub trait BackendRepeater: Send + Sync {
    // ── Repeater / Forge ───────────────────────────────────────────────────
    fn repeater_send(&self, request: RepeaterRequestDto) -> BridgeResult<RepeaterResponseDto>;

    // ── Folders (collections / stashes / endpoints / contexts / chronicle) ─
    fn get_collections(&self) -> BridgeResult<Vec<CollectionDto>>;
    fn save_collection(&self, collection: CollectionDto) -> BridgeResult<()>;
    fn delete_collection(&self, id: &str) -> BridgeResult<()>;
    fn get_stashes(&self) -> BridgeResult<Vec<StashDto>>;
    fn save_stash(&self, stash: StashDto) -> BridgeResult<()>;
    fn delete_stash(&self, id: &str) -> BridgeResult<()>;
    fn get_stash_endpoints(&self) -> BridgeResult<Vec<StashEndpointDto>>;
    fn save_stash_endpoint(&self, endpoint: StashEndpointDto) -> BridgeResult<()>;
    fn delete_stash_endpoint(&self, id: &str) -> BridgeResult<()>;
    fn get_contexts(&self) -> BridgeResult<Vec<ContextDto>>;
    fn save_context(&self, context: ContextDto) -> BridgeResult<()>;
    fn delete_context(&self, id: &str) -> BridgeResult<()>;
    fn get_chronicle_logs(&self, limit: u32) -> BridgeResult<Vec<ChronicleLogDto>>;
    fn add_chronicle_log(&self, record: ChronicleLogDto) -> BridgeResult<()>;
    fn clear_chronicle_logs(&self) -> BridgeResult<()>;
}

pub trait BackendIntruder: Send + Sync {
    // ── Intruder ───────────────────────────────────────────────────────────
    fn intruder_start(&self, draft: IntruderDraft) -> BridgeResult<IntruderStatus>;
    fn intruder_stop(&self) -> BridgeResult<IntruderStatus>;

    // ── Browser automation / crawler ───────────────────────────────────────
    fn crawler_start_crawl(&self, config: CrawlerCrawlConfig) -> BridgeResult<CrawlerSessionDto>;
    fn crawler_pause_crawl(&self, session_id: &str) -> BridgeResult<()>;
    fn crawler_resume_crawl(&self, session_id: &str) -> BridgeResult<()>;
    fn crawler_stop_crawl(&self, session_id: &str) -> BridgeResult<()>;
    fn crawler_delete_session(&self, session_id: &str) -> BridgeResult<()>;
    fn crawler_list_pages(&self, session_id: &str) -> BridgeResult<Vec<CrawlerPageDto>>;
    fn crawler_list_insights(&self, session_id: &str) -> BridgeResult<Vec<CrawlerInsightDto>>;
    fn crawler_list_logs(&self, session_id: &str) -> BridgeResult<Vec<CrawlerLogDto>>;
}

pub trait BackendCoder: Send + Sync {
    // ── Coder IDE filesystem ───────────────────────────────────────────────
    fn coder_list_dir(&self, path: &str) -> BridgeResult<Vec<CoderDirEntry>>;
    fn coder_read_file(&self, path: &str) -> BridgeResult<String>;
    /// Workspace-confined check that an image file exists and is within size limits.
    fn coder_ensure_image_file(&self, path: &str) -> BridgeResult<()>;
    fn coder_write_file(&self, path: &str, contents: &str) -> BridgeResult<()>;
    fn coder_create_file(&self, path: &str) -> BridgeResult<()>;
    fn coder_create_dir(&self, path: &str) -> BridgeResult<()>;
    fn coder_rename_path(&self, from: &str, to: &str) -> BridgeResult<()>;
    fn coder_delete_path(&self, path: &str) -> BridgeResult<()>;
    /// Soft-delete into app trash; returns the trash path for later restore.
    fn coder_trash_path(&self, path: &str) -> BridgeResult<String>;
    fn coder_purge_trash(&self, trash_path: &str) -> BridgeResult<()>;
    fn coder_copy_path(&self, from: &str, to: &str) -> BridgeResult<()>;
    fn coder_unique_sibling(&self, path: &str) -> BridgeResult<String>;
    /// Last opened project folder, if still valid on disk.
    fn coder_saved_root(&self) -> BridgeResult<Option<String>>;
    fn coder_save_root(&self, root_path: Option<&str>) -> BridgeResult<()>;
    /// Recently opened project folders (most recent first).
    fn coder_recent_projects(&self) -> BridgeResult<Vec<String>>;
    /// Drop a folder from the recent-projects list (files on disk stay).
    fn coder_remove_recent_project(&self, path: &str) -> BridgeResult<()>;
    /// Persisted editor tab strip for a project root.
    fn coder_load_open_tabs(
        &self,
        root: &str,
    ) -> BridgeResult<halo_backend::commands::coder::CoderPersistedTabs>;
    fn coder_save_open_tabs(
        &self,
        root: &str,
        tabs: &halo_backend::commands::coder::CoderPersistedTabs,
    ) -> BridgeResult<()>;
    /// Shared IDE settings (font, shell, agent, …) from `coder.json`.
    fn coder_load_settings(
        &self,
    ) -> BridgeResult<Option<halo_backend::commands::coder::CoderPersistedSettings>>;
    fn coder_save_settings(
        &self,
        settings: &halo_backend::commands::coder::CoderPersistedSettings,
    ) -> BridgeResult<()>;
    /// Last harness + model from the agent panel / settings.
    fn coder_load_agent_selection(&self) -> BridgeResult<(Option<String>, Option<String>)>;
    /// Last model **per harness**, so a switch restores that harness's own pick.
    fn coder_load_agent_models(&self) -> BridgeResult<std::collections::HashMap<String, String>>;
    fn coder_save_agent_selection(&self, harness: &str, model: Option<&str>) -> BridgeResult<()>;
    /// Last probed model lists per harness (+ unix fetch times) for instant picker opens.
    fn coder_load_agent_model_catalogs(
        &self,
    ) -> BridgeResult<(
        std::collections::HashMap<String, Vec<halo_backend::commands::halo_agent::ModelInfo>>,
        std::collections::HashMap<String, u64>,
    )>;
    fn coder_save_agent_model_catalog(
        &self,
        harness: &str,
        models: &[halo_backend::commands::halo_agent::ModelInfo],
    ) -> BridgeResult<()>;

    /// Hot-exit: write a dirty buffer backup (outside the project tree).
    fn coder_write_backup(&self, workspace: &str, path: &str, content: &str) -> BridgeResult<()>;
    fn coder_delete_backup(&self, workspace: &str, path: &str) -> BridgeResult<()>;
    fn coder_load_backup(
        &self,
        workspace: &str,
        path: &str,
        disk_content: &str,
    ) -> BridgeResult<Option<halo_backend::commands::coder_backup::BackupRestore>>;
    fn coder_push_history(&self, path: &str, previous_content: &str) -> BridgeResult<()>;
    fn coder_list_history(
        &self,
        path: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_backup::HistoryEntry>>;
    fn coder_read_history(&self, path: &str, id: &str) -> BridgeResult<String>;

    /// Persisted work-tab strip + activity sessions.
    fn shell_load_layout(&self) -> BridgeResult<halo_backend::commands::shell::ShellLayout>;
    fn shell_save_layout(
        &self,
        layout: &halo_backend::commands::shell::ShellLayout,
    ) -> BridgeResult<()>;
}

pub trait BackendGit: Send + Sync {
    // ── Coder Git ──────────────────────────────────────────────────────────
    fn git_status(&self, workspace: &str) -> BridgeResult<crate::work::GitState>;
    /// Read-only `git log` text for the terminal's Git Log tab.
    fn git_log(&self, workspace: &str) -> BridgeResult<String>;
    fn git_stage(&self, workspace: &str, paths: &[String]) -> BridgeResult<()>;
    fn git_unstage(&self, workspace: &str, paths: &[String]) -> BridgeResult<()>;
    fn git_stage_all(&self, workspace: &str) -> BridgeResult<()>;
    fn git_unstage_all(&self, workspace: &str) -> BridgeResult<()>;
    fn git_discard(&self, workspace: &str, path: &str) -> BridgeResult<()>;
    /// Unified diff for one path (`staged` = index vs HEAD).
    fn git_diff(&self, workspace: &str, path: &str, staged: bool) -> BridgeResult<String>;
    /// Apply one hunk to the index (`reverse` unstages it).
    fn git_apply_hunk(&self, workspace: &str, patch: &str, reverse: bool) -> BridgeResult<String>;
    fn git_commit(&self, workspace: &str, message: &str) -> BridgeResult<String>;
    fn git_amend(&self, workspace: &str, message: &str) -> BridgeResult<String>;
    /// Switch to an existing branch (or create a tracking branch from a remote).
    fn git_checkout(&self, workspace: &str, branch: &str) -> BridgeResult<String>;
    /// Stash local changes, then switch to `branch` (recovery from a refused
    /// checkout). The edits stay in the stash list.
    fn git_stash_and_checkout(&self, workspace: &str, branch: &str) -> BridgeResult<String>;
    /// Create a branch at HEAD and switch to it.
    fn git_create_branch(&self, workspace: &str, name: &str) -> BridgeResult<String>;
    /// Create a branch from `base` and switch to it.
    fn git_create_branch_from(
        &self,
        workspace: &str,
        name: &str,
        base: &str,
    ) -> BridgeResult<String>;
    /// Rename a branch (the current branch included).
    fn git_rename_branch(&self, workspace: &str, from: &str, to: &str) -> BridgeResult<String>;
    fn git_push(&self, workspace: &str) -> BridgeResult<String>;
    fn git_push_to(&self, workspace: &str, remote: &str) -> BridgeResult<String>;
    fn git_push_to_branch(
        &self,
        workspace: &str,
        remote: &str,
        branch: &str,
    ) -> BridgeResult<String>;
    fn git_pull(&self, workspace: &str) -> BridgeResult<String>;
    fn git_fetch(&self, workspace: &str) -> BridgeResult<String>;
    fn git_sync(&self, workspace: &str) -> BridgeResult<String>;
    fn git_graph_log(
        &self,
        workspace: &str,
        max_count: usize,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitCommitDto>>;
    fn git_list_branches(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitBranchInfoDto>>;
    fn git_list_remotes(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitRemoteInfoDto>>;
    fn git_list_tags(&self, workspace: &str) -> BridgeResult<Vec<String>>;
    fn git_list_stashes(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitStashDto>>;
    fn git_list_worktrees(
        &self,
        workspace: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitWorktreeDto>>;
    fn git_commit_files(
        &self,
        workspace: &str,
        sha: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::coder_git::GitCommitFileDto>>;
    fn git_commit_file_diff(&self, workspace: &str, sha: &str, path: &str) -> BridgeResult<String>;
    fn git_delete_branch(&self, workspace: &str, branch: &str, force: bool)
        -> BridgeResult<String>;

    // ── Coder PTY terminal ─────────────────────────────────────────────────
    /// Spawn a shell; returns the session generation used to stamp exit events.
    fn coder_pty_start(
        &self,
        session_id: &str,
        cwd: &str,
        shell: Option<&str>,
        rows: u16,
        cols: u16,
    ) -> BridgeResult<u64>;
    fn coder_pty_write(&self, session_id: &str, data: &[u8]) -> BridgeResult<()>;
    fn coder_pty_resize(&self, session_id: &str, rows: u16, cols: u16) -> BridgeResult<()>;
    fn coder_pty_kill(&self, session_id: &str) -> BridgeResult<()>;
}

pub trait BackendAgent: Send + Sync {
    // ── Halo Agent ─────────────────────────────────────────────────
    fn halo_agent_list_harnesses(
        &self,
        pi_custom: Option<&str>,
        opencode_custom: Option<&str>,
        antigravity_custom: Option<&str>,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::HarnessInfo>>;
    fn halo_agent_list_models(
        &self,
        harness: &str,
        custom_executable: Option<&str>,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::ModelInfo>>;
    fn halo_agent_is_installed(
        &self,
        harness: &str,
        custom_executable: Option<&str>,
    ) -> BridgeResult<bool>;
    fn halo_agent_probe_pi(
        &self,
        custom_executable: Option<&str>,
    ) -> BridgeResult<halo_backend::commands::halo_agent::PiProbeStatus>;
    fn halo_agent_probe_opencode(
        &self,
        custom_executable: Option<&str>,
    ) -> BridgeResult<halo_backend::commands::halo_agent::OpenCodeProbeStatus>;
    fn halo_agent_probe_antigravity(
        &self,
        custom_executable: Option<&str>,
    ) -> BridgeResult<halo_backend::commands::halo_agent::AntigravityProbeStatus>;
    fn halo_agent_install_antigravity(&self) -> BridgeResult<String>;
    /// Browser tooling availability for the agent surface.
    fn halo_agent_browser_status(
        &self,
    ) -> BridgeResult<halo_backend::commands::halo_agent::BrowserStatus>;
    /// Tear down one chat's browser session.
    fn halo_agent_browser_close(&self, session: &str) -> BridgeResult<()>;
    /// Tear down one chat's session *and* erase its profile and downloads.
    fn halo_agent_browser_forget(&self, session: &str) -> BridgeResult<()>;
    /// Tear down every session in one call (quit, and the settings row).
    fn halo_agent_browser_close_all(&self) -> BridgeResult<()>;
    fn halo_agent_run(
        &self,
        request: &halo_backend::commands::halo_agent::RunRequest,
        launch: &halo_backend::commands::halo_agent::AgentLaunchContext,
    ) -> BridgeResult<String>;
    /// Apply the composer approval mode to the harness process already running.
    fn halo_agent_set_live_approval(
        &self,
        chat_id: &str,
        run_id: &str,
        mode: &str,
    ) -> BridgeResult<()>;
    fn halo_agent_respond_input(
        &self,
        chat_id: &str,
        run_id: &str,
        request_id: &str,
        answers: &[halo_backend::commands::halo_agent::UserInputAnswer],
    ) -> BridgeResult<()>;
    fn halo_agent_steer(
        &self,
        chat_id: &str,
        run_id: &str,
        command_id: &str,
        prompt: &str,
        attachments: &[String],
    ) -> BridgeResult<()>;
    /// Whether the live harness can accept a steer for this chat.
    fn halo_agent_can_steer(&self, chat_id: &str) -> BridgeResult<bool>;
    /// Stop one chat's harness. Other chats keep running.
    fn halo_agent_interrupt_chat(&self, chat_id: &str) -> BridgeResult<()>;
    /// Drop cached harness thread id for a chat (clear / archive / delete).
    fn halo_agent_forget_harness_session(&self, chat_id: &str) -> BridgeResult<()>;
    /// Stop every in-flight agent run.
    fn halo_agent_interrupt(&self) -> BridgeResult<()>;
    fn halo_agent_search_files(
        &self,
        root: &str,
        query: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::FileSearchMatch>>;
    fn halo_agent_list_skills(
        &self,
        root: &str,
        harness: &str,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::SkillInfo>>;
    /// Skill catalog for the composer. OpenCode is asked through a short-lived
    /// `opencode serve`; other harnesses (and failures) use the filesystem walk.
    fn halo_agent_discover_skills(
        &self,
        root: &str,
        harness: &str,
        opencode_executable: Option<&str>,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::SkillInfo>>;
    /// Load every durable coding-agent chat (all projects).
    fn halo_agent_list_sessions(&self) -> BridgeResult<Vec<halo_backend::HaloAgentSessionRecord>>;
    /// Upsert one chat transcript + metadata into `halo.db`.
    fn halo_agent_upsert_session(
        &self,
        session: &halo_backend::HaloAgentSessionRecord,
    ) -> BridgeResult<()>;
    /// Hard-delete one chat from `halo.db`.
    fn halo_agent_delete_persisted_session(&self, id: &str) -> BridgeResult<()>;
}

pub trait BackendStorage: Send + Sync {
    // ── Settings / CA / Storage ────────────────────────────────────────────
    fn get_storage_info(&self) -> BridgeResult<StorageInfo>;
    fn delete_storage_artifact(&self, artifact: &str) -> BridgeResult<DeleteArtifactResult>;
    fn reset_all_app_data(&self) -> BridgeResult<ResetLocalDataResult>;
    fn get_ca_cert(&self) -> BridgeResult<String>;
    /// Write the CA PEM to `path` and return the absolute path string.
    fn export_ca_cert(&self, path: &str) -> BridgeResult<String>;
    fn regenerate_ca_cert(&self) -> BridgeResult<()>;
    fn trust_ca(&self) -> BridgeResult<String>;

    // ── Notes ──────────────────────────────────────────────────────────────
    fn notes_list(&self) -> BridgeResult<Vec<NoteRecord>>;
    fn note_upsert(&self, note: &NoteRecord) -> BridgeResult<NoteRecord>;
    fn note_delete(&self, id: &str) -> BridgeResult<()>;
    fn notes_delete(&self, ids: &[String]) -> BridgeResult<usize>;
    fn notes_import(&self, notes: &[NoteRecord]) -> BridgeResult<usize>;
}

pub trait BackendMock: Send + Sync {
    // ── API Mock / API Override (MockForge) ────────────────────────────────
    fn mock_domains(&self) -> BridgeResult<Vec<crate::types::MockDomainDto>>;
    fn mock_add_domain(
        &self,
        hostname: &str,
        ssl: bool,
    ) -> BridgeResult<crate::types::MockDomainDto>;
    fn mock_toggle_domain(&self, id: &str) -> BridgeResult<()>;
    fn mock_delete_domain(&self, id: &str) -> BridgeResult<()>;
    fn mock_routes(&self) -> BridgeResult<Vec<crate::types::MockRouteDto>>;
    /// `domain_id` decides which page the rule belongs to (`local_mock_server`
    /// for API Mock, a captured host id for API Override).
    fn mock_add_route(
        &self,
        domain_id: &str,
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        chaos: crate::types::MockChaosDto,
    ) -> BridgeResult<crate::types::MockRouteDto>;
    fn mock_update_route(
        &self,
        id: &str,
        method: &str,
        path: &str,
        status_code: u16,
        response_body: &str,
        chaos: crate::types::MockChaosDto,
    ) -> BridgeResult<()>;
    fn mock_set_route_enabled(&self, id: &str, enabled: bool) -> BridgeResult<()>;
    fn mock_delete_route(&self, id: &str) -> BridgeResult<()>;
    fn mock_logs(&self) -> BridgeResult<Vec<crate::types::MockLogDto>>;
    fn mock_clear_logs(&self) -> BridgeResult<()>;
    fn mock_server_start(
        &self,
        port: u16,
        cors: bool,
    ) -> BridgeResult<crate::types::MockServerStatusDto>;
    fn mock_server_stop(&self) -> BridgeResult<crate::types::MockServerStatusDto>;
    fn mock_server_status(&self) -> BridgeResult<crate::types::MockServerStatusDto>;

    // ── Port Scanner ────────────────────────────────────────────────────────
    /// Blocking: runs the scan to completion (call from a worker thread).
    /// Results also stream via `port-scan-result-{scan_id}` events.
    fn port_scan_start(
        &self,
        request: &crate::types::PortScanRequestDto,
        scan_id: &str,
    ) -> BridgeResult<Vec<crate::types::PortScanResultDto>>;
    fn port_scan_stop(&self, scan_id: &str) -> BridgeResult<()>;
}

pub trait BackendR2: Send + Sync {
    // ── R2 Storage ─────────────────────────────────────────────────────────
    /// Network-backed: callers must run these off the UI thread. The secret
    /// access key never crosses this boundary.
    fn r2_credentials_status(
        &self,
    ) -> BridgeResult<Option<halo_backend::commands::r2::R2CredentialsStatus>>;
    fn r2_save_credentials(
        &self,
        account_id: String,
        access_key_id: String,
        secret_access_key: Option<String>,
        custom_endpoint_url: Option<String>,
    ) -> BridgeResult<()>;
    fn r2_clear_credentials(&self) -> BridgeResult<()>;
    fn r2_list_buckets(&self) -> BridgeResult<Vec<String>>;
    fn r2_list_objects(
        &self,
        bucket: String,
        prefix: String,
    ) -> BridgeResult<halo_backend::commands::r2::R2Listing>;
    fn r2_create_bucket(&self, name: String) -> BridgeResult<()>;
    fn r2_delete_bucket(&self, name: String) -> BridgeResult<()>;
    fn r2_create_folder(&self, bucket: String, key: String) -> BridgeResult<()>;
    fn r2_delete_object(&self, bucket: String, key: String) -> BridgeResult<()>;
    fn r2_upload_file(
        &self,
        bucket: String,
        key: String,
        source_path: String,
        content_type: Option<String>,
    ) -> BridgeResult<()>;
    fn r2_download_object(&self, bucket: String, key: String) -> BridgeResult<String>;
    fn r2_presign_url(
        &self,
        bucket: String,
        key: String,
        expires_seconds: u64,
    ) -> BridgeResult<String>;
}

/// Aggregate façade binding every backend area into one object-safe trait.
///
/// Each area lives in its own supertrait ([`BackendProxy`], [`BackendCoder`],
/// …) so `native/` and `stub/` implement one area per file instead of one
/// 2,000-line trait impl. Callers keep using `&dyn Backend` / `T: Backend` —
/// supertrait methods resolve through the area traits.
pub trait Backend:
    Send
    + Sync
    + BackendProxy
    + BackendHistory
    + BackendIntercept
    + BackendRepeater
    + BackendIntruder
    + BackendCoder
    + BackendGit
    + BackendAgent
    + BackendStorage
    + BackendMock
    + BackendR2
{
    /// Why this backend is running reduced, if it is.
    ///
    /// A native boot failure falls back to the stub backend; carrying the error
    /// here lets the UI say *what* failed instead of leaving the user with a
    /// bare "unavailable" next to a screen that otherwise looks normal.
    fn degraded_reason(&self) -> Option<String> {
        None
    }
}

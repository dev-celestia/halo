//! Doctor subsystem: health diagnostics, environment probes, and package/tool installers.
//!
//! Every failing row is meant to be fixable from the page. The action itself
//! lives in [`super::doctor_actions`] (deadlines, preflight, privilege
//! hand-off); this module owns the probes and the dispatch table, and it
//! re-probes after every action so a row can never turn green on a lie.

use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

use super::doctor_actions::{self, privileged_command};
use super::doctor_probes::*;
use super::*;
use crate::backend::Backend;
use crate::error::BridgeError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DoctorCategory {
    #[default]
    All,
    System,
    Harnesses,
    LanguageServers,
    BrowserProxy,
}

impl DoctorCategory {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::System,
        Self::Harnesses,
        Self::LanguageServers,
        Self::BrowserProxy,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::System => "system",
            Self::Harnesses => "harnesses",
            Self::LanguageServers => "lsp",
            Self::BrowserProxy => "browser-proxy",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All Checks",
            Self::System => "System & Bundle",
            Self::Harnesses => "AI Harnesses",
            Self::LanguageServers => "Language Servers",
            Self::BrowserProxy => "Browser & Proxy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoctorStatus {
    Ready,
    NeedsUpgrade,
    Missing,
    Failed,
}

impl DoctorStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::NeedsUpgrade => "Upgrade Needed",
            Self::Missing => "Missing",
            Self::Failed => "Failed",
        }
    }

    pub fn is_healthy(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorItem {
    pub id: String,
    pub category: DoctorCategory,
    pub name: String,
    pub description: String,
    pub command: String,
    pub status: DoctorStatus,
    pub version: Option<String>,
    pub resolved_path: Option<String>,
    pub hint: String,
    pub can_auto_install: bool,
    pub action_label: String,
    #[serde(default)]
    pub doc_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorLogEntry {
    pub timestamp: String,
    pub item_id: String,
    pub message: String,
    pub is_error: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DoctorView {
    pub items: Vec<DoctorItem>,
    pub filter: DoctorCategory,
    pub checking: bool,
    pub installing_ids: Vec<String>,
    pub batch_running: bool,
    pub logs: Vec<DoctorLogEntry>,
    pub last_checked_summary: Option<String>,
    /// Row whose action is armed and waiting for the confirming click. The
    /// second click is what makes a system change, so the confirmation lives on
    /// the button itself instead of in a modal that can swallow the click.
    pub confirm_armed: Option<String>,
}

impl DoctorView {
    pub const MAX_LOGS: usize = 300;

    pub fn push_log(
        &mut self,
        item_id: impl Into<String>,
        message: impl Into<String>,
        is_error: bool,
    ) {
        let time = Local::now().format("%H:%M:%S").to_string();
        self.logs.push(DoctorLogEntry {
            timestamp: time,
            item_id: item_id.into(),
            message: message.into(),
            is_error,
        });
        if self.logs.len() > Self::MAX_LOGS {
            let overflow = self.logs.len() - Self::MAX_LOGS;
            self.logs.drain(0..overflow);
        }
    }

    pub fn ready_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == DoctorStatus::Ready)
            .count()
    }

    pub fn upgrade_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == DoctorStatus::NeedsUpgrade)
            .count()
    }

    pub fn missing_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == DoctorStatus::Missing || i.status == DoctorStatus::Failed)
            .count()
    }

    pub fn is_installing(&self, id: &str) -> bool {
        self.installing_ids.iter().any(|i| i == id)
    }

    /// The row whose action is waiting for a second, confirming click.
    pub fn is_armed(&self, id: &str) -> bool {
        self.confirm_armed.as_deref() == Some(id)
    }

    /// Any scan or action in flight.
    ///
    /// The page uses this for *every* action button: while something runs, the
    /// active row spins and the rest grey out, so a click never looks ignored.
    pub fn busy(&self) -> bool {
        self.checking || self.batch_running || !self.installing_ids.is_empty()
    }

    /// Most recent log line for one item.
    ///
    /// Surfaces the outcome of a repair on the surface that triggered it (the
    /// agent setup card asks Doctor to fix the OpenCode state directory) without
    /// sending people to the Doctor page to find out what happened.
    pub fn last_log_for(&self, id: &str) -> Option<&DoctorLogEntry> {
        self.logs.iter().rev().find(|entry| entry.item_id == id)
    }

    pub fn auto_installable_missing_ids(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|i| i.status != DoctorStatus::Ready && i.can_auto_install)
            .map(|i| i.id.clone())
            .collect()
    }

    pub fn filtered_items(&self) -> Vec<&DoctorItem> {
        match self.filter {
            DoctorCategory::All => self.items.iter().collect(),
            cat => self.items.iter().filter(|i| i.category == cat).collect(),
        }
    }
}

impl Bridge {
    /// Refresh all Doctor diagnostics off the UI thread and update `BridgeState::doctor`.
    pub fn doctor_refresh(cx: &mut App) {
        let Ok(backend) = Self::backend(cx) else {
            return;
        };
        let pi_custom = Self::pi_custom_executable(cx);
        let oc_custom = Self::opencode_custom_executable(cx);
        let ag_custom = Self::antigravity_custom_executable(cx);

        Self::state(cx).update(cx, |s| {
            s.doctor.checking = true;
        });

        Self::run_blocking(
            cx,
            move || {
                Ok(probe_all_dependencies(
                    backend.as_ref(),
                    pi_custom.as_deref(),
                    oc_custom.as_deref(),
                    ag_custom.as_deref(),
                ))
            },
            |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.doctor.checking = false;
                    if let Ok(items) = result {
                        s.doctor.items = items;
                        let ready = s.doctor.ready_count();
                        let upgrade = s.doctor.upgrade_count();
                        let missing = s.doctor.missing_count();
                        let summary =
                            format!("{ready} ready, {upgrade} upgrade needed, {missing} missing");
                        s.doctor.last_checked_summary = Some(summary);
                    }
                });
            },
        );
    }

    /// Change the category filter chip.
    pub fn doctor_set_filter(category: DoctorCategory, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.doctor.filter = category;
        });
    }

    /// Clear live diagnostic logs.
    pub fn doctor_clear_logs(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.doctor.logs.clear();
        });
    }

    /// Arm one row's action: the next click on it runs, this one only asks.
    ///
    /// Arming a row disarms every other one, so exactly one destructive action
    /// can be pending at a time.
    pub fn doctor_arm_item(item_id: &str, cx: &mut App) {
        let item_id = item_id.to_string();
        Self::state(cx).update(cx, |s| {
            s.doctor.confirm_armed = Some(item_id);
        });
    }

    /// Drop the pending confirmation, if any.
    pub fn doctor_disarm(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.doctor.confirm_armed = None;
        });
    }

    /// Install or upgrade a single dependency item.
    pub fn doctor_install_item(item_id: &str, cx: &mut App) {
        eprintln!("[doctor] install_item {item_id}");
        let state = Self::state(cx);
        if state.read(cx).doctor.is_installing(item_id) {
            return;
        }

        let item_id = item_id.to_string();
        let target_item = state
            .read(cx)
            .doctor
            .items
            .iter()
            .find(|i| i.id == item_id)
            .cloned();

        let item_name = target_item
            .as_ref()
            .map(|i| i.name.clone())
            .unwrap_or_else(|| item_id.clone());
        // "Install" is wrong for the repair-style actions (Fix Ownership, Clean
        // Sockets, Trust CA), so the activity log names the action that ran.
        let action_label = target_item
            .as_ref()
            .map(|i| i.action_label.clone())
            .unwrap_or_else(|| "Install".to_string());

        state.update(cx, |s| {
            if !s.doctor.installing_ids.contains(&item_id) {
                s.doctor.installing_ids.push(item_id.clone());
            }
            s.doctor.push_log(
                item_id.clone(),
                format!("Started {action_label} for {item_name}…"),
                false,
            );
        });

        let id = item_id.clone();
        let name = item_name.clone();
        let action = action_label.clone();
        let customs = CustomExecutables::capture(cx);
        let backend = Self::backend(cx).ok();

        cx.spawn(async move |cx: &mut AsyncApp| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let id_clone = id.clone();
            let customs_clone = customs.clone();
            let backend_clone = backend.clone();

            let spawn_ok = std::thread::Builder::new()
                .name(format!("halo-doc-{}", id_clone))
                .spawn(move || {
                    let res = execute_with_verification(
                        backend_clone.as_deref().map(|b| b as &dyn Backend),
                        &id_clone,
                        &customs_clone,
                    );
                    let _ = tx.send(res);
                })
                .is_ok();

            let result = if !spawn_ok {
                Err(format!("failed to spawn install thread for {name}"))
            } else {
                rx.await
                    .unwrap_or_else(|_| Err(format!("installer worker thread crashed for {name}")))
            };

            let _ = cx.update(|cx| {
                // macOS repairs ask with `osascript` before they return. A
                // leftover non-macOS hint still carries a command; copy it
                // before the store update borrows `cx`.
                let result = match result {
                    Ok(message) => Ok(message),
                    Err(err) => Err(match privileged_command(&err) {
                        Some(command) => {
                            cx.write_to_clipboard(ClipboardItem::new_string(command));
                            format!(
                                "{err}\n\nCommand copied to the clipboard — paste it into Terminal."
                            )
                        }
                        None => err,
                    }),
                };

                let pi_installed = result.is_ok() && id == "harness:pi";
                Self::state(cx).update(cx, |s| {
                    s.doctor.installing_ids.retain(|i| i != &id);
                    match &result {
                        Ok(msg) => {
                            s.doctor.push_log(
                                &id,
                                format!("{action} succeeded for {name}: {msg}"),
                                false,
                            );
                            s.apply_ok_msg(format!("{name}: {action} complete."));
                        }
                        Err(err) => {
                            s.doctor.push_log(
                                &id,
                                format!("{action} failed for {name}: {err}"),
                                true,
                            );
                            s.apply_err(BridgeError::message(format!(
                                "{name}: {action} failed — {err}"
                            )));
                        }
                    }
                });
                if pi_installed {
                    if let Some(bin) = super::doctor_actions::pinned_pi_executable() {
                        if bin.is_file() {
                            Self::state(cx).update(cx, |s| {
                                s.coder_settings.pi_executable_path =
                                    Some(bin.display().to_string());
                            });
                            Self::coder_persist_settings(cx);
                        }
                    }
                }
                // A repair that unblocks a harness must also refresh the agent
                // setup card, or the user has to press "Check again" to see it
                // work.
                if id == super::agent_state::ITEM_ID
                    || id == "harness:cursor"
                    || id == "harness:pi"
                {
                    let _ = Self::halo_agent_refresh_catalogs(cx);
                }
                Self::doctor_refresh(cx);
            });
        })
        .detach();
    }

    /// Install all missing and auto-installable dependencies in sequence.
    pub fn doctor_install_all_missing(cx: &mut App) {
        let state = Self::state(cx);
        if state.read(cx).doctor.batch_running {
            return;
        }

        let ids = state.read(cx).doctor.auto_installable_missing_ids();
        if ids.is_empty() {
            state.update(cx, |s| {
                s.doctor.push_log(
                    "batch",
                    "No auto-installable missing dependencies found.",
                    false,
                );
            });
            return;
        }

        state.update(cx, |s| {
            s.doctor.batch_running = true;
            s.doctor.push_log(
                "batch",
                format!(
                    "Beginning batch installation of {} dependencies…",
                    ids.len()
                ),
                false,
            );
        });

        let customs = CustomExecutables::capture(cx);
        let backend = Self::backend(cx).ok();

        cx.spawn(async move |cx: &mut AsyncApp| {
            let mut failed = 0usize;
            for id in ids {
                let id_clone = id.clone();
                let customs_clone = customs.clone();
                let backend_clone = backend.clone();

                let _ = cx.update(|cx| {
                    Self::state(cx).update(cx, |s| {
                        if !s.doctor.installing_ids.contains(&id_clone) {
                            s.doctor.installing_ids.push(id_clone.clone());
                        }
                        s.doctor
                            .push_log(&id_clone, format!("Installing {id_clone}…"), false);
                    });
                });

                let (tx, rx) = tokio::sync::oneshot::channel();
                let id_for_thread = id_clone.clone();
                let spawn_ok = std::thread::Builder::new()
                    .name(format!("halo-batch-{}", id_clone))
                    .spawn(move || {
                        let res = execute_with_verification(
                            backend_clone.as_deref().map(|b| b as &dyn Backend),
                            &id_for_thread,
                            &customs_clone,
                        );
                        let _ = tx.send(res);
                    })
                    .is_ok();

                let res = if !spawn_ok {
                    Err(format!("could not spawn thread for {id_clone}"))
                } else {
                    rx.await
                        .unwrap_or_else(|_| Err(format!("worker died for {id_clone}")))
                };

                let _ = cx.update(|cx| {
                    Self::state(cx).update(cx, |s| {
                        s.doctor.installing_ids.retain(|i| i != &id_clone);
                        match &res {
                            Ok(out) => {
                                s.doctor.push_log(&id_clone, format!("Done: {out}"), false);
                            }
                            Err(err) => {
                                s.doctor
                                    .push_log(&id_clone, format!("Error: {err}"), true);
                            }
                        }
                    });
                });

                if res.is_err() {
                    failed += 1;
                }
            }

            let _ = cx.update(|cx| {
                Self::state(cx).update(cx, |s| {
                    s.doctor.batch_running = false;
                    if failed == 0 {
                        s.doctor
                            .push_log("batch", "Batch finished — every item is green.", false);
                    } else {
                        s.doctor.push_log(
                            "batch",
                            format!("Batch finished with {failed} item(s) still failing — see the rows above."),
                            true,
                        );
                    }
                });
                // The batch may have installed or unblocked a harness, so the
                // agent card is refreshed alongside the Doctor rows.
                let _ = Self::halo_agent_refresh_catalogs(cx);
                let _ = Self::refresh_harness_readiness(cx, true);
                Self::doctor_refresh(cx);
            });
        })
        .detach();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Installation actions execution
// ─────────────────────────────────────────────────────────────────────────────

fn execute_install_action(
    backend: Option<&dyn Backend>,
    item_id: &str,
    oc_custom: Option<&str>,
) -> Result<String, String> {
    if item_id == super::agent_state::ITEM_ID {
        return super::agent_state::repair();
    }
    match item_id {
        "harness:pi" => {
            // User-owned prefix, then the macOS administrator dialog if npm
            // is denied. A global install would leave Homebrew's pi 1.0.2 first
            // on PATH, so the row would stay on "Update required".
            let bin = doctor_actions::install_pinned_pi()?;
            Ok(format!("Installed Pi 1.4.0 at {}.", bin.display()))
        }
        "runtime:node" => doctor_actions::install_node(),
        "harness:opencode" => {
            // A blocked state directory is not fixed by reinstalling the binary
            // — the row says "Fix Ownership", so do exactly that.
            if halo_backend::commands::halo_agent::probe_opencode(oc_custom).is_state_blocked() {
                return super::agent_state::repair();
            }
            halo_backend::commands::halo_agent::install_opencode(oc_custom)
                .map(|_| "OpenCode installed/upgraded successfully".to_string())
        }
        "harness:antigravity" => {
            if let Some(b) = backend {
                b.halo_agent_install_antigravity()
                    .map_err(|e| e.to_string())
            } else {
                Err("Backend unavailable for Antigravity install".to_string())
            }
        }
        "harness:cursor" => {
            // Auth cannot be invented here — Install only covers Node + pinned SDK.
            let note = doctor_actions::ensure_node()?;
            let (node, shim) = halo_backend::commands::halo_agent::cursor_ensure_shim()?;
            let installed = if shim.as_os_str().is_empty() {
                format!("Cursor shim ready via override ({})", node.display())
            } else {
                format!(
                    "Cursor @cursor/sdk shim ready (node {}, shim {})",
                    node.display(),
                    shim.display()
                )
            };
            Ok(match note {
                Some(note) => format!("{note} {installed}"),
                None => installed,
            })
        }
        "lsp:shellcheck" => {
            #[cfg(target_os = "macos")]
            {
                doctor_actions::require_tool(
                    "brew",
                    "Install Homebrew from https://brew.sh, then run Check again.",
                )?;
                doctor_actions::run_shell("brew install shellcheck")
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err("Please install shellcheck via your platform package manager.".to_string())
            }
        }
        id if id.starts_with("lsp:") => {
            let server_id = id.trim_start_matches("lsp:");
            halo_lsp::install_server(server_id).map_err(|e| e.to_string())
        }
        "browser:agent-browser" => {
            let note = doctor_actions::ensure_node()?;
            let out =
                doctor_actions::run_shell("npm install -g agent-browser && agent-browser install")?;
            Ok(match note {
                Some(note) => format!("{note} {out}"),
                None => out,
            })
        }
        "browser:agent-desktop" => {
            let note = doctor_actions::ensure_node()?;
            let out = doctor_actions::run_shell("npm install -g agent-desktop")?;
            Ok(match note {
                Some(note) => format!("{note} {out}"),
                None => out,
            })
        }
        "browser:agent-desktop:permissions" => open_desktop_permission_guide(),
        "browser:halo-browser" => doctor_actions::rebuild_halo_browser(),
        "browser:chrome" => doctor_actions::open_url("https://google.com/chrome"),
        "proxy:mitm-ca" => {
            if let Some(b) = backend {
                b.trust_ca().map_err(|e| e.to_string())
            } else {
                Err("Backend unavailable for Root CA trust".to_string())
            }
        }
        "system:app-bundle" => {
            #[cfg(target_os = "macos")]
            {
                let candidates = [
                    std::env::current_exe().ok().and_then(|p| {
                        p.ancestors()
                            .find(|a| a.extension().map_or(false, |ext| ext == "app"))
                            .map(|p| p.to_path_buf())
                    }),
                    Some(PathBuf::from("/Applications/Halo.app")),
                    Some(PathBuf::from("dist/Halo.app")),
                ];
                let found = candidates.into_iter().flatten().find(|p| p.is_dir());
                if let Some(app_path) = found {
                    let lsp_dir = app_path.join("Contents/Resources/lsp");
                    if lsp_dir.is_dir() {
                        let _ = std::fs::remove_dir_all(&lsp_dir);
                    }
                    let status = Command::new("codesign")
                        .args(["--force", "--sign", "-", &app_path.display().to_string()])
                        .status();
                    let _ = Command::new("touch").arg(&app_path).status();
                    Ok(format!(
                        "Slimmed {} to ~60 MB and re-signed ({:?}).",
                        app_path.display(),
                        status
                    ))
                } else {
                    Err("No .app bundle found to slim".to_string())
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                Ok("Non-macOS target: no bundle to slim.".to_string())
            }
        }
        "system:instance-locks" => doctor_actions::repair_run_dir(),
        "system:orphan-processes" => {
            let mut killed = 0;
            for (pid, _) in orphan_pids() {
                if Command::new("kill").args(["-9", &pid]).status().is_ok() {
                    killed += 1;
                }
            }
            Ok(format!(
                "Terminated {killed} orphan background process(es)."
            ))
        }
        "system:storage" => doctor_actions::repair_storage(backend),
        other => Err(format!("Unknown item id: {other}")),
    }
}

/// User-configured harness binaries, carried into the worker thread so the
/// post-action probe sees the same environment the row was built from.
#[derive(Clone, Default)]
struct CustomExecutables {
    pi: Option<String>,
    opencode: Option<String>,
    antigravity: Option<String>,
}

impl CustomExecutables {
    fn capture(cx: &App) -> Self {
        Self {
            pi: Bridge::pi_custom_executable(cx),
            opencode: Bridge::opencode_custom_executable(cx),
            antigravity: Bridge::antigravity_custom_executable(cx),
        }
    }
}

/// Actions that deliberately hand off to a human. The row is expected to stay
/// as it is afterwards, so verification would raise a false alarm.
fn is_manual_handoff(item_id: &str) -> bool {
    matches!(
        item_id,
        // Opens the Chrome download page: the user installs it.
        "browser:chrome"
            // Opens the TCC panes: only the user can flip the switch.
            | "browser:agent-desktop:permissions"
    )
}

/// Open the macOS privacy panes for the desktop engine.
///
/// This never *requests* a grant — it opens the guide, which is the only
/// action macOS allows a process to take on its own behalf. Verification is
/// skipped (`is_manual_handoff`): the row stays red until the user acts, and
/// that is the correct diagnosis, not an action failure.
fn open_desktop_permission_guide() -> Result<String, String> {
    let path = doctor_actions::find_tool("agent-desktop")
        .ok_or("agent-desktop is not installed — install it, then open the guide")?;
    let output = Command::new(&path)
        .args(["permissions", "--guide"])
        .output()
        .map_err(|error| format!("could not run {path}: {error}"))?;
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("agent-desktop printed something that is not JSON: {error}"))?;
    if envelope
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        Ok("Opened the macOS permission guide. Grant Accessibility (and Screen Recording for \
            screenshots), then click Re-check All."
            .to_string())
    } else {
        let message = envelope
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(|message| message.as_str())
            .unwrap_or("unknown error");
        Err(format!(
            "agent-desktop permissions --guide failed: {message}"
        ))
    }
}

/// Run one action, then re-probe and refuse to report success while the row is
/// still failing.
///
/// A green toast on a still-broken dependency is the worst possible outcome of
/// a "fix" button, so the action's own claim is never trusted on its own.
fn execute_with_verification(
    backend: Option<&dyn Backend>,
    item_id: &str,
    customs: &CustomExecutables,
) -> Result<String, String> {
    let message = execute_install_action(backend, item_id, customs.opencode.as_deref())?;
    let Some(backend) = backend else {
        return Ok(message);
    };
    if is_manual_handoff(item_id) {
        return Ok(message);
    }

    let items = probe_all_dependencies(
        backend,
        customs.pi.as_deref(),
        customs.opencode.as_deref(),
        customs.antigravity.as_deref(),
    );
    match items.into_iter().find(|item| item.id == item_id) {
        Some(item) if item.status == DoctorStatus::Ready => {
            Ok(format!("{message} Verified: {}", item.hint))
        }
        // Cursor Install cannot mint a user API key. After Node/SDK are in
        // place the row may still be Failed (auth) with can_auto_install=false —
        // treat that as install success plus the remaining auth hint.
        Some(item)
            if item_id == "harness:cursor"
                && item.status == DoctorStatus::Failed
                && !item.can_auto_install =>
        {
            Ok(format!("{message} SDK verified; auth still needed. {}", item.hint))
        }
        Some(item) => Err(format!(
            "{message} But the check still fails. {}",
            item.hint
        )),
        // The row disappeared (e.g. a category that no longer probes): nothing
        // to contradict the action's own report.
        None => Ok(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_view_counts() {
        assert!(DoctorCategory::ALL.contains(&DoctorCategory::System));
        let mut view = DoctorView::default();
        assert_eq!(view.ready_count(), 0);
        assert_eq!(view.upgrade_count(), 0);
        assert_eq!(view.missing_count(), 0);

        view.items.push(DoctorItem {
            id: "test:1".into(),
            category: DoctorCategory::Harnesses,
            name: "Test 1".into(),
            description: "Test".into(),
            command: "t1".into(),
            status: DoctorStatus::Ready,
            version: None,
            resolved_path: None,
            hint: "".into(),
            can_auto_install: false,
            action_label: "Install".into(),
            doc_url: None,
        });

        view.items.push(DoctorItem {
            id: "test:2".into(),
            category: DoctorCategory::LanguageServers,
            name: "Test 2".into(),
            description: "Test".into(),
            command: "t2".into(),
            status: DoctorStatus::Missing,
            version: None,
            resolved_path: None,
            hint: "".into(),
            can_auto_install: true,
            action_label: "Install".into(),
            doc_url: None,
        });

        assert_eq!(view.ready_count(), 1);
        assert_eq!(view.missing_count(), 1);
        assert_eq!(view.auto_installable_missing_ids(), vec!["test:2"]);

        view.push_log("test:2", "Installing...", false);
        assert_eq!(view.logs.len(), 1);
        assert_eq!(view.logs[0].message, "Installing...");
    }

    #[test]
    fn test_system_bundle_probes() {
        let bundle = super::probe_app_bundle();
        assert_eq!(bundle.id, "system:app-bundle");
        assert_eq!(bundle.category, DoctorCategory::System);

        let locks = super::probe_instance_locks();
        assert_eq!(locks.id, "system:instance-locks");
        assert_eq!(locks.category, DoctorCategory::System);

        let orphans = super::probe_orphan_processes();
        assert_eq!(orphans.id, "system:orphan-processes");
        assert_eq!(orphans.category, DoctorCategory::System);
    }
}

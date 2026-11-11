//! Harness selection, model picker, and Pi installation management.

use std::time::Duration;

use super::super::*;
use crate::backend::Backend;

/// How long a probed model list stays "warm". Within this window, opening the
/// picker / agent panel / switching back to a harness reuses the cache and
/// does not re-spawn `pi --list-models` / `opencode models` (each ~1–3s).
/// The strip's refresh button and install/doctor paths always force a probe.
const CATALOG_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Copy)]
enum CatalogRefresh {
    /// Re-probe even when the cache is warm (refresh button, install, doctor).
    Force,
    /// Skip the spawn when this harness was probed recently; otherwise probe
    /// in the background and only show the loading skeleton on a cache miss.
    Ensure,
}

/// True when Ensure may skip the CLI spawn: we have a list *and* a fetch
/// timestamp still inside [`CATALOG_TTL`]. A cache without a stamp (failed
/// Instant reconstruction on boot, or never probed this session) is stale —
/// keep the rows on screen, but re-probe in the background.
fn catalog_is_warm(has_cache: bool, fetched_at: Option<&std::time::Instant>) -> bool {
    has_cache && fetched_at.is_some_and(|at| at.elapsed() < CATALOG_TTL)
}

/// The model a harness should run, given what that harness can actually serve.
///
/// A model id is only meaningful inside the harness that published it. Pi
/// resolves `opencode/beam-free` as a provider it knows and has no credentials
/// for, so running it produces `Unauthorized` *after* the turn has started.
///
/// Callers must pass a candidate that already belongs to `harness` (its
/// remembered pick), never another harness's live selection — carrying
/// `opencode/big-pickle` into Cursor/Pi both polluted memory and flashed
/// "Model changed … to Auto" in the transcript.
///
/// Empty catalog = "not probed yet" and never discards a selection. A listed
/// catalog keeps a known pick, else the harness's remembered pick, else the
/// **first catalog row** (Cursor's `auto`, etc.).
fn reconcile_model(
    harness: &str,
    catalog: &[halo_backend::commands::halo_agent::ModelInfo],
    selected: Option<&str>,
    remembered: &mut std::collections::HashMap<String, String>,
) -> Option<String> {
    if catalog.is_empty() {
        return selected
            .map(str::to_string)
            .or_else(|| remembered.get(harness).cloned());
    }
    let known = |id: &str| catalog.iter().any(|model| model.id == id);
    if let Some(selected) = selected {
        if known(selected) {
            remembered.insert(harness.to_string(), selected.to_string());
            return Some(selected.to_string());
        }
    }
    if let Some(previous) = remembered.get(harness).filter(|id| known(id)).cloned() {
        return Some(previous);
    }
    let seeded = catalog.first()?.id.clone();
    remembered.insert(harness.to_string(), seeded.clone());
    Some(seeded)
}

/// Catalog refresh must not demote a same-harness pick to Auto.
///
/// `opencode models` can fail open onto a tiny curated fallback that omits
/// `opencode/big-pickle`; treating that as "model gone" cleared the user's
/// choice and wrote Auto into the transcript. Keep the live pick when it is
/// still this harness's remembered model (or was in the previous cache).
fn reconcile_model_refresh(
    harness: &str,
    catalog: &[halo_backend::commands::halo_agent::ModelInfo],
    previous_cache: &[halo_backend::commands::halo_agent::ModelInfo],
    selected: Option<&str>,
    remembered: &mut std::collections::HashMap<String, String>,
) -> Option<String> {
    let known = |id: &str, list: &[halo_backend::commands::halo_agent::ModelInfo]| {
        list.iter().any(|model| model.id == id)
    };
    if let Some(selected) = selected {
        if catalog.is_empty() || known(selected, catalog) {
            remembered.insert(harness.to_string(), selected.to_string());
            return Some(selected.to_string());
        }
        let same_harness_pick = remembered.get(harness).map(String::as_str) == Some(selected)
            || known(selected, previous_cache);
        if same_harness_pick {
            remembered.insert(harness.to_string(), selected.to_string());
            return Some(selected.to_string());
        }
    }
    reconcile_model(harness, catalog, None, remembered)
}

impl Bridge {
    pub(crate) fn pi_custom_executable(cx: &App) -> Option<String> {
        Self::state(cx)
            .read(cx)
            .coder_settings
            .pi_executable_path
            .clone()
    }

    pub(crate) fn opencode_custom_executable(cx: &App) -> Option<String> {
        Self::state(cx)
            .read(cx)
            .coder_settings
            .opencode_executable_path
            .clone()
    }

    pub(crate) fn antigravity_custom_executable(cx: &App) -> Option<String> {
        Self::state(cx)
            .read(cx)
            .coder_settings
            .antigravity_executable_path
            .clone()
    }

    /// Toggle the coding-agent surface (replaces the editor top slot).
    pub fn halo_agent_toggle(cx: &mut App) -> BridgeResult<()> {
        let opening = !Self::state(cx).read(cx).coder.agent_open;
        Self::state(cx).update(cx, |s| {
            s.coder.agent_open = !s.coder.agent_open;
            if s.coder.agent_open {
                s.coder.sidebar_open = false;
                s.coder.agent_ensure_session();
                s.coder.agent_history_open = true;
            }
            s.commit_active_tab();
        });
        if opening {
            let _ = Self::halo_agent_ensure_catalogs(cx);
            let _ = Self::halo_agent_refresh_skills(cx);
            let _ = Self::halo_agent_refresh_browser(cx);
            let _ = Self::halo_agent_refresh_desktop(cx);
            if Self::state(cx).read(cx).coder.agent_harness == "antigravity" {
                Self::antigravity_acp_refresh_auth(cx);
            }
        }
        Ok(())
    }

    /// Force a catalog re-probe (refresh button, install, doctor, settings).
    pub fn halo_agent_refresh_catalogs(cx: &mut App) -> BridgeResult<()> {
        Self::refresh_agent_catalogs(cx, CatalogRefresh::Force)
    }

    /// Open the picker / agent with the cached list when it is still warm.
    pub fn halo_agent_ensure_catalogs(cx: &mut App) -> BridgeResult<()> {
        Self::refresh_agent_catalogs(cx, CatalogRefresh::Ensure)
    }

    /// Refresh harness/model catalogs off the UI thread.
    ///
    /// OpenCode/`opencode models` and `--version` are process spawns (~1s each).
    /// Never call those from paint or from the picker open click path — the
    /// picker opens immediately and shows [`CoderWork::agent_catalog_loading`]
    /// inside the card until this finishes. [`CatalogRefresh::Ensure`] skips
    /// the spawn entirely while the per-harness cache is within [`CATALOG_TTL`].
    fn refresh_agent_catalogs(cx: &mut App, mode: CatalogRefresh) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let pi_custom = Self::pi_custom_executable(cx);
        let oc_custom = Self::opencode_custom_executable(cx);
        let ag_custom = Self::antigravity_custom_executable(cx);
        let Some((harness, epoch)) = Self::state(cx).update(cx, |s| {
            let harness = s.coder.agent_harness.clone();
            // Keep the last list on screen even when the live catalog was
            // cleared (project switch used to drop the cache and leave an
            // empty picker until the probe returned).
            if s.coder.agent_model_catalog.is_empty() {
                if let Some(cached) = s.coder.agent_model_cache.get(&harness).cloned() {
                    s.coder.agent_model_catalog = cached;
                }
            }
            let has_cache = s.coder.agent_model_cache.contains_key(&harness);
            let warm = catalog_is_warm(has_cache, s.coder.agent_catalog_fetched_at.get(&harness));
            if matches!(mode, CatalogRefresh::Ensure) && warm {
                s.coder.agent_catalog_loading = false;
                return None;
            }
            // Skeleton only on a true miss — a stale cache stays visible while
            // the background probe updates it.
            s.coder.agent_catalog_loading = !has_cache;
            s.coder.agent_catalog_refresh_epoch =
                s.coder.agent_catalog_refresh_epoch.wrapping_add(1);
            Some((harness, s.coder.agent_catalog_refresh_epoch))
        }) else {
            return Ok(());
        };
        Self::run_blocking(
            cx,
            move || {
                Ok(Self::load_agent_catalogs(
                    backend.as_ref(),
                    &harness,
                    pi_custom.as_deref(),
                    oc_custom.as_deref(),
                    ag_custom.as_deref(),
                ))
            },
            move |result, cx| {
                let still_current = Self::state(cx).update(cx, |s| {
                    if s.coder.agent_catalog_refresh_epoch != epoch {
                        return false;
                    }
                    // Clear loading even on failure so the card never sticks.
                    s.coder.agent_catalog_loading = false;
                    true
                });
                if !still_current {
                    return;
                }
                let Ok(snapshot) = result else {
                    return;
                };
                // Drop stale results if the user switched harness while probing.
                if Self::state(cx).read(cx).coder.agent_harness != snapshot.harness {
                    return;
                }
                Self::apply_agent_catalog(snapshot, cx);
            },
        );
        Ok(())
    }

    fn load_agent_catalogs(
        backend: &dyn Backend,
        harness: &str,
        pi_custom: Option<&str>,
        oc_custom: Option<&str>,
        ag_custom: Option<&str>,
    ) -> AgentCatalogSnapshot {
        let (installed, needs_upgrade, state_blocked, setup_hint) = match harness {
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
                (
                    probe.is_ready(),
                    false,
                    false,
                    if probe.is_ready() {
                        None
                    } else {
                        Some(probe.setup_hint())
                    },
                )
            }
            _ => {
                let probe = backend.halo_agent_probe_pi(pi_custom).unwrap_or_else(|e| {
                    halo_backend::commands::halo_agent::PiProbeStatus::Failed {
                        message: e.to_string(),
                    }
                });
                (
                    probe.is_ready(),
                    probe.needs_upgrade(),
                    false,
                    if probe.is_ready() {
                        None
                    } else {
                        Some(probe.setup_hint())
                    },
                )
            }
        };
        let harnesses = backend
            .halo_agent_list_harnesses(pi_custom, oc_custom, ag_custom)
            .unwrap_or_default();
        let custom_for_models = match harness {
            "opencode" => oc_custom,
            "antigravity" => ag_custom,
            "cursor" => None,
            _ => pi_custom,
        };
        let models = backend
            .halo_agent_list_models(harness, custom_for_models)
            .unwrap_or_default();
        AgentCatalogSnapshot {
            harness: harness.to_string(),
            installed,
            needs_upgrade,
            state_blocked,
            setup_hint,
            harnesses,
            models,
        }
    }

    fn apply_agent_catalog(snapshot: AgentCatalogSnapshot, cx: &mut App) {
        let mut save = None;
        let persist_catalog = (snapshot.harness.clone(), snapshot.models.clone());
        let was_usable = selected_harness_usable(Self::state(cx).read(cx));
        Self::state(cx).update(cx, |s| {
            // Snapshot the prior list before replacing it — refresh uses it to
            // tell a transient catalog shrink from a truly foreign model id.
            let previous_cache = s
                .coder
                .agent_model_cache
                .get(&snapshot.harness)
                .cloned()
                .unwrap_or_default();
            s.coder.agent_harness_catalog = snapshot.harnesses;
            s.coder.agent_model_catalog = snapshot.models.clone();
            // Remember it per harness: the next switch back is instant.
            s.coder
                .agent_model_cache
                .insert(snapshot.harness.clone(), snapshot.models.clone());
            s.coder
                .agent_catalog_fetched_at
                .insert(snapshot.harness.clone(), std::time::Instant::now());
            s.coder.agent_harness_installed = snapshot.installed;
            s.coder.agent_pi_needs_upgrade = snapshot.needs_upgrade;
            s.coder.agent_state_blocked = snapshot.state_blocked;
            s.coder.agent_pi_setup_hint = snapshot.setup_hint;

            // Refresh must not demote a same-harness pick just because this
            // probe's list shrank. Foreign leftovers still get reconciled away.
            let before = s.coder.agent_model.clone();
            let after = reconcile_model_refresh(
                &snapshot.harness,
                &snapshot.models,
                &previous_cache,
                before.as_deref(),
                &mut s.coder.agent_model_by_harness,
            );
            if after != before {
                s.coder.agent_model = after.clone();
                save = Some((snapshot.harness.clone(), after));
                s.commit_active_tab();
            }
            // An install or probe that just made the selected harness runnable
            // closes the sheet. A refresh that finds it still runnable leaves
            // an explicitly opened sheet alone, so the user can switch.
            if !was_usable && selected_harness_usable(s) {
                s.coder.agent_provider_sheet_open = false;
                s.coder.agent_provider_sheet_dismissed = false;
            }
        });
        if let Ok(backend) = Self::backend(cx) {
            let _ = backend.coder_save_agent_model_catalog(&persist_catalog.0, &persist_catalog.1);
            if let Some((harness, model)) = save {
                let _ = backend.coder_save_agent_selection(&harness, model.as_deref());
            }
        }
    }

    pub fn halo_agent_set_harness(harness: &str, cx: &mut App) -> BridgeResult<()> {
        let harness = harness.to_string();
        let model = Self::state(cx).update(cx, |s| {
            let previous = std::mem::replace(&mut s.coder.agent_harness, harness.clone());
            if harness == "cursor" {
                // Prefer the sheet's readiness snapshot so radio selection does
                // not block on Node/SDK probes while the fill paints.
                if let Some(row) = s
                    .coder
                    .agent_harness_readiness
                    .iter()
                    .find(|row| row.id == "cursor")
                {
                    s.coder.agent_harness_installed = row.installed && row.auth_ready;
                    s.coder.agent_pi_setup_hint = if row.installed && row.auth_ready {
                        None
                    } else {
                        Some(row.subtitle.clone())
                    };
                } else {
                    let (ready, hint) = halo_backend::commands::halo_agent::cursor_session_ready();
                    s.coder.agent_harness_installed = ready;
                    s.coder.agent_pi_setup_hint = hint;
                }
            } else if let Some(info) = s
                .coder
                .agent_harness_catalog
                .iter()
                .find(|h| h.id == harness)
            {
                s.coder.agent_harness_installed = info.installed;
            }
            // Each harness keeps its own model. Never feed the previous
            // harness's live id into reconcile for the new one — that both
            // leaked `opencode/big-pickle` into Pi/Cursor and wrote
            // "Model changed … to Auto" whenever the strip switched.
            let previous_model = s.coder.agent_model.clone();
            let harness_changed = previous != harness;
            if harness_changed {
                if let Some(model) = previous_model.clone() {
                    s.coder.agent_model_by_harness.insert(previous, model);
                }
            }
            let catalog = s
                .coder
                .agent_model_cache
                .get(&harness)
                .cloned()
                .unwrap_or_default();
            let candidate = s.coder.agent_model_by_harness.get(&harness).cloned();
            s.coder.agent_model = reconcile_model(
                &harness,
                &catalog,
                candidate.as_deref(),
                &mut s.coder.agent_model_by_harness,
            );
            s.coder.agent_model_catalog = catalog;
            let new_model = s.coder.agent_model.clone();
            record_model_change(s, previous_model.as_deref(), new_model.as_deref());
            s.coder.agent_picker_filter = None;
            s.coder.agent_approval_picker_open = false;
            if harness_changed {
                s.coder.agent_provider_sheet_dismissed = false;
                if selected_harness_usable(s) {
                    s.coder.agent_provider_sheet_open = false;
                }
            }
            s.commit_active_tab();
            s.coder.agent_model.clone()
        });
        // Disk + catalog/skills probes stay off this click so the radio fill
        // paints immediately; a one-frame yield matches ConfirmFooter / theme.
        let save_harness = harness.clone();
        let save_model = model.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let _ = cx.update(|cx| {
                if let Ok(backend) = Self::backend(cx) {
                    let _ = backend.coder_save_agent_selection(&save_harness, save_model.as_deref());
                }
                if save_harness == "antigravity" {
                    Self::antigravity_acp_refresh_auth(cx);
                }
                let _ = Self::halo_agent_ensure_catalogs(cx);
                let _ = Self::halo_agent_refresh_skills(cx);
            });
        })
        .detach();
        Ok(())
    }

    /// The sheet is up when the user opened it, or when the selected harness
    /// cannot run and they have not dismissed it.
    pub fn agent_provider_sheet_visible(state: &BridgeState) -> bool {
        if state.coder.agent_provider_sheet_open {
            return true;
        }
        if state.coder.agent_running || state.coder.agent_provider_sheet_dismissed {
            return false;
        }
        !selected_harness_usable(state)
    }

    /// Header control. Keeps the sheet up even when the current harness is ready.
    pub fn halo_agent_open_provider_sheet(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_provider_sheet_open = true;
            s.coder.agent_provider_sheet_dismissed = false;
        });
        let _ = Self::refresh_harness_readiness(cx, false);
    }

    /// Scrim or Escape. The setup card above the composer comes back if the
    /// selected harness still cannot run.
    pub fn halo_agent_dismiss_provider_sheet(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_provider_sheet_open = false;
            s.coder.agent_provider_sheet_dismissed = true;
        });
    }

    /// Drop an explicit hold once the selected harness can run (install finished,
    /// or Antigravity sign-in completed).
    pub(crate) fn close_provider_sheet_if_usable(cx: &mut App) {
        if !selected_harness_usable(Self::state(cx).read(cx)) {
            return;
        }
        if !Self::state(cx).read(cx).coder.agent_provider_sheet_open {
            return;
        }
        Self::state(cx).update(cx, |s| {
            s.coder.agent_provider_sheet_open = false;
            s.coder.agent_provider_sheet_dismissed = false;
        });
    }

    /// Radio selection. A harness that can already run closes the sheet.
    /// Switching is refused while a turn is in flight.
    pub fn halo_agent_choose_provider(harness: &str, cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).coder.agent_running {
            return Ok(());
        }
        let current = Self::state(cx).read(cx).coder.agent_harness.clone();
        if current != harness {
            Self::halo_agent_set_harness(harness, cx)?;
        }
        let usable = selected_harness_usable(Self::state(cx).read(cx));
        Self::state(cx).update(cx, |s| {
            s.coder.agent_provider_sheet_dismissed = false;
            s.coder.agent_provider_sheet_open = !usable;
        });
        if !usable {
            let _ = Self::refresh_harness_readiness(cx, false);
        }
        Ok(())
    }

    /// Cold probe for the sheet. No-op while a probe is in flight or the snapshot
    /// is still inside the catalog TTL.
    pub fn halo_agent_ensure_provider_readiness(cx: &mut App) {
        let _ = Self::refresh_harness_readiness(cx, false);
    }

    /// Probe every harness. `force` ignores the warm cache (after install).
    /// A cold open reuses a recent snapshot so the sheet does not spawn four
    /// CLIs on every click.
    pub(crate) fn refresh_harness_readiness(cx: &mut App, force: bool) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let pi_custom = Self::pi_custom_executable(cx);
        let oc_custom = Self::opencode_custom_executable(cx);
        let ag_custom = Self::antigravity_custom_executable(cx);
        let signed_in = Self::state(cx).read(cx).antigravity_acp_signed_in;
        let Some(epoch) = Self::state(cx).update(cx, |s| {
            let warm = !s.coder.agent_harness_readiness.is_empty()
                && s.coder
                    .agent_harness_readiness_at
                    .is_some_and(|at| at.elapsed() < CATALOG_TTL);
            if !force && (s.coder.agent_harness_readiness_loading || warm) {
                return None;
            }
            s.coder.agent_harness_readiness_loading = true;
            s.coder.agent_harness_readiness_epoch =
                s.coder.agent_harness_readiness_epoch.wrapping_add(1);
            Some(s.coder.agent_harness_readiness_epoch)
        }) else {
            return Ok(());
        };
        Self::run_blocking(
            cx,
            move || {
                Ok(probe_all_harness_readiness(
                    backend.as_ref(),
                    pi_custom.as_deref(),
                    oc_custom.as_deref(),
                    ag_custom.as_deref(),
                    signed_in,
                ))
            },
            move |result, cx| {
                let Ok(mut rows) = result else {
                    Self::state(cx).update(cx, |s| {
                        if s.coder.agent_harness_readiness_epoch == epoch {
                            s.coder.agent_harness_readiness_loading = false;
                        }
                    });
                    return;
                };
                let signed_in = {
                    let state = Self::state(cx).read(cx);
                    state.antigravity_acp_signed_in || antigravity_session_authed(&state)
                };
                if let Some(row) = rows.iter_mut().find(|row| row.id == "antigravity") {
                    if row.installed && !row.needs_upgrade && !row.state_blocked {
                        row.auth_ready = signed_in;
                        row.subtitle = if signed_in {
                            halo_backend::commands::halo_agent::HarnessReadiness::ready_blurb(
                                "antigravity",
                            )
                            .to_string()
                        } else {
                            "Sign in with Google to use Antigravity models.".into()
                        };
                    }
                }
                Self::state(cx).update(cx, |s| {
                    if s.coder.agent_harness_readiness_epoch != epoch {
                        return;
                    }
                    s.coder.agent_harness_readiness = rows;
                    s.coder.agent_harness_readiness_loading = false;
                    s.coder.agent_harness_readiness_at = Some(std::time::Instant::now());
                });
            },
        );
        Ok(())
    }

    pub fn halo_agent_install_pi(cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).coder.agent_installing.is_some() {
            return Ok(());
        }

        let backend = Self::backend(cx)?;
        let oc_custom = Self::opencode_custom_executable(cx);
        let ag_custom = Self::antigravity_custom_executable(cx);
        let harness = Self::state(cx).read(cx).coder.agent_harness.clone();

        Self::state(cx).update(cx, |s| {
            s.coder.agent_installing = Some("pi".into());
            s.coder.agent_install_error = None;
            s.coder.agent_install_error_harness = Some("pi".into());
        });

        // Install + catalog probe stay off the UI thread so the window keeps
        // painting and the OS cursor never goes busy. The binary lands under
        // ~/.halo/pi so a Homebrew pi 1.0.2 earlier on PATH is not the one
        // Halo keeps running. A protected prefix asks macOS for the password.
        Self::run_blocking(
            cx,
            move || {
                let bin = super::super::doctor_actions::install_pinned_pi()
                    .map_err(BridgeError::message)?;
                let pi = bin.display().to_string();
                let snapshot = Self::load_agent_catalogs(
                    backend.as_ref(),
                    &harness,
                    Some(&pi),
                    oc_custom.as_deref(),
                    ag_custom.as_deref(),
                );
                Ok((pi, snapshot))
            },
            |result, cx| match result {
                Ok((pi, snapshot)) => {
                    Self::state(cx).update(cx, |s| {
                        s.coder_settings.pi_executable_path = Some(pi);
                    });
                    Self::coder_persist_settings(cx);
                    Self::finish_agent_install(Ok(snapshot), cx);
                }
                Err(error) => Self::finish_agent_install(Err(error), cx),
            },
        );

        Ok(())
    }

    /// Install OpenCode ≥ 2.2.0 (v2 installer for 1.x; `opencode upgrade` for too-old 2.x).
    pub fn halo_agent_install_opencode(cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).coder.agent_installing.is_some() {
            return Ok(());
        }

        let backend = Self::backend(cx)?;
        let pi_custom = Self::pi_custom_executable(cx);
        let oc_custom = Self::opencode_custom_executable(cx);
        let ag_custom = Self::antigravity_custom_executable(cx);
        let harness = Self::state(cx).read(cx).coder.agent_harness.clone();
        let install_custom = oc_custom.clone();

        Self::state(cx).update(cx, |s| {
            s.coder.agent_installing = Some("opencode".into());
            s.coder.agent_install_error = None;
            s.coder.agent_install_error_harness = Some("opencode".into());
        });

        Self::run_blocking(
            cx,
            move || {
                halo_backend::commands::halo_agent::install_opencode(install_custom.as_deref())
                    .map_err(BridgeError::message)?;
                Ok(Self::load_agent_catalogs(
                    backend.as_ref(),
                    &harness,
                    pi_custom.as_deref(),
                    oc_custom.as_deref(),
                    ag_custom.as_deref(),
                ))
            },
            |result, cx| Self::finish_agent_install(result, cx),
        );

        Ok(())
    }

    fn finish_agent_install(result: BridgeResult<AgentCatalogSnapshot>, cx: &mut App) {
        match result {
            Ok(snapshot) => {
                Self::state(cx).update(cx, |s| {
                    s.coder.agent_installing = None;
                    s.coder.agent_install_error = None;
                    s.coder.agent_install_error_harness = None;
                });
                if Self::state(cx).read(cx).coder.agent_harness == snapshot.harness {
                    Self::apply_agent_catalog(snapshot, cx);
                }
            }
            Err(e) => {
                Self::state(cx).update(cx, |s| {
                    s.coder.agent_installing = None;
                    s.coder.agent_install_error = Some(e.to_string());
                });
            }
        }
        let _ = Self::refresh_harness_readiness(cx, true);
    }

    pub fn halo_agent_set_model(model: &str, cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).coder.agent_running {
            return Err(crate::BridgeError::message(
                "Wait for the agent to finish before changing the model.",
            ));
        }
        let model = halo_backend::commands::halo_agent::migrate_model_id(model);
        let harness = Self::state(cx).update(cx, |s| {
            let previous = s.coder.agent_model.clone();
            s.coder.agent_model = Some(model.clone());
            s.coder
                .agent_model_by_harness
                .insert(s.coder.agent_harness.clone(), model.clone());
            push_recent_model(&mut s.coder.agent_recent_models, &model);
            s.coder.agent_picker_open = false;
            s.coder.agent_approval_picker_open = false;
            // The transcript announces the switch so the user can tell which
            // half of the thread answered with which model.
            record_model_change(s, previous.as_deref(), Some(&model));
            s.commit_active_tab();
            s.coder.agent_harness.clone()
        });
        let _ = Self::backend(cx)?.coder_save_agent_selection(&harness, Some(&model));
        Ok(())
    }

    /// Clear the selected model (Agent default) for the active harness.
    pub fn halo_agent_clear_model(cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).coder.agent_running {
            return Err(crate::BridgeError::message(
                "Wait for the agent to finish before changing the model.",
            ));
        }
        let harness = Self::state(cx).update(cx, |s| {
            let previous = s.coder.agent_model.clone();
            s.coder.agent_model = None;
            s.coder
                .agent_model_by_harness
                .remove(&s.coder.agent_harness);
            s.coder.agent_picker_open = false;
            s.coder.agent_approval_picker_open = false;
            record_model_change(s, previous.as_deref(), None);
            s.commit_active_tab();
            s.coder.agent_harness.clone()
        });
        let _ = Self::backend(cx)?.coder_save_agent_selection(&harness, None);
        Ok(())
    }

    /// Set the picker provider filter (`None` = All, `Some("recent")`, or a provider id).
    pub fn halo_agent_set_picker_filter(filter: Option<&str>, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_picker_filter = filter.map(str::to_string).filter(|f| !f.is_empty());
        });
        Ok(())
    }

    /// Open (never close) the picker — the failure card's "Choose model".
    pub fn halo_agent_open_picker(cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).coder.agent_running {
            return Ok(());
        }
        if Self::state(cx).read(cx).coder.agent_picker_open {
            return Ok(());
        }
        Self::halo_agent_toggle_picker(cx)
    }

    pub fn halo_agent_toggle_picker(cx: &mut App) -> BridgeResult<()> {
        let (opening, blocked) = {
            let s = Self::state(cx).read(cx);
            let open = s.coder.agent_picker_open;
            // Opening during a run is blocked; closing stays allowed so a sheet
            // left open when the turn started can still dismiss.
            (!open, !open && s.coder.agent_running)
        };
        if blocked {
            return Ok(());
        }
        Self::state(cx).update(cx, |s| {
            s.coder.agent_picker_open = !s.coder.agent_picker_open;
            if s.coder.agent_picker_open {
                s.coder.agent_approval_picker_open = false;
                s.coder.agent_picker_filter = None;
                if let Some(model) = s.coder.agent_model.clone() {
                    push_recent_model(&mut s.coder.agent_recent_models, &model);
                }
            }
        });
        if opening {
            let _ = Self::halo_agent_ensure_catalogs(cx);
        }
        Ok(())
    }

    pub fn halo_agent_close_picker(cx: &mut App) -> BridgeResult<()> {
        // Every press outside the sheet asks for this. Bail out when it is already
        // closed: `update` always notifies, and a needless notify repaints the whole
        // shell on every stray click.
        if !Self::state(cx).read(cx).coder.agent_picker_open {
            return Ok(());
        }
        Self::state(cx).update(cx, |s| {
            s.coder.agent_picker_open = false;
            s.coder.agent_picker_filter = None;
        });
        Ok(())
    }

    /// Cached harness rows only — never shells out (paint-safe).
    pub fn halo_agent_list_harnesses(
        cx: &App,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::HarnessInfo>> {
        Ok(Self::state(cx).read(cx).coder.agent_harness_catalog.clone())
    }

    /// Download/install the pinned Antigravity ACP server archive (off UI thread).
    pub fn halo_agent_install_antigravity(cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).coder.agent_installing.is_some() {
            return Ok(());
        }

        let backend = Self::backend(cx)?;
        let pi_custom = Self::pi_custom_executable(cx);
        let oc_custom = Self::opencode_custom_executable(cx);
        let ag_custom = Self::antigravity_custom_executable(cx);
        let harness = Self::state(cx).read(cx).coder.agent_harness.clone();

        Self::state(cx).update(cx, |s| {
            s.coder.agent_installing = Some("antigravity".into());
            s.coder.agent_install_error = None;
            s.coder.agent_install_error_harness = Some("antigravity".into());
        });

        Self::run_blocking(
            cx,
            move || {
                let path = backend
                    .halo_agent_install_antigravity()
                    .map_err(|e| BridgeError::message(e.to_string()))?;
                let snapshot = Self::load_agent_catalogs(
                    backend.as_ref(),
                    &harness,
                    pi_custom.as_deref(),
                    oc_custom.as_deref(),
                    ag_custom.as_deref(),
                );
                Ok((path, snapshot))
            },
            |result, cx| match result {
                Ok((path, snapshot)) => {
                    Self::state(cx).update(cx, |s| {
                        s.coder.agent_installing = None;
                        s.coder.agent_install_error = None;
                        s.coder.agent_install_error_harness = None;
                        s.last_message = Some(format!("Installed Antigravity ACP at {path}"));
                    });
                    if Self::state(cx).read(cx).coder.agent_harness == snapshot.harness {
                        Self::apply_agent_catalog(snapshot, cx);
                    }
                    let _ = Self::refresh_harness_readiness(cx, true);
                }
                Err(e) => {
                    Self::state(cx).update(cx, |s| {
                        s.coder.agent_installing = None;
                        s.coder.agent_install_error = Some(e.to_string());
                        s.last_message = Some(format!("Antigravity install failed: {e}"));
                    });
                    let _ = Self::refresh_harness_readiness(cx, true);
                }
            },
        );

        Ok(())
    }

    /// Cached model rows only — never shells out (paint-safe).
    /// Call [`Self::halo_agent_refresh_catalogs`] to populate / refresh.
    pub fn halo_agent_list_models(
        harness: &str,
        cx: &App,
    ) -> BridgeResult<Vec<halo_backend::commands::halo_agent::ModelInfo>> {
        let state = Self::state(cx).read(cx);
        if state.coder.agent_harness == harness {
            return Ok(state.coder.agent_model_catalog.clone());
        }
        Ok(Vec::new())
    }

    pub(crate) fn coder_require_root(cx: &App) -> BridgeResult<String> {
        Self::state(cx)
            .read(cx)
            .coder
            .root_path
            .clone()
            .ok_or_else(|| BridgeError::message("No folder open"))
    }
}

const RECENT_MODELS_CAP: usize = 6;

/// The selected harness can start a turn. Pi's model API key is not part of
/// this — that card stays above the composer. Antigravity also counts an ACP
/// session that already exists on this chat.
fn selected_harness_usable(state: &BridgeState) -> bool {
    let coder = &state.coder;
    if !coder.agent_harness_installed || coder.agent_state_blocked || coder.agent_pi_needs_upgrade {
        return false;
    }
    if coder.agent_harness == "antigravity" {
        return state.antigravity_acp_signed_in || antigravity_session_authed(state);
    }
    true
}

fn antigravity_session_authed(state: &BridgeState) -> bool {
    state.coder.agent_harness == "antigravity"
        && state
            .coder
            .agent_sessions
            .iter()
            .find(|session| session.id == state.coder.agent_active_session_id)
            .is_some_and(|session| session.harness_session_id.is_some())
}

fn probe_all_harness_readiness(
    backend: &dyn Backend,
    pi_custom: Option<&str>,
    oc_custom: Option<&str>,
    ag_custom: Option<&str>,
    antigravity_signed_in: bool,
) -> Vec<halo_backend::commands::halo_agent::HarnessReadiness> {
    vec![
        pi_readiness(backend.halo_agent_probe_pi(pi_custom).unwrap_or_else(|e| {
            halo_backend::commands::halo_agent::PiProbeStatus::Failed {
                message: e.to_string(),
            }
        })),
        opencode_readiness(
            backend
                .halo_agent_probe_opencode(oc_custom)
                .unwrap_or_else(|e| {
                    halo_backend::commands::halo_agent::OpenCodeProbeStatus::Failed {
                        message: e.to_string(),
                    }
                }),
        ),
        antigravity_readiness(
            backend
                .halo_agent_probe_antigravity(ag_custom)
                .unwrap_or_else(|e| {
                    halo_backend::commands::halo_agent::AntigravityProbeStatus::Failed {
                        message: e.to_string(),
                    }
                }),
            antigravity_signed_in,
        ),
        cursor_readiness(),
    ]
}

fn pi_readiness(
    probe: halo_backend::commands::halo_agent::PiProbeStatus,
) -> halo_backend::commands::halo_agent::HarnessReadiness {
    use halo_backend::commands::halo_agent::HarnessReadiness;
    let needs_upgrade = probe.needs_upgrade();
    let ready = probe.is_ready();
    halo_backend::commands::halo_agent::HarnessReadiness {
        id: "pi".into(),
        installed: ready || needs_upgrade,
        needs_upgrade,
        state_blocked: false,
        auth_ready: true,
        subtitle: if ready {
            HarnessReadiness::ready_blurb("pi").to_string()
        } else {
            probe.setup_hint()
        },
    }
}

fn opencode_readiness(
    probe: halo_backend::commands::halo_agent::OpenCodeProbeStatus,
) -> halo_backend::commands::halo_agent::HarnessReadiness {
    use halo_backend::commands::halo_agent::HarnessReadiness;
    let ready = probe.is_ready();
    let needs_upgrade = probe.needs_upgrade();
    let state_blocked = probe.is_state_blocked();
    halo_backend::commands::halo_agent::HarnessReadiness {
        id: "opencode".into(),
        installed: ready || needs_upgrade || state_blocked,
        needs_upgrade,
        state_blocked,
        auth_ready: true,
        subtitle: if ready {
            HarnessReadiness::ready_blurb("opencode").to_string()
        } else {
            probe.setup_hint()
        },
    }
}

fn antigravity_readiness(
    probe: halo_backend::commands::halo_agent::AntigravityProbeStatus,
    signed_in: bool,
) -> halo_backend::commands::halo_agent::HarnessReadiness {
    use halo_backend::commands::halo_agent::HarnessReadiness;
    let ready = probe.is_ready();
    halo_backend::commands::halo_agent::HarnessReadiness {
        id: "antigravity".into(),
        installed: ready,
        needs_upgrade: false,
        state_blocked: false,
        auth_ready: ready && signed_in,
        subtitle: if ready && signed_in {
            HarnessReadiness::ready_blurb("antigravity").to_string()
        } else if ready {
            "Sign in with Google to use Antigravity models.".into()
        } else {
            probe.setup_hint()
        },
    }
}

fn cursor_readiness() -> halo_backend::commands::halo_agent::HarnessReadiness {
    use halo_backend::commands::halo_agent::HarnessReadiness;
    let sdk = halo_backend::commands::halo_agent::cursor_sdk_ready();
    let (ready, hint) = halo_backend::commands::halo_agent::cursor_session_ready();
    halo_backend::commands::halo_agent::HarnessReadiness {
        id: "cursor".into(),
        installed: sdk,
        needs_upgrade: false,
        state_blocked: false,
        auth_ready: ready,
        subtitle: if ready {
            HarnessReadiness::ready_blurb("cursor").to_string()
        } else {
            hint.unwrap_or_else(|| "Cursor harness is not ready.".into())
        },
    }
}

struct AgentCatalogSnapshot {
    harness: String,
    installed: bool,
    needs_upgrade: bool,
    /// OpenCode's binary is fine but its state directory is not writable, so
    /// the card must offer the ownership repair instead of a reinstall.
    state_blocked: bool,
    setup_hint: Option<String>,
    harnesses: Vec<halo_backend::commands::halo_agent::HarnessInfo>,
    models: Vec<halo_backend::commands::halo_agent::ModelInfo>,
}

fn push_recent_model(recent: &mut Vec<String>, model: &str) {
    recent.retain(|id| id != model);
    recent.insert(0, model.to_string());
    recent.truncate(RECENT_MODELS_CAP);
}

/// Human label for a model id: the catalog's display name when it lists the
/// model, else the id's last path segment (`deepseek/deepseek-flash` →
/// `deepseek-flash`). The transcript shows this, not the provider-qualified
/// key the harness runs on.
fn model_display_name(
    catalog: &[halo_backend::commands::halo_agent::ModelInfo],
    model: &str,
) -> String {
    if let Some(info) = catalog.iter().find(|m| m.id == model) {
        return info.display_name.clone();
    }
    model
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(model)
        .to_string()
}

/// Every model the app currently knows about, across harnesses.
///
/// The live catalog only covers the selected harness, but a switch can leave
/// a model behind that belongs to the harness we just left — its display name
/// has to be looked up in that harness's cache, or the notice would print a
/// raw id like `deepseek/deepseek-flash` instead of `DeepSeek Flash`.
fn known_models(state: &BridgeState) -> Vec<halo_backend::commands::halo_agent::ModelInfo> {
    let mut models = state.coder.agent_model_catalog.clone();
    for cached in state.coder.agent_model_cache.values() {
        for model in cached {
            if !models.iter().any(|m| m.id == model.id) {
                models.push(model.clone());
            }
        }
    }
    models
}

/// Record a model switch in the active chat's transcript.
///
/// A switch only matters once the thread has content: on an empty chat the
/// next turn simply uses the new model, and a divider would be noise. The
/// notice is a system row, so it never becomes a user prompt and never feeds
/// the harness's context.
fn record_model_change(state: &mut BridgeState, from: Option<&str>, to: Option<&str>) {
    // Nothing to announce when the pick did not actually change.
    if from == to {
        return;
    }
    let catalog = known_models(state);
    // "Auto" is the picker label for "no explicit model" — the harness picks
    // its own default. Say so rather than printing an empty slot.
    let label = |model: Option<&str>| match model {
        None => "Auto".to_string(),
        Some(model) => model_display_name(&catalog, model),
    };
    let from_label = label(from);
    let to_label = label(to);
    // An empty transcript has nothing to divide yet.
    if state.coder.agent_messages.is_empty() {
        return;
    }
    state.coder.agent_messages.push(HaloAgentMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: HaloAgentRole::System,
        parts: vec![HaloAgentPart::ModelChanged {
            from: from_label,
            to: to_label,
        }],
        usage: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo_backend::commands::halo_agent::ModelInfo;
    use std::collections::HashMap;

    fn catalog(ids: &[&str]) -> Vec<ModelInfo> {
        ids.iter()
            .map(|id| ModelInfo {
                id: (*id).into(),
                display_name: (*id).into(),
                supports_images: None,
            })
            .collect()
    }

    #[test]
    fn warm_cache_skips_ensure_probe() {
        let just_now = std::time::Instant::now();
        assert!(catalog_is_warm(true, Some(&just_now)));
    }

    #[test]
    fn stale_or_unstamped_cache_still_needs_a_probe() {
        assert!(!catalog_is_warm(false, None));
        assert!(!catalog_is_warm(true, None));
        let stale = std::time::Instant::now()
            .checked_sub(CATALOG_TTL + Duration::from_secs(1))
            .expect("ttl fits in Instant");
        assert!(!catalog_is_warm(true, Some(&stale)));
    }

    /// Harness switch passes only this harness's remembered candidate — a
    /// foreign id is ignored and Pi seeds its first served model.
    #[test]
    fn a_model_from_another_harness_is_not_carried_over() {
        let mut remembered = HashMap::new();
        let pi = catalog(&["antigravity/gemini-3.8-flash"]);
        // Simulate switch: candidate is Pi's memory (none), not OpenCode's live id.
        let picked = reconcile_model("pi", &pi, None, &mut remembered);
        assert_eq!(picked.as_deref(), Some("antigravity/gemini-3.8-flash"));
        assert_eq!(
            remembered.get("pi").map(String::as_str),
            Some("antigravity/gemini-3.8-flash")
        );
    }

    /// A shrunk OpenCode probe must not wipe `big-pickle` back to Auto.
    #[test]
    fn refresh_keeps_a_same_harness_pick_missing_from_a_shrunk_catalog() {
        let mut remembered =
            HashMap::from([("opencode".to_string(), "opencode/big-pickle".to_string())]);
        let previous = catalog(&["opencode/beam-free", "opencode/big-pickle"]);
        let shrunk = catalog(&["anthropic/claude-sonnet-4-5"]); // curated fallback
        assert_eq!(
            reconcile_model_refresh(
                "opencode",
                &shrunk,
                &previous,
                Some("opencode/big-pickle"),
                &mut remembered,
            )
            .as_deref(),
            Some("opencode/big-pickle")
        );
    }

    #[test]
    fn a_served_model_is_never_moved() {
        let mut remembered = HashMap::new();
        let pi = catalog(&["antigravity/gemini-3.8-flash", "antigravity/gpt-oss-120b"]);
        assert_eq!(
            reconcile_model("pi", &pi, Some("antigravity/gpt-oss-120b"), &mut remembered)
                .as_deref(),
            Some("antigravity/gpt-oss-120b")
        );
    }

    /// Switch restores this harness's remembered pick, ignoring any leftover
    /// foreign id the caller might still hold.
    #[test]
    fn a_remembered_model_wins_over_a_foreign_selection() {
        let mut remembered =
            HashMap::from([("pi".to_string(), "antigravity/gpt-oss-120b".to_string())]);
        let pi = catalog(&["antigravity/gemini-3.8-flash", "antigravity/gpt-oss-120b"]);
        let candidate = remembered.get("pi").cloned();
        assert_eq!(
            reconcile_model("pi", &pi, candidate.as_deref(), &mut remembered).as_deref(),
            Some("antigravity/gpt-oss-120b")
        );
    }

    #[test]
    fn each_harness_keeps_its_own_model() {
        let mut remembered =
            HashMap::from([("opencode".to_string(), "opencode/beam-free".to_string())]);
        let opencode = catalog(&["opencode/beam-free", "opencode/big-pickle"]);
        assert_eq!(
            reconcile_model("opencode", &opencode, None, &mut remembered).as_deref(),
            Some("opencode/beam-free")
        );
        let pi = catalog(&["antigravity/gemini-3.8-flash"]);
        assert_eq!(
            reconcile_model("pi", &pi, None, &mut remembered).as_deref(),
            Some("antigravity/gemini-3.8-flash")
        );
    }

    #[test]
    fn every_harness_seeds_a_real_catalog_model() {
        let mut remembered = HashMap::new();
        let pi = catalog(&["anthropic/claude-sonnet-5-5", "openai/gpt-5"]);
        assert_eq!(
            reconcile_model("pi", &pi, None, &mut remembered).as_deref(),
            Some("anthropic/claude-sonnet-5-5")
        );
        let opencode = catalog(&["opencode/beam-free"]);
        assert_eq!(
            reconcile_model("opencode", &opencode, None, &mut remembered).as_deref(),
            Some("opencode/beam-free")
        );
        let antigravity = catalog(&["gemini-3-flash"]);
        assert_eq!(
            reconcile_model("antigravity", &antigravity, None, &mut remembered).as_deref(),
            Some("gemini-3-flash")
        );
    }

    #[test]
    fn cursor_seeds_the_first_catalog_row_when_nothing_is_remembered() {
        let mut remembered = HashMap::new();
        let cursor = catalog(&["auto", "composer-1", "gpt-5.4"]);
        assert_eq!(
            reconcile_model("cursor", &cursor, None, &mut remembered).as_deref(),
            Some("auto")
        );
        assert_eq!(remembered.get("cursor").map(String::as_str), Some("auto"));
    }

    #[test]
    fn seed_is_always_the_first_list_entry() {
        let mut remembered = HashMap::new();
        let cursor = catalog(&["composer-1", "auto"]);
        assert_eq!(
            reconcile_model("cursor", &cursor, None, &mut remembered).as_deref(),
            Some("composer-1"),
            "first row wins even when auto is later in the list"
        );
    }

    #[test]
    fn cursor_keeps_an_explicit_non_auto_pick() {
        let mut remembered = HashMap::new();
        let cursor = catalog(&["auto", "composer-1"]);
        assert_eq!(
            reconcile_model("cursor", &cursor, Some("composer-1"), &mut remembered).as_deref(),
            Some("composer-1")
        );
    }

    /// "Not probed yet" is not evidence that the selection is wrong.
    #[test]
    fn an_empty_catalog_never_discards_a_selection() {
        let mut remembered = HashMap::new();
        assert_eq!(
            reconcile_model(
                "pi",
                &[],
                Some("antigravity/gemini-3.8-flash"),
                &mut remembered
            )
            .as_deref(),
            Some("antigravity/gemini-3.8-flash")
        );
    }

    fn state_with_transcript() -> BridgeState {
        let mut state = BridgeState::default();
        state.coder.agent_messages.push(HaloAgentMessage {
            id: "m1".into(),
            role: HaloAgentRole::User,
            parts: vec![HaloAgentPart::Text("hello".into())],
            usage: None,
        });
        state
    }

    /// The divider names the models the user actually picked.
    #[test]
    fn a_model_switch_is_recorded_in_the_transcript() {
        let mut state = state_with_transcript();
        state.coder.agent_model_catalog = vec![ModelInfo {
            id: "deepseek/deepseek-flash".into(),
            display_name: "DeepSeek Flash".into(),
            supports_images: None,
        }];
        record_model_change(&mut state, None, Some("deepseek/deepseek-flash"));
        let last = state.coder.agent_messages.last().unwrap();
        assert_eq!(last.role, HaloAgentRole::System);
        assert_eq!(
            last.parts,
            vec![HaloAgentPart::ModelChanged {
                from: "Auto".into(),
                to: "DeepSeek Flash".into(),
            }]
        );
    }

    /// An empty chat has nothing to divide yet — the next turn just uses the
    /// new model.
    #[test]
    fn an_empty_chat_gets_no_divider() {
        let mut state = BridgeState::default();
        record_model_change(&mut state, None, Some("deepseek/deepseek-flash"));
        assert!(state.coder.agent_messages.is_empty());
    }

    /// Re-picking the same model is not a switch.
    #[test]
    fn the_same_model_records_nothing() {
        let mut state = state_with_transcript();
        let before = state.coder.agent_messages.len();
        record_model_change(&mut state, Some("a/b"), Some("a/b"));
        assert_eq!(state.coder.agent_messages.len(), before);
    }

    /// Clearing the model returns the picker to "Agent default".
    #[test]
    fn clearing_the_model_reports_auto() {
        let mut state = state_with_transcript();
        record_model_change(&mut state, Some("a/b"), None);
        let last = state.coder.agent_messages.last().unwrap();
        assert_eq!(
            last.parts,
            vec![HaloAgentPart::ModelChanged {
                from: "b".into(),
                to: "Auto".into(),
            }]
        );
    }
}

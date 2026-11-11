//! Settings: ports, storage housekeeping, app-data reset and CA certificate management.

use super::*;
use halo_backend::commands::shell::ShellAppearance;

impl Bridge {
    // ── Settings ───────────────────────────────────────────────────────────

    pub fn set_settings_tab(tab: SettingsTab, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.settings_tab = tab;
        });
        match tab {
            // Storage sizes are read off disk, so refresh on entry.
            SettingsTab::General => {
                let _ = Self::refresh_storage(cx);
            }
            // The server list is probed from `$PATH`, so refresh on entry.
            SettingsTab::CoderLanguageServers => Self::coder_lsp_refresh_servers(cx),
            // Google credentials live in the keychain / token file, and the
            // browser row probes a binary that may have been installed since
            // the last visit.
            SettingsTab::Agent => {
                Self::antigravity_acp_refresh_auth(cx);
                let _ = Self::halo_agent_refresh_browser(cx);
                let _ = Self::halo_agent_refresh_desktop(cx);
            }
            _ => {}
        }
    }

    /// Pick the interface typeface (Settings → Appearance → Typography).
    ///
    /// `None` means "whatever the theme ships" (Geist). The value is persisted
    /// with the shell layout, and the UI re-applies it to the live theme.
    pub fn set_app_font_family(family: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.appearance.font_family = family;
        });
        Self::persist_shell_layout(cx);
    }

    /// Pick the monospace typeface (Settings → Appearance → Typography).
    ///
    /// `None` means Geist Mono (bundled). Used by diffs, logs, terminals, and
    /// other mono surfaces that read `theme.mono_font_family`.
    pub fn set_app_mono_font_family(family: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.appearance.mono_font_family = family;
        });
        Self::persist_shell_layout(cx);
    }

    /// Set the interface base size in px — also the window rem size.
    ///
    /// Clamped to `11..=20` so a stray value cannot make the chrome unusable.
    pub fn set_app_font_size(size: u16, cx: &mut App) {
        let size = size.clamp(11, 20);
        Self::state(cx).update(cx, |s| {
            s.appearance.font_size = Some(size);
        });
        Self::persist_shell_layout(cx);
    }

    /// Drop typography overrides and fall back to Geist + Geist Mono at 16px.
    pub fn reset_app_font(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.appearance = ShellAppearance::default();
        });
        Self::persist_shell_layout(cx);
    }

    /// Pin or unpin a tool as an icon on the shell desktop (Start menu
    /// control + desktop tile menu). Persisted with the shell layout.
    pub fn set_desktop_pin(kind: WorkKind, pinned: bool, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            let has = s.pinned_apps.contains(&kind);
            if pinned && !has {
                s.pinned_apps.push(kind);
            } else if !pinned && has {
                s.pinned_apps.retain(|k| *k != kind);
            }
        });
        Self::persist_shell_layout(cx);
    }

    /// Flip the desktop pin for a tool.
    pub fn toggle_desktop_pin(kind: WorkKind, cx: &mut App) {
        let pinned = !Self::state(cx).read(cx).pinned_apps.contains(&kind);
        Self::set_desktop_pin(kind, pinned, cx);
    }

    /// Advance or restore the first-run onboarding stepper (persisted).
    pub fn set_onboarding_step(step: u8, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.onboarding_step = step;
        });
        Self::persist_shell_layout(cx);
    }

    /// Dismiss or finish first-run onboarding (persisted).
    pub fn complete_onboarding(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.onboarding_completed = true;
        });
        Self::persist_shell_layout(cx);
    }

    pub fn set_preferred_ports(http: u16, tls: u16, cx: &mut App) -> BridgeResult<()> {
        if !(1..=65535).contains(&http) {
            return Err(BridgeError::message(
                "HTTP port must be between 1 and 65535",
            ));
        }
        if !(1..=65535).contains(&tls) {
            return Err(BridgeError::message("TLS port must be between 1 and 65535"));
        }
        Self::state(cx).update(cx, |s| {
            s.preferred_http_port = http;
            s.preferred_tls_port = tls;
            s.proxy.default_port = http;
            s.apply_ok_msg(format!("Preferred ports set to HTTP {http} / TLS {tls}"));
        });
        Ok(())
    }

    pub fn reset_preferred_ports(cx: &mut App) {
        let _ = Self::set_preferred_ports(8888, 8443, cx);
    }

    pub fn refresh_storage(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        match backend.get_storage_info() {
            Ok(info) => {
                Self::state(cx).update(cx, |s| s.storage = Some(info));
                Ok(())
            }
            Err(err) => {
                Self::state(cx).update(cx, |s| s.apply_err(err.clone()));
                Err(err)
            }
        }
    }

    pub fn delete_storage_artifact(artifact: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let state = Self::state(cx);
        if state.read(cx).storage_busy {
            return Ok(());
        }

        state.update(cx, |s| {
            s.storage_busy = true;
            s.last_error = None;
            s.last_message = Some(format!("Clearing {artifact}…"));
        });

        let artifact = artifact.to_string();
        Self::run_blocking(
            cx,
            {
                let backend = backend.clone();
                let artifact = artifact.clone();
                move || backend.delete_storage_artifact(&artifact)
            },
            move |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.storage_busy = false;
                    match result {
                        Ok(res) => {
                            s.apply_ok_msg(format!(
                                "Cleared {} ({} bytes)",
                                res.label, res.bytes_deleted
                            ));
                        }
                        Err(err) => s.apply_err(err),
                    }
                });
                let _ = Self::refresh_storage(cx);
                if artifact == "database" {
                    let _ = Self::refresh_history(cx);
                    let _ = Self::refresh_sessions(cx);
                }
            },
        );
        Ok(())
    }

    pub fn reset_all_app_data(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let state = Self::state(cx);
        if state.read(cx).storage_busy {
            return Ok(());
        }

        state.update(cx, |s| {
            s.storage_busy = true;
            s.last_error = None;
            s.last_message = Some("Resetting all local data…".into());
        });

        Self::run_blocking(
            cx,
            {
                let backend = backend.clone();
                move || backend.reset_all_app_data()
            },
            |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.storage_busy = false;
                    match result {
                        Ok(res) => {
                            s.history.clear();
                            s.selected_log_id = None;
                            s.selected_detail = None;
                            s.paused.clear();
                            s.apply_ok_msg(format!(
                                "Factory reset complete — {} files / {} bytes removed",
                                res.files_deleted, res.bytes_deleted
                            ));
                        }
                        Err(err) => s.apply_err(err),
                    }
                });
                let _ = Self::refresh_storage(cx);
            },
        );
        Ok(())
    }

    pub fn export_ca_cert(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let downloads = halo_backend::commands::cert::default_ca_download_dir();
        let _ = std::fs::create_dir_all(&downloads);
        let dialog = cx.prompt_for_new_path(&downloads, Some("halo-ca.pem"));

        Self::state(cx).update(cx, |s| {
            s.ca_busy = true;
            s.last_error = None;
            s.last_message = Some("Choose where to save the CA certificate…".into());
        });

        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let selected = match dialog.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    let _ = cx.update(|cx| {
                        Self::state(cx).update(cx, |s| {
                            s.ca_busy = false;
                            s.last_message = None;
                        });
                    });
                    return;
                }
                Ok(Err(err)) => {
                    let _ = cx.update(|cx| {
                        Self::state(cx).update(cx, |s| {
                            s.ca_busy = false;
                            s.apply_err(BridgeError::message(format!("Save dialog failed: {err}")));
                        });
                    });
                    return;
                }
                Err(_) => {
                    let _ = cx.update(|cx| {
                        Self::state(cx).update(cx, |s| {
                            s.ca_busy = false;
                            s.last_message = None;
                        });
                    });
                    return;
                }
            };

            let path = selected.display().to_string();
            let (tx, rx) = std::sync::mpsc::channel();
            let spawn_ok = std::thread::Builder::new()
                .name("halo-ca-export".into())
                .spawn({
                    let backend = backend.clone();
                    let path = path.clone();
                    move || {
                        let _ = tx.send(backend.export_ca_cert(&path));
                    }
                })
                .is_ok();

            let result = if !spawn_ok {
                Err(BridgeError::message("failed to spawn CA export worker"))
            } else {
                loop {
                    match rx.try_recv() {
                        Ok(result) => break result,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            executor.timer(Duration::from_millis(8)).await;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            break Err(BridgeError::message("CA export worker died"));
                        }
                    }
                }
            };

            let _ = cx.update(|cx| {
                Self::state(cx).update(cx, |s| {
                    s.ca_busy = false;
                    match result {
                        Ok(path) => s.apply_ok_msg(format!("CA certificate saved to {path}")),
                        Err(err) => s.apply_err(err),
                    }
                });
            });
        })
        .detach();
        Ok(())
    }

    pub fn regenerate_ca_cert(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            s.ca_busy = true;
            s.last_error = None;
            s.last_message = Some("Regenerating CA…".into());
        });
        Self::run_blocking(
            cx,
            {
                let backend = backend.clone();
                move || backend.regenerate_ca_cert()
            },
            |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.ca_busy = false;
                    match result {
                        Ok(()) => s.apply_ok_msg(
                            "CA regenerated. Re-install it in Keychain / browsers.".into(),
                        ),
                        Err(err) => s.apply_err(err),
                    }
                });
            },
        );
        Ok(())
    }

    pub fn trust_ca(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            s.ca_busy = true;
            s.last_error = None;
            s.last_message = Some("Installing CA…".into());
        });
        Self::run_blocking(
            cx,
            {
                let backend = backend.clone();
                move || backend.trust_ca()
            },
            |result, cx| {
                Self::state(cx).update(cx, |s| {
                    s.ca_busy = false;
                    match result {
                        Ok(msg) => s.apply_ok_msg(msg),
                        Err(err) => s.apply_err(err),
                    }
                });
            },
        );
        Ok(())
    }
}

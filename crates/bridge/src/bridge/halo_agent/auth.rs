//! Provider authentication for Halo Agent (Pi harness).
//!
//! Pi picks a provider from the model id (`anthropic/…`, `openai/…`,
//! `google/…`, `deepseek/…`, `ollama`). API keys live in coder settings.
//! Antigravity ACP keeps its own Google OAuth flow below.

use super::super::*;
use crate::work::AntigravityLoginStatus;

impl Bridge {
    pub fn active_provider_info(cx: &App) -> (&'static str, bool, Option<String>) {
        let state = Self::state(cx).read(cx);
        let model = state.coder.agent_model.as_deref().unwrap_or("");

        // Slice on chars, not bytes: a pasted BOM or any non-ASCII byte in
        // the key would panic a byte-index slice. Short keys reveal too much
        // even 4+4, so they stay fully masked.
        let mask_key = |k: &str| {
            let trimmed = k.trim();
            let count = trimmed.chars().count();
            if count < 12 {
                "••••••••••••".to_string()
            } else {
                let head: String = trimmed.chars().take(4).collect();
                let tail: String = trimmed.chars().skip(count - 4).collect();
                format!("{head}••••{tail}")
            }
        };

        if model.starts_with("openai/") {
            let has_key = state
                .coder_settings
                .openai_api_key
                .as_ref()
                .is_some_and(|k| !k.trim().is_empty())
                || std::env::var("OPENAI_API_KEY").is_ok_and(|k| !k.trim().is_empty());
            let detail = state
                .coder_settings
                .openai_api_key
                .as_ref()
                .map(|k| mask_key(k));
            ("OpenAI", has_key, detail)
        } else if model.starts_with("google/") {
            let has_key = state
                .coder_settings
                .gemini_api_key
                .as_ref()
                .is_some_and(|k| !k.trim().is_empty())
                || std::env::var("GEMINI_API_KEY").is_ok_and(|k| !k.trim().is_empty());
            let detail = state
                .coder_settings
                .gemini_api_key
                .as_ref()
                .map(|k| mask_key(k));
            ("Google Gemini", has_key, detail)
        } else if model.starts_with("deepseek/") {
            let has_key = state
                .coder_settings
                .deepseek_api_key
                .as_ref()
                .is_some_and(|k| !k.trim().is_empty())
                || std::env::var("DEEPSEEK_API_KEY").is_ok_and(|k| !k.trim().is_empty());
            let detail = state
                .coder_settings
                .deepseek_api_key
                .as_ref()
                .map(|k| mask_key(k));
            ("DeepSeek", has_key, detail)
        } else if model.starts_with("ollama") {
            let url = state
                .coder_settings
                .ollama_base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".into());
            ("Ollama", true, Some(url))
        } else {
            let has_key = state
                .coder_settings
                .anthropic_api_key
                .as_ref()
                .is_some_and(|k| !k.trim().is_empty())
                || std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.trim().is_empty());
            let detail = state
                .coder_settings
                .anthropic_api_key
                .as_ref()
                .map(|k| mask_key(k));
            ("Anthropic", has_key, detail)
        }
    }

    pub fn coder_provider_set_api_key(provider: &str, key: &str, cx: &mut App) -> BridgeResult<()> {
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err(BridgeError::message("API key cannot be empty"));
        }
        match provider {
            "Anthropic" => {
                Self::update_coder_settings(cx, |s| s.anthropic_api_key = Some(key));
                Ok(())
            }
            "OpenAI" => {
                Self::update_coder_settings(cx, |s| s.openai_api_key = Some(key));
                Ok(())
            }
            "Google Gemini" | "Google" => {
                Self::update_coder_settings(cx, |s| s.gemini_api_key = Some(key));
                Ok(())
            }
            "DeepSeek" => {
                Self::update_coder_settings(cx, |s| s.deepseek_api_key = Some(key));
                Ok(())
            }
            "Ollama" => {
                Self::update_coder_settings(cx, |s| s.ollama_base_url = Some(key));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn coder_provider_logout(provider: &str, cx: &mut App) -> BridgeResult<()> {
        match provider {
            "Anthropic" => {
                Self::update_coder_settings(cx, |s| s.anthropic_api_key = None);
                Ok(())
            }
            "OpenAI" => {
                Self::update_coder_settings(cx, |s| s.openai_api_key = None);
                Ok(())
            }
            "Google Gemini" | "Google" => {
                Self::update_coder_settings(cx, |s| s.gemini_api_key = None);
                Ok(())
            }
            "DeepSeek" => {
                Self::update_coder_settings(cx, |s| s.deepseek_api_key = None);
                Ok(())
            }
            "Ollama" => {
                Self::update_coder_settings(cx, |s| s.ollama_base_url = None);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    // ── Antigravity ACP (`agy_acp_server`) browser OAuth ────────────────────

    pub fn antigravity_acp_login_status(cx: &App) -> AntigravityLoginStatus {
        Self::state(cx).read(cx).antigravity_acp_login.clone()
    }

    /// Whether the ACP server holds Google credentials (keychain or token file).
    ///
    /// Reads the cached flag; call [`Self::antigravity_acp_refresh_auth`] to
    /// re-probe off the UI thread.
    pub fn antigravity_acp_signed_in(cx: &App) -> bool {
        Self::state(cx).read(cx).antigravity_acp_signed_in
    }

    /// Re-probe ACP Google credentials in the background (keychain read).
    pub fn antigravity_acp_refresh_auth(cx: &mut App) {
        Self::run_blocking(
            cx,
            || Ok(halo_backend::commands::halo_agent::antigravity_acp_is_signed_in()),
            |result, cx| {
                if let Ok(signed_in) = result {
                    Self::state(cx).update(cx, |s| {
                        s.antigravity_acp_signed_in = signed_in;
                    });
                    if signed_in {
                        Self::close_provider_sheet_if_usable(cx);
                    }
                }
            },
        );
    }

    /// Start Google browser OAuth for the ACP harness. `agy_acp_server` opens
    /// the sign-in URL itself and prints it on stderr. A stored token returns
    /// without a browser.
    pub fn coder_antigravity_acp_begin_login(cx: &mut App) -> BridgeResult<()> {
        if Self::state(cx).read(cx).antigravity_acp_login.is_waiting() {
            return Ok(());
        }
        let custom = Self::state(cx)
            .read(cx)
            .coder_settings
            .antigravity_executable_path
            .clone();
        Self::state(cx).update(cx, |s| {
            s.antigravity_acp_login = AntigravityLoginStatus::Waiting { auth_url: None };
            s.last_error = None;
        });

        use halo_backend::commands::halo_agent::AntigravityAuthEvent;
        let (tx, rx) = std::sync::mpsc::sync_channel::<AntigravityAuthEvent>(8);
        cx.spawn(async move |cx: &mut AsyncApp| {
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let custom_for_worker = custom.clone();
            let spawn_ok = std::thread::Builder::new()
                .name("antigravity-acp-login".into())
                .spawn(move || {
                    let result = halo_backend::commands::halo_agent::antigravity_acp_run_login(
                        custom_for_worker.as_deref(),
                        |event| {
                            let _ = tx.send(event);
                        },
                    );
                    let _ = done_tx.send(result);
                })
                .is_ok();

            if !spawn_ok {
                let _ = cx.update(|cx| {
                    Self::state(cx).update(cx, |s| {
                        s.antigravity_acp_login = AntigravityLoginStatus::Failed {
                            message: "Cannot start Antigravity ACP login worker".into(),
                        };
                    });
                });
                return;
            }

            loop {
                match rx.try_recv() {
                    Ok(AntigravityAuthEvent::AuthUrl(url)) => {
                        let _ = cx.update(|cx| {
                            Self::state(cx).update(cx, |s| {
                                s.antigravity_acp_login = AntigravityLoginStatus::Waiting {
                                    auth_url: Some(url),
                                };
                            });
                        });
                    }
                    Ok(AntigravityAuthEvent::SignedIn) => {
                        let _ = cx.update(|cx| {
                            Self::state(cx).update(cx, |s| {
                                s.antigravity_acp_login = AntigravityLoginStatus::Idle;
                                s.antigravity_acp_signed_in = true;
                                s.last_message =
                                    Some("Signed in to Google Antigravity (ACP)".into());
                            });
                            Self::close_provider_sheet_if_usable(cx);
                        });
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty)
                    | Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                }

                match done_rx.try_recv() {
                    Ok(Ok(())) => {
                        let _ = cx.update(|cx| {
                            Self::state(cx).update(cx, |s| {
                                s.antigravity_acp_login = AntigravityLoginStatus::Idle;
                                s.last_message =
                                    Some("Signed in to Google Antigravity (ACP)".into());
                            });
                            Self::close_provider_sheet_if_usable(cx);
                        });
                        return;
                    }
                    Ok(Err(message)) => {
                        let _ = cx.update(|cx| {
                            Self::state(cx).update(cx, |s| {
                                // Prefer an earlier success over a late worker error.
                                if !matches!(s.antigravity_acp_login, AntigravityLoginStatus::Idle)
                                {
                                    s.antigravity_acp_login = AntigravityLoginStatus::Failed {
                                        message: message.clone(),
                                    };
                                    s.last_error = Some(message);
                                    s.last_error_info = false;
                                }
                            });
                        });
                        return;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        let still_waiting = cx.update(|cx| {
                            Self::state(cx).read(cx).antigravity_acp_login.is_waiting()
                        });
                        if !still_waiting {
                            return;
                        }
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(50))
                            .await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let _ = cx.update(|cx| {
                            Self::state(cx).update(cx, |s| {
                                if s.antigravity_acp_login.is_waiting() {
                                    s.antigravity_acp_login = AntigravityLoginStatus::Failed {
                                        message: "Antigravity ACP login ended unexpectedly".into(),
                                    };
                                }
                            });
                        });
                        return;
                    }
                }
            }
        })
        .detach();
        Ok(())
    }

    pub fn coder_antigravity_acp_cancel_login(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if s.antigravity_acp_login.is_waiting() {
                s.antigravity_acp_login = AntigravityLoginStatus::Idle;
            }
        });
    }

    /// Sign out of the ACP harness (server clears keychain + token file).
    pub fn coder_antigravity_acp_logout(cx: &mut App) -> BridgeResult<()> {
        let custom = Self::state(cx)
            .read(cx)
            .coder_settings
            .antigravity_executable_path
            .clone();
        Self::state(cx).update(cx, |s| {
            s.antigravity_acp_login = AntigravityLoginStatus::Idle;
        });
        Self::run_blocking(
            cx,
            move || {
                halo_backend::commands::halo_agent::antigravity_acp_logout(custom.as_deref())
                    .map_err(BridgeError::message)
            },
            |result, cx| match result {
                Ok(()) => {
                    Self::state(cx).update(cx, |s| {
                        s.antigravity_acp_signed_in = false;
                        s.last_message = Some("Signed out of Google Antigravity (ACP)".into());
                    });
                }
                Err(e) => {
                    let message = e.to_string();
                    Self::state(cx).update(cx, |s| {
                        s.last_error = Some(message.clone());
                        s.last_error_info = false;
                        s.last_message = Some(format!("Antigravity sign-out failed: {message}"));
                    });
                    // Sign-out may have half-succeeded (RPC failed, cache still
                    // stale) — re-probe rather than trust either answer.
                    Self::antigravity_acp_refresh_auth(cx);
                }
            },
        );
        Ok(())
    }

    pub fn coder_antigravity_acp_clear_login_error(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if matches!(
                s.antigravity_acp_login,
                AntigravityLoginStatus::Failed { .. }
            ) {
                s.antigravity_acp_login = AntigravityLoginStatus::Idle;
            }
        });
    }
}

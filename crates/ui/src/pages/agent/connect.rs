//! Provider authentication card — stacked above the prompt input (same column
//! as harness setup), so it stays aligned with the composer.

use celestia_ui::components::Input;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::Sizable as _;
use halo_bridge::{AntigravityLoginStatus, Bridge, BridgeState, HarnessReadiness, SettingsTab};

use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};
use crate::pages::editors::ToolEditors;

/// Provider auth card above the prompt; `None` once the active provider is connected.
/// Suppressed while shell onboarding or the harness setup card above the prompt is up.
pub(super) fn render_auth_card(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> Option<AnyElement> {
    // Setup gate is per selected harness; Pi upgrade only applies when Pi is selected.
    let setup = !state.coder.agent_harness_installed
        || state.coder.agent_state_blocked
        || (state.coder.agent_harness == "pi" && state.coder.agent_pi_needs_upgrade);
    if state.coder.agent_harness == "antigravity" {
        if setup {
            return None;
        }
        let waiting = state.antigravity_acp_login.is_waiting();
        // Hide once Google auth is known, a turn is live, or this chat already
        // holds an ACP session id (models work without a second sign-in).
        let session_ready = state
            .coder
            .agent_sessions
            .iter()
            .find(|s| s.id == state.coder.agent_active_session_id)
            .is_some_and(|s| s.harness_session_id.is_some());
        if (state.antigravity_acp_signed_in || state.coder.agent_running || session_ready)
            && !waiting
        {
            return None;
        }
        return Some(antigravity_sign_in_card(state, cx));
    }
    if matches!(state.coder.agent_harness.as_str(), "opencode" | "cursor") {
        // OpenCode and Cursor handle auth via their setup card or native CLI login.
        return None;
    }
    if !state.onboarding_completed || setup {
        return None;
    }

    let (provider, is_authenticated, _detail) = Bridge::active_provider_info(cx);
    if is_authenticated {
        return None;
    }

    Some(render_disconnected(provider, state, editors, cx))
}

fn provider_id_from_display(provider: &str, fallback: &str) -> &'static str {
    match provider {
        "Anthropic" => "anthropic",
        "OpenAI" => "openai",
        "Google Gemini" => "gemini",
        "DeepSeek" => "deepseek",
        "Ollama" => "ollama",
        _ => match fallback {
            "openai" => "openai",
            "gemini" => "gemini",
            "deepseek" => "deepseek",
            "ollama" => "ollama",
            _ => "anthropic",
        },
    }
}

fn render_disconnected(
    provider: &'static str,
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let warning = theme.warning;
    let chip_bg = warning.opacity(0.14);
    let chip_radius = px(6.);

    let selected_id = if state.coder.agent_inline_key_provider.is_empty() {
        provider_id_from_display(provider, "anthropic")
    } else {
        match state.coder.agent_inline_key_provider.as_str() {
            "openai" => "openai",
            "gemini" => "gemini",
            "deepseek" => "deepseek",
            "ollama" => "ollama",
            _ => "anthropic",
        }
    };

    let (selected_label, desc, input_entity, save_label) = match selected_id {
        "openai" => (
            "OpenAI",
            "Enter your OpenAI API key (sk-…) to use GPT and o-series models, or sign in via Settings.",
            &editors.coder_settings_openai_key,
            "Save key",
        ),
        "gemini" => (
            "Google Gemini",
            "Enter your Google AI Studio API key (AIza…) to use Gemini models.",
            &editors.coder_settings_gemini_key,
            "Save key",
        ),
        "deepseek" => (
            "DeepSeek",
            "Enter your DeepSeek API key (sk-…) to use DeepSeek models.",
            &editors.coder_settings_deepseek_key,
            "Save key",
        ),
        "ollama" => (
            "Ollama",
            "Enter your local Ollama base URL (default: http://localhost:11434/v1).",
            &editors.coder_settings_ollama_url,
            "Save URL",
        ),
        _ => (
            "Anthropic",
            "Enter your Anthropic API key (sk-ant-…) to use Claude models.",
            &editors.coder_settings_anthropic_key,
            "Save key",
        ),
    };

    let providers: &[(&str, &str)] = &[
        ("anthropic", "Anthropic"),
        ("openai", "OpenAI"),
        ("gemini", "Gemini"),
        ("deepseek", "DeepSeek"),
        ("ollama", "Ollama"),
    ];

    let mut provider_pills = div().flex().flex_wrap().items_center().gap(px(4.));
    for (id, label) in providers {
        let active = *id == selected_id;
        let target_id = (*id).to_string();
        provider_pills = provider_pills.child(
            Button::new(SharedString::from(format!("coder-inline-provider-{id}")))
                .variant(if active {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Small)
                .label(*label)
                .on_click(move |_, _, cx| {
                    Bridge::halo_agent_set_inline_key_provider(&target_id, cx);
                }),
        );
    }

    let input_for_save = input_entity.clone();
    let provider_for_save = selected_id.to_string();

    let inline_input_row = div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Input::new(input_entity).small()),
        )
        .child(
            Button::new("coder-provider-save-inline-key")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Check)
                .label(save_label)
                .on_click(move |_, window, cx| {
                    let val = input_for_save.read(cx).value().to_string();
                    let _ = Bridge::halo_agent_save_inline_api_key(&provider_for_save, &val, cx);
                    if provider_for_save != "ollama" {
                        input_for_save.update(cx, |state, cx| {
                            state.set_value("", window, cx);
                        });
                    }
                }),
        )
        .child(
            Button::new("coder-provider-open-settings")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Gear)
                .label("Settings…")
                .on_click(|_, _, cx| {
                    crate::windows::open_settings_window(SettingsTab::Agent, cx);
                }),
        );

    let ready_alternatives: Vec<(String, String)> = state
        .coder
        .agent_harness_readiness
        .iter()
        .filter(|r| r.id != state.coder.agent_harness && r.is_usable())
        .map(|r| {
            (
                r.id.clone(),
                HarnessReadiness::display_name(&r.id).to_string(),
            )
        })
        .collect();

    let card = div()
        .id("coder-provider-auth-card")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.))
        .px(px(14.))
        .py(px(12.))
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_start()
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(28.))
                        .rounded(chip_radius)
                        .bg(chip_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::Key)
                                .size(px(14.))
                                .color(warning),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_sm()
                                        .line_height(CARD_TITLE_LINE)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(foreground)
                                        .child(format!("Connect {selected_label}")),
                                )
                                .child(
                                    div()
                                        .px(px(7.))
                                        .py(px(2.))
                                        .rounded(chip_radius)
                                        .bg(chip_bg)
                                        .text_xs()
                                        .text_color(warning)
                                        .child("API key needed"),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .line_height(META_LINE_HEIGHT)
                                .text_color(muted)
                                .child(desc),
                        ),
                ),
        )
        .child(provider_pills)
        .child(inline_input_row)
        .when(!ready_alternatives.is_empty(), |el| {
            let mut row = div()
                .w_full()
                .pt(px(6.))
                .border_t_1()
                .border_color(theme.border.opacity(0.6))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Or switch to a ready harness:"),
                );
            for (alt_id, alt_name) in ready_alternatives {
                row = row.child(
                    Button::new(SharedString::from(format!("auth-switch-ready-{alt_id}")))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label(format!("Use {alt_name}"))
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::halo_agent_set_harness(&alt_id, cx);
                        }),
                );
            }
            el.child(row)
        });

    div()
        .id("coder-provider-auth-above-prompt")
        .w_full()
        .child(card)
        .into_any_element()
}

/// Google OAuth for Antigravity ACP — opens the browser via the existing login worker.
fn antigravity_sign_in_card(state: &BridgeState, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let login = &state.antigravity_acp_login;
    let waiting = login.is_waiting();
    let fail = match login {
        AntigravityLoginStatus::Failed { message } => Some(message.clone()),
        _ => None,
    };
    let auth_url = match login {
        AntigravityLoginStatus::Waiting { auth_url } => auth_url.clone(),
        _ => None,
    };
    let status = if waiting {
        "Complete Google sign-in in your browser to continue…".to_string()
    } else if let Some(message) = &fail {
        message.clone()
    } else {
        "Sign in with your Google account to use Antigravity models.".to_string()
    };
    let status_color = if fail.is_some() {
        theme.danger
    } else {
        theme.muted_foreground
    };
    let label = if waiting {
        "Cancel"
    } else if fail.is_some() {
        "Try again"
    } else {
        "Sign in with Google"
    };

    let mut actions = div().flex().flex_wrap().items_center().gap_2().child(
        Button::new("agent-antigravity-sign-in")
            .variant(if waiting {
                ButtonVariant::Ghost
            } else {
                ButtonVariant::Primary
            })
            .size(ButtonSize::Small)
            .leading_icon(if waiting {
                PhosphorIcon::X
            } else {
                PhosphorIcon::GoogleLogo
            })
            .label(label)
            .on_click(|_, _, cx| {
                if Bridge::antigravity_acp_login_status(cx).is_waiting() {
                    Bridge::coder_antigravity_acp_cancel_login(cx);
                    return;
                }
                Bridge::coder_antigravity_acp_clear_login_error(cx);
                let _ = Bridge::coder_antigravity_acp_begin_login(cx);
            }),
    );
    if let Some(url) = auth_url {
        actions = actions.child(
            Button::new("agent-antigravity-open-url")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Small)
                .label("Open sign-in page")
                .on_click(move |_, _, cx| {
                    cx.open_url(&url);
                }),
        );
    }

    div()
        .id("agent-antigravity-sign-in")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.))
        .px(px(14.))
        .py(px(12.))
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .child(
            div()
                .text_sm()
                .line_height(CARD_TITLE_LINE)
                .font_weight(gpui::FontWeight::NORMAL)
                .child("Sign in to Antigravity"),
        )
        .child(
            div()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(status_color)
                .child(status),
        )
        .child(actions)
        .into_any_element()
}

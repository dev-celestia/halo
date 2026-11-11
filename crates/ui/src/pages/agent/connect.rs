//! Provider authentication card — stacked above the prompt input (same column
//! as harness setup), so it stays aligned with the composer.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{AntigravityLoginStatus, Bridge, BridgeState, SettingsTab};

use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};

/// Provider auth card above the prompt; `None` once the active provider is connected.
/// Suppressed while shell onboarding or the harness setup card above the prompt is up.
pub(super) fn render_auth_card(state: &BridgeState, cx: &App) -> Option<AnyElement> {
    // Setup gate is per selected harness; Pi upgrade only applies when Pi is selected.
    let setup = !state.coder.agent_harness_installed
        || (state.coder.agent_harness == "pi" && state.coder.agent_pi_needs_upgrade);
    if state.coder.agent_harness == "antigravity" {
        if !state.coder.agent_harness_installed {
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
        // OpenCode and Cursor own their auth; Halo's Pi provider cards do not apply.
        return None;
    }
    if !state.onboarding_completed || setup {
        return None;
    }

    let (provider, is_authenticated, _detail) = Bridge::active_provider_info(cx);
    if is_authenticated {
        return None;
    }

    Some(render_disconnected(provider, cx))
}

fn render_disconnected(provider: &'static str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let warning = theme.warning;
    let chip_bg = warning.opacity(0.14);
    let chip_radius = px(6.);

    let desc = match provider {
        "Anthropic" => {
            "Configure your Anthropic API key in Coder Settings → Agent before sending prompts."
        }
        "OpenAI" => {
            "Configure your OpenAI API key in Coder Settings → Agent before sending prompts."
        }
        "Google Gemini" => {
            "Configure your Gemini API key in Coder Settings → Agent before sending prompts."
        }
        "DeepSeek" => {
            "Add a DeepSeek API key under Coder Settings → Agent → DeepSeek, then pick V4 Flash or V4 Pro."
        }
        "Ollama" => {
            "Set your Ollama base URL in Coder Settings → Agent (default: http://localhost:11434/v1)."
        }
        _ => {
            "Authenticate or configure an API key in Coder Settings → Agent before sending prompts."
        }
    };

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
                        .size(px(26.))
                        .rounded(chip_radius)
                        .bg(chip_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Phosphor::new(PhosphorIcon::Key)
                                .size(px(13.))
                                .color(warning),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .px(px(7.))
                                .py(px(2.))
                                .rounded(chip_radius)
                                .bg(chip_bg)
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(warning)
                                .child("AUTH REQUIRED"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .line_height(CARD_TITLE_LINE)
                                .font_weight(FontWeight::NORMAL)
                                .text_color(foreground)
                                .child(format!("Authenticate {provider}")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .line_height(META_LINE_HEIGHT)
                                .text_color(muted)
                                .child(desc.to_string()),
                        ),
                ),
        );

    let card = card.child(
        Button::new("coder-provider-open-settings")
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Small)
            .leading_icon(PhosphorIcon::Gear)
            .label("Open Agent Settings")
            .on_click(|_, _, cx| {
                crate::windows::open_settings_window(SettingsTab::Agent, cx);
            }),
    );

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
        "Waiting for Google sign-in in your browser…".to_string()
    } else if let Some(message) = &fail {
        message.clone()
    } else {
        "Sign in with Google to use Antigravity models.".to_string()
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
                .label("Open page")
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

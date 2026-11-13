//! Quiet status strip under the composer: provider auth + branch checkout.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, BridgeState, GitState, WorkKind};

use super::super::model_picker::{harness_mark, render_mark};
use super::super::CARD_TITLE_LINE;

/// One line under the card: branch + sign-out on the right once the provider
/// is connected. Disconnected providers get the floating auth card instead
/// (see `agent::connect`).
pub(crate) fn render_checkout_row(state: &BridgeState, cx: &App) -> impl IntoElement {
    let harness_id = state.coder.agent_harness.as_str();
    let readiness_subtitle = state
        .coder
        .agent_harness_readiness
        .iter()
        .find(|r| r.id == harness_id)
        .and_then(|r| {
            let sub = r.subtitle.trim();
            (!sub.is_empty()).then_some(sub)
        });

    enum SignOutAction {
        PiProvider(String),
        Antigravity,
    }

    // Harness identity is the brand mark only — no "OpenCode ·" / "Cursor ·" label.
    let (status_text, sign_out): (Option<String>, Option<SignOutAction>) = match harness_id {
        "pi" => {
            let (provider, is_authenticated, detail) = Bridge::active_provider_info(cx);
            if is_authenticated {
                let text = match detail {
                    Some(info) if !info.is_empty() => format!("{provider} · {info}"),
                    _ => format!("{provider} · Connected"),
                };
                (
                    Some(text),
                    Some(SignOutAction::PiProvider(provider.to_string())),
                )
            } else {
                (None, None)
            }
        }
        "antigravity" => {
            if state.coder.agent_harness_installed && state.antigravity_acp_signed_in {
                let text = readiness_subtitle
                    .map(str::to_string)
                    .unwrap_or_else(|| "Signed in with Google".to_string());
                (Some(text), Some(SignOutAction::Antigravity))
            } else {
                (None, None)
            }
        }
        "cursor" => {
            if state.coder.agent_harness_installed && !state.coder.agent_state_blocked {
                let text = readiness_subtitle
                    .map(str::to_string)
                    .unwrap_or_else(|| "Ready".to_string());
                (Some(text), None)
            } else {
                (None, None)
            }
        }
        "opencode" => {
            if state.coder.agent_harness_installed && !state.coder.agent_state_blocked {
                let text = readiness_subtitle
                    .map(str::to_string)
                    .unwrap_or_else(|| "Ready".to_string());
                (Some(text), None)
            } else {
                (None, None)
            }
        }
        _ => (None, None),
    };

    let muted = cx.theme().muted_foreground;
    let busy = state.git_busy.is_some();

    let Some(status) = status_text else {
        return div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .px(px(2.))
            .child(branch_switcher(&state.git, busy, muted))
            .into_any_element();
    };

    let (mark, tint) = harness_mark(harness_id);
    let harness_glyph = render_mark(mark, tint, px(14.), PhosphorIcon::CheckCircle, muted);

    div()
        .id("coder-agent-status-row")
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(2.))
        .child(harness_glyph)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_sm()
                .line_height(CARD_TITLE_LINE)
                .text_color(muted)
                .child(status),
        )
        .child(branch_switcher(&state.git, busy, muted))
        .when_some(sign_out, |el, action| {
            el.child(
                Button::new("coder-provider-disconnect")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .label("Sign out")
                    .on_click(move |_, _, cx| match &action {
                        SignOutAction::PiProvider(prov) => {
                            let _ = Bridge::coder_provider_logout(prov, cx);
                        }
                        SignOutAction::Antigravity => {
                            let _ = Bridge::coder_antigravity_acp_logout(cx);
                        }
                    }),
            )
        })
        .into_any_element()
}

/// Branch chip that switches checkout via a local-branch menu (same actions as
/// Source Control). "Create New Branch…" jumps to the git panel's inline field.
fn branch_switcher(git: &GitState, busy: bool, muted: Hsla) -> AnyElement {
    let current = git.branch.clone().unwrap_or_default();
    let label = if git.not_a_repo {
        "No git repo".to_string()
    } else if current.is_empty() {
        "Detached".to_string()
    } else {
        current.clone()
    };

    if git.not_a_repo {
        return div()
            .id("coder-agent-checkout-branch")
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .py(px(5.))
            .rounded(px(6.))
            .child(
                Phosphor::new(PhosphorIcon::GitBranch)
                    .size(px(13.))
                    .color(muted),
            )
            .child(div().text_sm().text_color(muted).child(label))
            .into_any_element();
    }

    let mut branches = git.local_branches.clone();
    // Unborn HEAD names are missing from `for-each-ref`; keep the chip's own
    // branch in the list so the current checkout stays selectable/ticked.
    if !current.is_empty() && !branches.iter().any(|b| b == &current) {
        branches.insert(0, current.clone());
    }

    let tooltip = if current.is_empty() {
        "Switch or create branch".to_string()
    } else {
        format!("{current} — switch or create branch")
    };

    Button::new("coder-agent-checkout-branch")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::GitBranch)
        .label(label)
        .dropdown_caret(true)
        .tooltip(tooltip)
        .disabled(busy)
        .dropdown_menu({
            move |menu, _window, _cx| {
                let mut menu = menu;
                for branch in &branches {
                    let is_current = branch == &current;
                    let name = branch.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(is_current)
                            .on_click(move |_, _, cx| {
                                if !is_current {
                                    let _ = Bridge::git_checkout(&name, cx);
                                }
                            }),
                    );
                }
                if !branches.is_empty() {
                    menu = menu.separator();
                }
                menu.item(
                    PopupMenuItem::new("Create New Branch…").on_click(move |_, _, cx| {
                        let _ = Bridge::focus_or_open_work_tab(WorkKind::SourceControl, cx);
                        Bridge::git_begin_new_branch(cx);
                    }),
                )
            }
        })
        .into_any_element()
}

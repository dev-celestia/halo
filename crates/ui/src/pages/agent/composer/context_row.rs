//! Quiet status strip under the composer: provider auth + branch checkout.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, BridgeState, GitState, WorkKind};

use super::super::CARD_TITLE_LINE;

/// One line under the card: branch + sign-out on the right once the provider
/// is connected. Disconnected providers get the floating auth card instead
/// (see `agent::connect`).
pub(crate) fn render_checkout_row(state: &BridgeState, cx: &App) -> impl IntoElement {
    let (provider, is_authenticated, detail) = if state.coder.agent_harness == "pi" {
        Bridge::active_provider_info(cx)
    } else {
        ("", false, None)
    };
    let muted = cx.theme().muted_foreground;
    let busy = state.git_busy.is_some();

    if !is_authenticated {
        return div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .px(px(2.))
            .child(branch_switcher(&state.git, busy, muted))
            .into_any_element();
    }

    let success = cx.theme().success;
    let status = match detail {
        Some(info) if !info.is_empty() => format!("{provider} Authenticated · {info}"),
        _ => format!("{provider} Authenticated"),
    };
    let prov = provider.to_string();

    div()
        .id("coder-agent-status-row")
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(2.))
        .child(
            Phosphor::new(PhosphorIcon::CheckCircle)
                .size(px(14.))
                .color(success),
        )
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
        .child(
            Button::new("coder-provider-disconnect")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label("Sign out")
                .on_click(move |_, _, cx| {
                    let _ = Bridge::coder_provider_logout(&prov, cx);
                }),
        )
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

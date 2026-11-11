//! Inline git failure card shared by the Source Control panes.
//!
//! A refused branch switch is the common failure and has a safe recovery —
//! stash the colliding edits, then retry — so it gets a card naming the target
//! branch and the files in the way. Everything else falls back to the message
//! git produced, with a way to dismiss it.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, GitCheckoutBlock, SourceControlPane};

/// Paths listed before the rest are summarized as "+N more".
const MAX_LISTED_FILES: usize = 4;

/// The current git failure, if any, ready to place in a pane.
pub(crate) fn render_git_alert(state: &BridgeState, cx: &App) -> Option<AnyElement> {
    if let Some(block) = &state.git.checkout_block {
        return Some(checkout_blocked_card(block, state.git_busy.is_some(), cx));
    }
    let theme = cx.theme();
    let err = state.git.error.as_ref()?;
    Some(
        div()
            .w_full()
            .flex()
            .items_start()
            .justify_between()
            .gap_1()
            .px_2()
            .py_1()
            .rounded(theme.radius_sm())
            .bg(theme.danger.opacity(0.08))
            .text_xs()
            .text_color(theme.danger)
            .child(div().flex_1().min_w_0().child(err.clone()))
            .child(dismiss_button())
            .into_any_element(),
    )
}

fn checkout_blocked_card(block: &GitCheckoutBlock, busy: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let count = block.files.len();
    let summary = if count == 1 {
        "1 uncommitted file would be overwritten.".to_string()
    } else {
        format!("{count} uncommitted files would be overwritten.")
    };

    let mut card = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.danger.opacity(0.35))
        .bg(theme.danger.opacity(0.06))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Phosphor::new(PhosphorIcon::Warning)
                        .size(px(13.))
                        .color(theme.danger),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.danger)
                        .child(format!("Can't switch to '{}'", block.branch)),
                )
                .child(dismiss_button()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(summary),
        );

    for path in block.files.iter().take(MAX_LISTED_FILES) {
        card = card.child(
            div()
                .pl_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(theme.foreground.opacity(0.85))
                .child(path.clone()),
        );
    }
    if count > MAX_LISTED_FILES {
        card = card.child(
            div()
                .pl_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("+{} more", count - MAX_LISTED_FILES)),
        );
    }

    card.child(
        div()
            .flex()
            .items_center()
            .gap_1()
            .pt_0p5()
            .child(
                Button::new("sc-git-alert-stash")
                    .variant(ButtonVariant::Primary)
                    .size(ButtonSize::XSmall)
                    .leading_icon(PhosphorIcon::Archive)
                    .label("Stash & Switch")
                    .disabled(busy)
                    .tooltip("Stash local changes, then switch to this branch")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::git_recover_checkout(cx);
                    }),
            )
            .child(
                Button::new("sc-git-alert-changes")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::XSmall)
                    .label("View Changes")
                    .on_click(|_, _, cx| {
                        Bridge::select_source_control_pane(SourceControlPane::Changes, cx);
                    }),
            ),
    )
    .into_any_element()
}

fn dismiss_button() -> impl IntoElement {
    Button::new("sc-git-alert-dismiss")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .leading_icon(PhosphorIcon::X)
        .tooltip("Dismiss")
        .on_click(|_, _, cx| Bridge::git_dismiss_error(cx))
}

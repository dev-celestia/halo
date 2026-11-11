//! `@` file and `/` skill completion popup.
//!
//! Accepting a file or skill turns the trigger into an [`InlineToken`] so the
//! composer can paint it as a coloured mention (file = blue + icon, skill =
//! orange), matching the Cursor-style chips in the prompt.

use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::InlineToken;
use celestia_ui::components::primitive::textarea::TextareaState;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, HaloAgentCompletionKind};

use crate::explorer_icons;

use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};

const LIST_MAX: Pixels = px(240.);
const ROW_HEIGHT: Pixels = px(32.);

/// Id prefix for file mentions (`file:@path`).
pub(super) const FILE_TOKEN_PREFIX: &str = "file:";
/// Id prefix for skill mentions (`skill:/name`).
pub(super) const SKILL_TOKEN_PREFIX: &str = "skill:";

pub(super) fn render_popup(
    state: &BridgeState,
    input: &Entity<TextareaState>,
    cx: &App,
) -> AnyElement {
    let Some(comp) = state.coder.agent_completion.as_ref() else {
        return Empty.into_any_element();
    };

    let theme = cx.theme();
    let border = theme.border;
    let surface = theme.popover;
    let accent = theme.accent;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let radius = theme.radius;
    let row_radius = theme.radius_sm();
    let kind = comp.kind;
    let selected = comp.selected;
    let title = match kind {
        HaloAgentCompletionKind::File => "Files",
        HaloAgentCompletionKind::Skill => "Skills",
    };

    let mut list = div()
        .id("coder-agent-completion-list")
        .w_full()
        .max_h(LIST_MAX)
        .overflow_y_scrollbar()
        .flex()
        .flex_col()
        .p(px(4.));

    if comp.items.is_empty() {
        list = list.child(
            div()
                .w_full()
                .px(px(10.))
                .py(px(12.))
                .text_xs()
                .text_color(muted)
                .child(match kind {
                    HaloAgentCompletionKind::File => "No files match.",
                    HaloAgentCompletionKind::Skill => "No skills found.",
                }),
        );
    } else {
        for (index, item) in comp.items.iter().enumerate() {
            let is_selected = index == selected;
            let label = item.label.clone();
            let detail = item.detail.clone();
            let detail_same = detail == item.label;
            let mark: AnyElement = match kind {
                HaloAgentCompletionKind::File if item.is_dir => {
                    explorer_icons::folder_icon(&label, false).into_any_element()
                }
                HaloAgentCompletionKind::File => {
                    explorer_icons::file_icon(&label).into_any_element()
                }
                HaloAgentCompletionKind::Skill => Phosphor::new(PhosphorIcon::Sparkle)
                    .size(px(13.))
                    .color(muted)
                    .into_any_element(),
            };
            let input = input.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!(
                        "coder-agent-completion-{index}"
                    )))
                    .w_full()
                    .h(ROW_HEIGHT)
                    .flex_none()
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(row_radius)
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(accent))
                    .hover(move |s| s.bg(accent))
                    .child(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .line_height(CARD_TITLE_LINE)
                            .text_color(foreground)
                            .child(label),
                    )
                    .when(!detail.is_empty() && !detail_same, |el| {
                        el.child(
                            div()
                                .flex_none()
                                .max_w(px(160.))
                                .truncate()
                                .text_xs()
                                .text_color(muted)
                                .child(detail),
                        )
                    })
                    .on_click(move |_, window, cx| {
                        let _ = Bridge::halo_agent_completion_select(index, cx);
                        let _ = apply_accept(&input, window, cx);
                    }),
            );
        }
    }

    celestia_ui::motion::menu_in_from(
        "coder-agent-completion-in",
        6.0,
        div()
            .id("coder-agent-completion")
            .w_full()
            .flex()
            .flex_col()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(surface)
            .overflow_hidden()
            .child(
                div()
                    .w_full()
                    .px(px(10.))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(border)
                    .text_xs()
                    .line_height(META_LINE_HEIGHT)
                    .text_color(muted)
                    .child(title),
            )
            .child(list),
    )
    .into_any_element()
}

/// Replace the active `@` / `/` token using the current Bridge selection.
///
/// Files and skills become atomic inline tokens (coloured chips). Directory
/// prefixes stay plain text so the user can keep browsing.
pub(crate) fn apply_accept(
    input: &Entity<TextareaState>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let (kind, range, item) = {
        let Some(comp) = Bridge::state(cx).read(cx).coder.agent_completion.clone() else {
            return false;
        };
        let Some(item) = comp.items.get(comp.selected).cloned() else {
            let _ = Bridge::halo_agent_completion_dismiss(cx);
            return false;
        };
        (comp.kind, comp.token_start..comp.token_end, item)
    };

    let draft = input.read(cx).value().to_string();
    if range.end > draft.len()
        || range.start > range.end
        || !draft.is_char_boundary(range.start)
        || !draft.is_char_boundary(range.end)
    {
        let _ = Bridge::halo_agent_completion_dismiss(cx);
        return false;
    }

    // Directory rows keep a trailing `/` so the next keystroke continues the
    // file search — leave them as plain text, not a chip.
    if item.is_dir || item.insert.ends_with('/') {
        input.update(cx, |input, cx| {
            input.set_selected_range(range.clone(), cx);
            input.replace(item.insert.clone(), window, cx);
        });
        let _ = Bridge::halo_agent_completion_dismiss(cx);
        let text = input.read(cx).value().to_string();
        let cursor = input.read(cx).cursor();
        let _ = Bridge::halo_agent_completion_sync(&text, cursor, cx);
        return true;
    }

    let token_text = item.insert.clone();
    let token = match kind {
        HaloAgentCompletionKind::File => InlineToken::new(
            format!("{FILE_TOKEN_PREFIX}{token_text}"),
            token_text.clone(),
        )
        .with_label(item.label),
        HaloAgentCompletionKind::Skill => InlineToken::new(
            format!("{SKILL_TOKEN_PREFIX}{token_text}"),
            token_text.clone(),
        )
        .with_label(token_text),
    };

    let inserted = input.update(cx, |input, cx| {
        input
            .replace_range_with_token(range, token, window, cx)
            .is_ok()
    });
    if !inserted {
        // Fallback: plain-text accept via the Bridge helper.
        let draft = input.read(cx).value().to_string();
        let Ok(Some((new_draft, _))) = Bridge::halo_agent_completion_accept(&draft, cx) else {
            return false;
        };
        input.update(cx, |input, cx| {
            input.set_value(new_draft, window, cx);
        });
        return true;
    }

    input.update(cx, |input, cx| {
        input.insert(" ", window, cx);
    });
    let _ = Bridge::halo_agent_completion_dismiss(cx);
    true
}

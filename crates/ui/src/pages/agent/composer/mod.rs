//! Full-page prompt composer — hero card, context chips, and send / stop actions.
//!
//! Empty sessions get a taller hero card. Once a transcript exists the same
//! structure collapses: draft on top, tools on a second row — never one crowded
//! control strip fighting the caret.

mod context_row;
mod controls;
mod mentions;

use celestia_ui::components::Textarea;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::input::InputState;
use celestia_ui::components::primitive::textarea::TextareaState;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{Sizable as _, Size};
use halo_bridge::{Bridge, BridgeState};

pub(super) use context_row::render_checkout_row;
use controls::{
    approval_picker_anchor, context_usage_chip, model_picker_anchor, plan_mode_chip,
    trailing_action,
};
pub(crate) use mentions::attachment_label;
pub(crate) use mentions::attachment_preview_tooltip;
use mentions::{render_attachment_chips, render_mention_token};

use crate::pages::editors::ToolEditors;

const FOLLOW_UP_PLACEHOLDER: &str = "Ask a follow-up or steer the agent…";
const HERO_PLACEHOLDER: &str = "Ask anything, edit files, or run commands… (@ files, / skills)";

pub(super) fn render_composer(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let compact = !state.coder.agent_messages.is_empty();
    let theme = cx.theme();
    let border = theme.border;
    let surface = theme.popover;
    let accent = theme.accent;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let primary = theme.primary;
    let primary_foreground = theme.primary_foreground;
    let danger = theme.danger;
    let danger_foreground = theme.danger_foreground;
    // Outer ≈ inner (radius_sm chips) + padding — concentric with the toolbar.
    let radius = theme.radius_lg;

    let running = state.coder.agent_running;
    let input = editors.halo_agent_input.clone();
    let draft = input.clone();
    let search = editors.halo_agent_model_search.clone();
    let draft_empty =
        input.read(cx).value().trim().is_empty() && state.coder.agent_attachments.is_empty();
    let attachments = state.coder.agent_attachments.clone();

    let card = if compact {
        compact_card(
            state,
            &attachments,
            &input,
            &draft,
            &search,
            running,
            draft_empty,
            border,
            surface,
            accent,
            foreground,
            muted,
            primary,
            primary_foreground,
            danger,
            danger_foreground,
            radius,
            cx,
        )
        .into_any_element()
    } else {
        hero_card(
            state,
            &attachments,
            &input,
            &draft,
            &search,
            running,
            draft_empty,
            border,
            surface,
            accent,
            foreground,
            muted,
            primary,
            primary_foreground,
            danger,
            danger_foreground,
            radius,
            cx,
        )
        .into_any_element()
    };

    div()
        .id("coder-agent-composer")
        .relative()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .when(state.coder.agent_completion.is_some(), |el| {
                    el.mb(px(6.))
                        .child(super::completion::render_popup(state, &draft, cx))
                }),
        )
        .child(card)
}

/// Keep textarea grow range + placeholder in sync with landing vs follow-up.
pub fn sync_composer_mode(
    editors: &ToolEditors,
    has_messages: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let want = if has_messages {
        FOLLOW_UP_PLACEHOLDER
    } else {
        HERO_PLACEHOLDER
    };
    let current = editors
        .halo_agent_input
        .read(cx)
        .presentation()
        .placeholder()
        .to_string();
    if current == want {
        return;
    }
    let (min_rows, max_rows) = if has_messages { (1, 6) } else { (2, 12) };
    editors.halo_agent_input.update(cx, |input, cx| {
        input.set_soft_wrap(true, window, cx);
        input.set_auto_grow(min_rows, max_rows, cx);
        input.set_placeholder(want, window, cx);
    });
}

fn attach_button() -> impl IntoElement {
    // Quiet + 14px matches the Plan chip so both toolbar icons share weight.
    Button::icon("coder-agent-attach", PhosphorIcon::Paperclip)
        .variant(ButtonVariant::Quiet)
        .size(ButtonSize::Icon)
        .icon_size(px(15.))
        .accessibility_label("Attach images")
        .tooltip("Attach images (png, jpeg, gif, webp) — or paste with ⌘V")
        .on_click(|_, _, cx| {
            let _ = Bridge::halo_agent_attach_images(cx);
        })
}

/// Inline hint for text-only models — attaching images is blocked, so warn
/// before the user tries (paste or picker) and name a vision alternative.
fn image_model_warning(state: &BridgeState, cx: &App) -> Option<AnyElement> {
    let model_id = state.coder.agent_model.as_deref()?;
    let catalog = &state.coder.agent_model_catalog;
    let model = catalog.iter().find(|model| model.id == model_id)?;
    if model.supports_images != Some(false) {
        return None;
    }
    let alternative = catalog
        .iter()
        .find(|model| model.supports_images == Some(true))
        .map(|model| model.display_name.as_str());
    let hint = match alternative {
        Some(alternative) => format!(
            "{} can't read images. Switch to {alternative} (or another vision-capable model) to attach them.",
            model.display_name
        ),
        None => format!(
            "{} can't read images. Pick a vision-capable model in Coder Settings to attach them.",
            model.display_name
        ),
    };
    let warning = cx.theme().warning;
    Some(
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(2.))
            .text_xs()
            .line_height(px(16.))
            .text_color(warning)
            .child(
                Phosphor::new(PhosphorIcon::Warning)
                    .size(px(12.))
                    .color(warning),
            )
            .child(hint)
            .into_any_element(),
    )
}

/// ⌘V with an image on the clipboard stages it as an attachment chip.
///
/// `w_full` + `min_w_0` force a bounded layout width so soft-wrap breaks at the
/// card edge instead of letting the longest line set the field's min width.
fn composer_textarea(input: &Entity<TextareaState>) -> Textarea {
    Textarea::new(input)
        .appearance(false)
        .bordered(false)
        .with_size(Size::Large)
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .token(render_mention_token)
        .on_paste(|item, _, cx| Bridge::halo_agent_attach_paste(item, cx))
}

/// 1px card frame painted *under* the card's own content.
///
/// gpui paints a div's border after its children (`Style::paint`: background →
/// children → border), so a card that hosts an overflowing flyout would draw its
/// frame straight across the open popup — the prompt's border showing through the
/// model picker. Painting the frame as the first child keeps it beneath.
fn card_frame(border: Hsla, radius: Pixels) -> AnyElement {
    div()
        .absolute()
        .top(px(0.))
        .left(px(0.))
        .right(px(0.))
        .bottom(px(0.))
        .rounded(radius)
        .border_1()
        .border_color(border)
        .into_any_element()
}

/// Shared toolbar: attach + approval + plan on the left, model + send on the right.
fn composer_toolbar(
    state: &BridgeState,
    draft: &Entity<TextareaState>,
    search: &Entity<InputState>,
    running: bool,
    draft_empty: bool,
    accent: Hsla,
    foreground: Hsla,
    muted: Hsla,
    primary: Hsla,
    primary_foreground: Hsla,
    danger: Hsla,
    danger_foreground: Hsla,
    cx: &App,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .child(div().flex_none().child(attach_button()))
        .child(approval_picker_anchor(state, muted, accent, cx))
        .child(plan_mode_chip(state.coder.agent_plan_mode, cx))
        .child(div().flex_1().min_w(px(8.)))
        .child(context_usage_chip(state, muted, accent, cx))
        .child(model_picker_anchor(
            state,
            running,
            foreground,
            muted,
            accent,
            search.clone(),
            draft.clone(),
            cx,
        ))
        .child(div().flex_none().child(trailing_action(
            running,
            draft_empty,
            draft.clone(),
            primary,
            primary_foreground,
            danger,
            danger_foreground,
        )))
}

/// Compact follow-up card — draft on top, tools underneath (same hierarchy as
/// the hero, tighter padding so the chat keeps the space).
fn compact_card(
    state: &BridgeState,
    attachments: &[String],
    input: &Entity<TextareaState>,
    draft: &Entity<TextareaState>,
    search: &Entity<InputState>,
    running: bool,
    draft_empty: bool,
    border: Hsla,
    surface: Hsla,
    accent: Hsla,
    foreground: Hsla,
    muted: Hsla,
    primary: Hsla,
    primary_foreground: Hsla,
    danger: Hsla,
    danger_foreground: Hsla,
    radius: Pixels,
    cx: &App,
) -> Div {
    div()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(10.))
        .pt(px(10.))
        .pb(px(8.))
        .rounded(radius)
        .bg(surface)
        .child(card_frame(border, radius))
        .when(!attachments.is_empty(), |el| {
            el.child(render_attachment_chips(attachments, cx))
                .children(image_model_warning(state, cx))
        })
        .child(
            div()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                .min_h(px(36.))
                .px(px(2.))
                .child(composer_textarea(input)),
        )
        .child(composer_toolbar(
            state,
            draft,
            search,
            running,
            draft_empty,
            accent,
            foreground,
            muted,
            primary,
            primary_foreground,
            danger,
            danger_foreground,
            cx,
        ))
}

/// Empty-session hero — taller draft area with tools on a second row.
fn hero_card(
    state: &BridgeState,
    attachments: &[String],
    input: &Entity<TextareaState>,
    draft: &Entity<TextareaState>,
    search: &Entity<InputState>,
    running: bool,
    draft_empty: bool,
    border: Hsla,
    surface: Hsla,
    accent: Hsla,
    foreground: Hsla,
    muted: Hsla,
    primary: Hsla,
    primary_foreground: Hsla,
    danger: Hsla,
    danger_foreground: Hsla,
    radius: Pixels,
    cx: &App,
) -> Div {
    div()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(12.))
        .rounded(radius)
        .bg(surface)
        .child(card_frame(border, radius))
        .when(!attachments.is_empty(), |el| {
            el.child(render_attachment_chips(attachments, cx))
                .children(image_model_warning(state, cx))
        })
        .child(
            div()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                .min_h(px(64.))
                .px(px(2.))
                .child(composer_textarea(input)),
        )
        .child(composer_toolbar(
            state,
            draft,
            search,
            running,
            draft_empty,
            accent,
            foreground,
            muted,
            primary,
            primary_foreground,
            danger,
            danger_foreground,
            cx,
        ))
}

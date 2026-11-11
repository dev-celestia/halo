//! Reply, user-turn, actions, and error-text heuristics.

use super::*;

/// Assistant reply: markdown on the surface (no card), fenced code in
/// [`super::super::markdown`]'s CodeBlock shower.
pub(crate) fn reply_text(
    state: &Entity<TextViewState>,
    extensions: MarkdownExtensions,
) -> impl IntoElement {
    super::super::markdown::render_reply(state, extensions)
}

/// User turn: attachments and prompt share one outlined bubble — the image is
/// part of the message, not a row floating above it. The 80% width cap comes
/// from `Bubble`'s own root style, which keeps a long prompt from running the
/// full column.
pub(crate) fn user_turn(
    msg: &halo_bridge::HaloAgentMessage,
    chat: &ChatKey,
    message_id: &str,
    cx: &App,
) -> impl IntoElement {
    let mut stack = div().flex().flex_col().gap(px(6.)).min_w_0();
    for (part_index, part) in msg.parts.iter().enumerate() {
        match part {
            HaloAgentPart::Attachments(paths) if !paths.is_empty() => {
                let part_id: SharedString = chat.part_id(message_id, part_index).into();
                stack = stack.child(attachment_chips(paths, part_id, cx));
            }
            HaloAgentPart::Text(text) if !text.trim().is_empty() => {
                stack = stack.child(text.clone());
            }
            _ => {}
        }
    }
    Bubble::new()
        .with_variant(BubbleVariant::Outline)
        .content(
            BubbleContent::new()
                .px(USER_PAD_X)
                .py(USER_PAD_Y)
                .rounded(USER_RADIUS)
                .text_base()
                .line_height(LINE_HEIGHT),
        )
        .child(stack)
}

/// What "copy message" puts on the clipboard: every text part in order, each
/// trimmed and joined by a blank line as Markdown source. Reasoning, tool
/// output and attachment paths are left out — the reader is copying the
/// answer, not the harness log.
pub(crate) fn message_source(msg: &halo_bridge::HaloAgentMessage) -> Option<String> {
    if !matches!(msg.role, HaloAgentRole::Assistant | HaloAgentRole::User) {
        return None;
    }
    let mut joined = String::new();
    for part in &msg.parts {
        let HaloAgentPart::Text(text) = part else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push_str("\n\n");
        }
        joined.push_str(text);
    }
    (!joined.is_empty()).then_some(joined)
}

pub(crate) fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Footer cluster: copy icon with turn token usage immediately beside it.
pub(crate) fn message_actions(
    message_id: &str,
    source: Option<String>,
    usage: Option<HaloAgentContextUsage>,
    cx: &App,
) -> MessageFooter {
    let muted = cx.theme().muted_foreground;
    // One compact chip so usage cannot drift away from the copy control across
    // the full-width Message footer.
    let mut cluster = div()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .h(px(20.));
    if let Some(source) = source {
        let id: SharedString = format!("{message_id}:copy-message").into();
        cluster = cluster.child(div().flex_none().h(px(20.)).w(px(20.)).child(copy_button(
            id,
            source,
            "Copy message",
            cx,
        )));
    }
    if let Some(usage) = usage {
        let label = format!(
            "{} in · {} out",
            format_tokens(usage.input_tokens),
            format_tokens(usage.output_tokens),
        );
        let tip = if usage.window_tokens > 0 {
            format!(
                "Context · {} / {} tokens · {} in · {} out",
                format_tokens(usage.used_tokens),
                format_tokens(usage.window_tokens),
                format_tokens(usage.input_tokens),
                format_tokens(usage.output_tokens),
            )
        } else {
            format!(
                "{} input · {} output tokens",
                format_tokens(usage.input_tokens),
                format_tokens(usage.output_tokens),
            )
        };
        cluster = cluster.child(
            div()
                .id(SharedString::from(format!("{message_id}:token-usage")))
                .flex_none()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted)
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .child(label),
        );
    }
    MessageFooter::new().content_inset(false).child(cluster)
}

pub(crate) fn copy_button(
    id: SharedString,
    text: String,
    label: &'static str,
    cx: &App,
) -> impl IntoElement {
    let flashing = copy_is_flashing(&id, cx);
    let flash_id = id.clone();
    Button::icon(
        id,
        if flashing {
            PhosphorIcon::Check
        } else {
            PhosphorIcon::Copy
        },
    )
    .variant(ButtonVariant::Ghost)
    .size(ButtonSize::IconSm)
    .accessibility_label(if flashing { "Copied" } else { label })
    .tooltip(if flashing { "Copied" } else { label })
    .on_click(move |_, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        copy_flash(flash_id.clone(), cx);
    })
}

/// System notice: secondary scale, so it reads as transcript furniture rather
/// than as part of the answer.
pub(crate) fn notice_text(text: String, color: Hsla) -> impl IntoElement {
    div()
        .w_full()
        .min_w_0()
        .text_xs()
        .line_height(META_LINE_HEIGHT)
        .text_color(color)
        .whitespace_normal()
        .child(text)
}

/// Divider announcing a mid-chat model switch: a hairline that runs to both
/// edges with the change spelled out in the gap.
///
/// The label sits *between* two rules rather than beside them so the row
/// reads as a section break in the thread — the point is "everything above
/// used one model, everything below uses another".
pub(crate) fn model_change_divider(from: &str, to: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let rule = theme.border.opacity(0.7);

    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(8.))
        .py(px(2.))
        .child(div().flex_1().min_w_0().h(px(1.)).bg(rule))
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted)
                .whitespace_nowrap()
                .child(format!("Model changed from {from} to {to}"))
                .child(
                    Phosphor::new(PhosphorIcon::Info)
                        .size(px(13.))
                        .color(muted.opacity(0.75)),
                ),
        )
        .child(div().flex_1().min_w_0().h(px(1.)).bg(rule))
}

/// A send that died because the CLI cannot write its own state directory.
///
/// Resend re-runs the identical failing `open()`, so the card offers the repair
/// instead. Only host-generated guidance contains this phrase.
pub(crate) fn needs_ownership_repair(text: &str) -> bool {
    text.contains("Fix Ownership")
}

/// The harness cannot serve the selected model: no credentials for its
/// provider, or a model this CLI does not list at all.
///
/// Resending repeats the same request with the same model, so the card offers
/// the picker instead. Matched against the CLI's own wording — Pi answers
/// `No API key found for the selected model`, `Model not found: …` and
/// `Unauthorized: Authentication required`.
pub(crate) fn needs_another_model(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "no api key found",
        "authentication required",
        "unauthorized",
        "invalid api key",
        "model not found",
        "unavailable or ambiguous",
        "pi auth login",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// The prompt reached the harness and the *turn* died afterwards: the server
/// stalled or dropped its event stream. Resending is still the recovery, but
/// "Send failed" would blame the wrong step — nothing was lost on the way out.
/// Matched against the harness's own wording for a silence / stream failure.
pub(crate) fn turn_died_after_send(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "no events",
        "event stream",
        "stopped responding",
        "turn is stuck",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

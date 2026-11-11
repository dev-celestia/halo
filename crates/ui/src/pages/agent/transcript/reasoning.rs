//! The reasoning/thinking transcript row.

use super::*;

/// One thought row: the same disclosure the work group uses — boxed chevron,
/// shimmering title while the thought streams, guide-lined body — so every
/// dropdown in the transcript opens the same way.
pub(crate) fn reasoning_row(
    part_id: SharedString,
    text: &str,
    streaming: bool,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let can_expand = !text.trim().is_empty();
    let open = expanded && can_expand;

    let title: AnyElement = if streaming {
        ShimmerText::wave(SharedString::from("Thinking"))
            .id(ElementId::Name(format!("{part_id}-thinking").into()))
            .with_shimmer_style(
                ShimmerStyle::wave()
                    .duration(Duration::from_millis(2400))
                    .highlight_color(foreground)
                    .spread(0.4),
            )
            .text_sm()
            .line_height(CARD_TITLE_LINE)
            .font_weight(FontWeight::NORMAL)
            .text_color(foreground)
            .into_any_element()
    } else {
        // Past tense once the thought settles: the run is over, so "Thinking"
        // would claim a process that is no longer happening. Matches the label
        // the folded work group gives the same part.
        div()
            .text_sm()
            .line_height(CARD_TITLE_LINE)
            .font_weight(FontWeight::NORMAL)
            .text_color(muted)
            .child("Thought")
            .into_any_element()
    };

    let header = disclosure_header(
        part_id.clone(),
        Some("agent-thought-row".into()),
        open,
        can_expand,
        title,
        on_toggle,
        cx,
    );

    let mut root = div().w_full().min_w_0().flex().flex_col().child(header);
    if open {
        let end = preview_end(text);
        let mut body = guide_body(cx)
            .debug_selector(|| "agent-thought-body".to_string())
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_base()
                    .line_height(LINE_HEIGHT)
                    .text_color(muted)
                    .whitespace_normal()
                    .child(text[..end].to_owned()),
            );
        if end < text.len() {
            body = body
                .child(
                    div()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(muted)
                        .child(
                            "Large reasoning preview limited. Copy includes the complete reasoning.",
                        ),
                )
                .child(copy_button(
                    SharedString::from(format!("{part_id}:copy-reasoning")),
                    text.to_owned(),
                    "Copy reasoning",
                    cx,
                ));
        }
        root = root.child(body);
    }
    root.into_any_element()
}

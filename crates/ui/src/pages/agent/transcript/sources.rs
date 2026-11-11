//! Tool-part rows: process rows, previews, and web-search source rows.

use super::*;

/// First non-blank line of a payload, trimmed to a scannable length. The row
/// clips it again at layout; this only keeps the string small.
pub(crate) fn preview_line(text: &str) -> String {
    let mut lines = text
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .map(str::trim);
    let line = lines.next().unwrap_or("");
    let mut end = line.len().min(160);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let mut preview = line[..end].to_owned();
    if end < line.len() || lines.next().is_some() {
        preview.push('…');
    }
    preview
}

/// One step row in the work group: status glyph, medium label, and a one-line
/// preview of the payload. Clicking opens the full payload in place, so the
/// clipped head never becomes the only copy of an output.
pub(crate) fn process_row(
    id: SharedString,
    state: TraceState,
    label: String,
    detail: Option<String>,
    sources: &[HaloAgentSource],
    mono: bool,
    peek: bool,
    expanded: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let can_expand = !sources.is_empty() || detail.as_ref().is_some_and(|d| !d.trim().is_empty());
    let open = expanded && can_expand;

    let status = match state {
        TraceState::Pending => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(12.))
            .child(
                Phosphor::new(PhosphorIcon::Circle)
                    .size(px(10.))
                    .color(muted),
            )
            .into_any_element(),
        TraceState::Running => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(12.))
            .child(Loader::grid_shift().size(px(12.)).color(theme.primary))
            .into_any_element(),
        TraceState::Done => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(12.))
            .child(
                Phosphor::new(PhosphorIcon::CheckCircle)
                    .size(px(12.))
                    .color(theme.success),
            )
            .into_any_element(),
        TraceState::Failed | TraceState::Error => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(12.))
            .child(
                Phosphor::new(PhosphorIcon::XCircle)
                    .size(px(12.))
                    .color(theme.danger),
            )
            .into_any_element(),
        TraceState::Denied => div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(12.))
            .child(
                Phosphor::new(PhosphorIcon::Prohibit)
                    .size(px(12.))
                    .color(theme.warning),
            )
            .into_any_element(),
    };
    let label_color = match state {
        TraceState::Denied => theme.warning,
        TraceState::Failed | TraceState::Error => theme.danger,
        TraceState::Pending => muted,
        TraceState::Running | TraceState::Done => theme.foreground,
    };

    // A peeking step keeps its label short so the payload preview can own the
    // middle; an inline chip lets the label (often a full command) take the row.
    let label_el = div()
        .text_xs()
        .line_height(META_LINE_HEIGHT)
        .font_weight(FontWeight::NORMAL)
        .text_color(label_color)
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .child(label);
    let label_el = if peek {
        label_el.flex_none().max_w(px(240.))
    } else {
        label_el.flex_1().min_w_0()
    };
    let separator = div()
        .flex_none()
        .text_xs()
        .line_height(META_LINE_HEIGHT)
        .text_color(muted)
        .child("·");
    let peek_el = preview_line_for(detail.as_deref(), sources).map(|preview| {
        div()
            .flex_1()
            .min_w_0()
            .text_xs()
            .line_height(META_LINE_HEIGHT)
            .text_color(muted)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .when(mono, |line| {
                line.font_family(theme.mono_font_family.clone())
            })
            .child(preview)
    });

    let header = div()
        .id(ElementId::Name(format!("{id}:row").into()))
        .w_full()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .py(px(2.))
        .rounded(theme.radius_xs())
        .when(can_expand, |row| {
            row.cursor_pointer()
                .hover(|s| s.bg(muted.opacity(0.5)))
                .capture_any_mouse_down(move |event, window, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    on_toggle(&ClickEvent::default(), window, cx);
                    cx.stop_propagation();
                })
        })
        .child(status)
        .child(label_el)
        .when(peek, |header| header.child(separator))
        .when(peek, |header| header.children(peek_el))
        .when(can_expand, |header| {
            header.child(
                Phosphor::new(if open {
                    PhosphorIcon::CaretDown
                } else {
                    PhosphorIcon::CaretRight
                })
                .size(px(10.))
                .color(muted),
            )
        });

    let mut root = div().w_full().min_w_0().flex().flex_col().child(header);
    if open {
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .pl(px(18.))
            .pb(px(4.));
        if !sources.is_empty() {
            body = body.child(source_rows(&id, sources, cx));
        }
        if let Some(detail) = detail {
            let end = preview_end(&detail);
            body = body.child(
                div()
                    .w_full()
                    .min_w_0()
                    .rounded(theme.radius_sm())
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .px(px(8.))
                    .py(px(6.))
                    .when(mono, |d| d.font_family(theme.mono_font_family.clone()))
                    .text_xs()
                    .line_height(px(18.))
                    .text_color(muted)
                    .whitespace_normal()
                    .child(detail[..end].to_owned()),
            );
            if end < detail.len() {
                body = body
                    .child(
                        div()
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .text_color(muted)
                            .child(
                                "Large step preview limited. Copy includes the complete payload.",
                            ),
                    )
                    .child(copy_button(
                        SharedString::from(format!("{id}:copy")),
                        detail,
                        "Copy step",
                        cx,
                    ));
            }
        }
        root = root.child(body);
    }
    root.into_any_element()
}

/// What the muted middle of a tool row says: a result count for a web search,
/// otherwise the head of the payload.
pub(crate) fn preview_line_for(
    detail: Option<&str>,
    sources: &[HaloAgentSource],
) -> Option<String> {
    if !sources.is_empty() {
        return Some(sources_summary(sources));
    }
    detail.map(preview_line)
}

pub(crate) fn sources_summary(sources: &[HaloAgentSource]) -> String {
    let count = if sources.len() == 1 {
        "1 source".to_string()
    } else {
        format!("{} sources", sources.len())
    };
    let mut hosts: Vec<String> = Vec::new();
    for source in sources {
        let host = source_host(&source.url);
        if !host.is_empty() && !hosts.contains(&host) {
            hosts.push(host);
        }
        if hosts.len() == 2 {
            break;
        }
    }
    if hosts.is_empty() {
        count
    } else {
        format!("{count} · {}", hosts.join(", "))
    }
}

pub(crate) fn source_host(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

/// Clickable links under a web-search row. A letter chip stands in for a
/// favicon the app does not fetch; the whole row opens the system browser.
pub(crate) fn source_rows(id: &SharedString, sources: &[HaloAgentSource], cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let mut list = div().w_full().min_w_0().flex().flex_col().gap(px(1.));
    for (index, source) in sources.iter().enumerate() {
        let key = format!("{id}:source:{index}");
        let probe = format!("agent-source-row-{index}");
        let host = source_host(&source.url);
        let initial = host
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default();
        let url = source.url.clone();
        list = list.child(
            div()
                .id(ElementId::Name(key.into()))
                .debug_selector(move || probe.clone())
                .w_full()
                .min_w_0()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .px(px(4.))
                .py(px(3.))
                .rounded(theme.radius_xs())
                .cursor_pointer()
                .hover(|style| style.bg(muted.opacity(0.4)))
                .on_click(move |_, _, cx| cx.open_url(&url))
                .child(
                    div()
                        .flex_none()
                        .size(px(16.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .bg(muted.opacity(0.25))
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(initial),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(theme.foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(source.title.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(muted)
                        .whitespace_nowrap()
                        .child(host),
                ),
        );
    }
    list.into_any_element()
}

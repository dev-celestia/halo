//! Work-group rendering: disclosure headers, guides, step lists.

use super::*;

pub(crate) fn format_worked_secs(secs: u64) -> String {
    if secs < 60 {
        format!("Worked for {secs}s")
    } else {
        let mins = secs / 60;
        let rem = secs % 60;
        if rem == 0 {
            format!("Worked for {mins}m")
        } else {
            format!("Worked for {mins}m {rem}s")
        }
    }
}

/// The disclosure header every transcript dropdown shares: a caret in a
/// bordered box, then the row title. The toggle runs in the capture phase of
/// mouse-down so the disclosure can't be lost between press and release the
/// way a passive click target can.
pub(crate) fn disclosure_header(
    id: impl Into<SharedString>,
    debug: Option<SharedString>,
    expanded: bool,
    can_expand: bool,
    title: impl IntoElement,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let border = theme.border;
    let id: SharedString = id.into();
    let mut header = div()
        .id(ElementId::Name(format!("{id}:disclosure").into()))
        .when_some(debug, |header, selector| {
            header.debug_selector(move || selector.to_string())
        })
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .py(px(8.))
        .child(
            div()
                .flex_none()
                .size(px(20.))
                .rounded(px(6.))
                .border_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Phosphor::new(if expanded {
                        PhosphorIcon::CaretDown
                    } else {
                        PhosphorIcon::CaretRight
                    })
                    .size(px(12.))
                    .color(muted),
                ),
        )
        .child(div().flex_1().min_w_0().child(title));
    if can_expand {
        header = header
            .cursor_pointer()
            .capture_any_mouse_down(move |event, window, cx| {
                if event.button != MouseButton::Left {
                    return;
                }
                on_toggle(&ClickEvent::default(), window, cx);
                cx.stop_propagation();
            });
    }
    header
}

/// Body under a disclosure header: the guide hangs from the chevron box's
/// centre, like a step rail.
pub(crate) fn guide_body(cx: &App) -> Div {
    let border = cx.theme().border;
    div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .ml(px(10.))
        .pl(px(14.))
        .pr(px(2.))
        .pb(px(10.))
        .border_l_1()
        .border_color(border)
}

/// The turn's process group (Qoder-style). A boxed chevron and a
/// "Running · Ns" title sit above the turn's step list — every foldable part
/// as one glyph + label + one-line preview row, hung off a left guide. While
/// the run is live the group opens itself and ends in the same spinner +
/// shimmer line the session loader uses; once settled the title carries the
/// final "Worked for" total and the group folds away.
pub(crate) fn render_work_group(
    timer: WorkTimer,
    rows: Vec<AnyElement>,
    steps_scroll: &ScrollHandle,
    force_scroll: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let border = theme.border;
    // A live run always has its activity line to reveal, so the chevron stays
    // honest even before the first step lands.
    let can_expand = timer.running || !rows.is_empty();
    let expanded = timer.expanded && can_expand;
    let live_label = if timer.running {
        timer.detail.clone()
    } else {
        None
    };
    let title = if timer.running {
        format!("Running · {}s", timer.secs)
    } else {
        format_worked_secs(timer.secs)
    };

    let header = disclosure_header(
        "coder-agent-work-timer-header",
        Some("coder-agent-work-timer-header".into()),
        expanded,
        can_expand,
        div()
            .text_sm()
            .line_height(CARD_TITLE_LINE)
            .font_weight(FontWeight::NORMAL)
            .text_color(if timer.running { foreground } else { muted })
            .child(title),
        on_toggle,
        cx,
    );

    let mut root = div()
        .id("coder-agent-work-timer")
        .w_full()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(border)
        .child(header);

    if expanded {
        // The chat already scrolls. Past this height the steps get a second
        // viewport, so a wheel over the thought process does not move the
        // transcript. A label line plus its padding is about 26px.
        const WORK_ROW_H: f32 = 26.;
        let content_h = rows.len() as f32 * WORK_ROW_H + 12.;
        let mut steps = guide_body(cx).w_full().min_w_0().children(rows);
        // An opened thought or tool body is taller than the row estimate, so
        // that case takes the fixed viewport too.
        if force_scroll || content_h > WORK_LIST_MAX_H {
            root = root.child(capped_steps(steps, steps_scroll));
        } else {
            steps = steps.max_h(px(WORK_LIST_MAX_H));
            root = root.child(steps);
        }
        if let Some(detail) = live_label {
            root = root.child(live_line(detail.label, foreground, cx));
        }
    }

    Message::new()
        .alignment(MessageAlignment::Start)
        .content(MessageContent::new().child(root))
}

/// Steps taller than [`WORK_LIST_MAX_H`] scroll here. The mask takes the wheel
/// in the capture phase, before the chat list's own mask can claim it, and
/// hands the event back once this list is at its edge.
pub(crate) fn capped_steps(steps: Div, scroll: &ScrollHandle) -> impl IntoElement {
    let scroll_for_track = scroll.clone();
    // The mask is the last child so its hitbox sits in front of the steps.
    // It only listens for the wheel, and a normal hitbox still lets clicks
    // through to the rows underneath.
    div()
        .id("coder-agent-work-steps")
        .relative()
        .w_full()
        .h(px(WORK_LIST_MAX_H))
        .max_h(px(WORK_LIST_MAX_H))
        .flex_none()
        .overflow_hidden()
        .child(
            div()
                .id("coder-agent-work-steps-area")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&scroll_for_track)
                .child(steps),
        )
        .child(
            div().absolute().inset_0().child(
                Scrollbar::new(scroll)
                    .id("coder-agent-work-steps-bar")
                    .axis(ScrollbarAxis::Vertical)
                    .viewport_from_layout(),
            ),
        )
        .child(ScrollableMask::new(Axis::Vertical, scroll).id("coder-agent-work-steps-mask"))
}

/// The run's current activity — same grid loader as the session rail, beside a
/// shimmering label.
pub(crate) fn live_line(label: String, foreground: Hsla, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .py(px(2.))
        .child(Loader::grid_shift().size(px(12.)).color(muted))
        .child(
            ShimmerText::wave(SharedString::from(label))
                .id(ElementId::Name("coder-agent-work-live".into()))
                .with_shimmer_style(
                    ShimmerStyle::wave()
                        .duration(Duration::from_millis(2400))
                        .highlight_color(foreground)
                        .spread(0.4),
                )
                .text_xs()
                .line_height(META_LINE_HEIGHT),
        )
        .into_any_element()
}

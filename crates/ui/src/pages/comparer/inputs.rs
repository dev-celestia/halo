//! Comparer columns — Original A / Modified B.
//!
//! Columns stay as CodeEditors while typing. After a short idle with both
//! sides filled, the same two columns flip to the highlighted diff. Click a
//! diff column to edit again. Paste/Clear always write through the editor.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, DiffInlinePart, DiffPartKind, DiffRow, RowKind};

use crate::pages::editors::ToolEditors;

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

pub(super) fn render_inputs(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    // Diff renders in these same two columns — no panel below.
    let show_diff = !state.comparer.show_inputs
        && !state.comparer.rows.is_empty()
        && !state.comparer.value_a.is_empty()
        && !state.comparer.value_b.is_empty();

    div()
        .w_full()
        .h_full()
        .flex()
        .gap_2()
        .bg(theme.muted.opacity(0.1))
        .p_2()
        .child(render_pane(
            "cmp-pane-a",
            "ORIGINAL TEXT (A)",
            Side::Left,
            &editors.cmp_a,
            &state.comparer.rows,
            show_diff,
            cx,
        ))
        .child(render_pane(
            "cmp-pane-b",
            "MODIFIED TEXT (B)",
            Side::Right,
            &editors.cmp_b,
            &state.comparer.rows,
            show_diff,
            cx,
        ))
}

fn render_pane(
    id: &'static str,
    label: &'static str,
    side: Side,
    editor: &Entity<celestia_ui::components::CodeEditor>,
    rows: &[DiffRow],
    show_diff: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();

    div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .h(px(28.))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .px_2p5()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.4))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.muted_foreground)
                        .child(label),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new(SharedString::from(format!("{id}-paste")))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Clipboard)
                                .label("Paste")
                                .on_click({
                                    let editor = editor.clone();
                                    move |_, window, cx| {
                                        if let Some(item) =
                                            cx.read_from_clipboard().and_then(|item| item.text())
                                        {
                                            editor.update(cx, |e, cx| e.set_text(item, window, cx));
                                        }
                                    }
                                }),
                        )
                        .child(
                            Button::new(SharedString::from(format!("{id}-clear")))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::Trash)
                                .label("Clear")
                                .on_click({
                                    let editor = editor.clone();
                                    move |_, window, cx| {
                                        editor.update(cx, |e, cx| e.set_text("", window, cx));
                                    }
                                }),
                        ),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(if show_diff {
                    render_side_diff(id, side, rows, cx).into_any_element()
                } else {
                    editor.clone().into_any_element()
                }),
        )
}

fn render_side_diff(id: &'static str, side: Side, rows: &[DiffRow], cx: &App) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("{id}-diff")))
        .w_full()
        .h_full()
        .overflow_y_scrollbar()
        .overflow_x_hidden()
        .font_family("Menlo")
        .text_xs()
        // Click the highlighted column to return to the editable editor.
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_, _, cx| Bridge::comparer_edit(cx))
        .child(
            div().w_full().flex().flex_col().children(
                rows.iter()
                    .enumerate()
                    .map(|(idx, row)| side_row(id, idx, side, row, cx))
                    .collect::<Vec<_>>(),
            ),
        )
}

fn side_row(id: &'static str, idx: usize, side: Side, row: &DiffRow, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (kind, num, content, inline) = match side {
        Side::Left => (
            row.left_kind,
            row.left_num,
            row.left_content.as_str(),
            row.inline_left.as_slice(),
        ),
        Side::Right => (
            row.right_kind,
            row.right_num,
            row.right_content.as_str(),
            row.inline_right.as_slice(),
        ),
    };

    let tint = if row.paired {
        hsla(45. / 360., 0.9, 0.5, 0.08)
    } else {
        match kind {
            RowKind::Removed => hsla(0., 0.75, 0.5, 0.07),
            RowKind::Added => hsla(145. / 360., 0.7, 0.45, 0.07),
            RowKind::Empty => theme.muted.opacity(0.15),
            RowKind::Unchanged => theme.background,
        }
    };

    let text_color = match kind {
        RowKind::Removed => hsla(0., 0.7, 0.42, 0.95),
        RowKind::Added => hsla(145. / 360., 0.7, 0.35, 0.95),
        RowKind::Empty => theme.muted_foreground.opacity(0.3),
        RowKind::Unchanged => theme.foreground.opacity(0.85),
    };

    div()
        .id(SharedString::from(format!(
            "{id}-row-{}-{}",
            idx,
            match side {
                Side::Left => "l",
                Side::Right => "r",
            }
        )))
        .w_full()
        .flex()
        .items_start()
        .border_b_1()
        .border_color(theme.border.opacity(0.4))
        .bg(tint)
        .child(
            div()
                .w(px(40.))
                .flex_none()
                .px_2()
                .py_0p5()
                .text_right()
                .text_xs()
                .text_color(theme.muted_foreground.opacity(0.6))
                .border_r_1()
                .border_color(theme.border)
                .child(num.map(|n| n.to_string()).unwrap_or_default()),
        )
        .child(content_cell(content, inline, text_color, cx))
        .into_any_element()
}

fn content_cell(
    content: &str,
    inline: &[DiffInlinePart],
    text_color: Hsla,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let body: AnyElement = if !inline.is_empty() {
        div()
            .flex()
            .flex_wrap()
            .children(inline.iter().map(|part| {
                let color = match part.kind {
                    DiffPartKind::Added => hsla(145. / 360., 0.7, 0.35, 0.9),
                    DiffPartKind::Removed => hsla(0., 0.7, 0.42, 0.9),
                    DiffPartKind::Unchanged => text_color,
                };
                let bg = match part.kind {
                    DiffPartKind::Added => hsla(145. / 360., 0.7, 0.5, 0.25),
                    DiffPartKind::Removed => hsla(0., 0.75, 0.55, 0.25),
                    DiffPartKind::Unchanged => gpui::transparent_black(),
                };
                div()
                    .px_0p5()
                    .rounded(px(2.))
                    .text_color(color)
                    .bg(bg)
                    .child(part.text.clone())
                    .into_any_element()
            }))
            .into_any_element()
    } else if content.is_empty() {
        div().child("\u{00A0}").into_any_element()
    } else {
        div().child(content.to_string()).into_any_element()
    };

    div()
        .flex_1()
        .min_w_0()
        .px_2()
        .py_0p5()
        .whitespace_normal()
        .when(content.is_empty(), |this| this.opacity(0.3))
        .text_color(text_color)
        .border_color(theme.border.opacity(0.0))
        .child(body)
        .into_any_element()
}

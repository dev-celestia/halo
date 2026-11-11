//! Table geometry and rendering (div-based so text shaping uses GPUI's normal pipeline).

use gpui::*;
use halo_bridge::DiagramTable;

pub const TITLE_H: f32 = 28.0;
pub const HEADER_H: f32 = 28.0;
pub const ROW_H: f32 = 26.0;

/// World-space size of a table.
pub fn table_size(t: &DiagramTable) -> (f32, f32) {
    let w: f32 = t.col_widths.iter().sum();
    (w, TITLE_H + HEADER_H + ROW_H * t.rows.len() as f32)
}

/// Render a table at screen position `(sx, sy)` scaled by `zoom`.
pub fn render_table(
    t: &DiagramTable,
    sx: f32,
    sy: f32,
    zoom: f32,
    selected: bool,
) -> impl IntoElement {
    let (w, h) = table_size(t);
    let border = rgb(t.border_color);
    let cell = |text: String, width: f32, height: f32, bg: u32, bold: bool| {
        div()
            .w(px(width * zoom))
            .h(px(height * zoom))
            .flex_none()
            .flex()
            .items_center()
            .px(px(6.0 * zoom))
            .overflow_hidden()
            .whitespace_nowrap()
            .bg(rgb(bg))
            .border_r_1()
            .border_b_1()
            .border_color(border)
            .text_size(px(12.0 * zoom))
            .text_color(rgb(0xe2e8f0))
            .when_bold(bold)
            .child(text)
    };

    let mut col = div()
        .absolute()
        .left(px(sx))
        .top(px(sy))
        .w(px(w * zoom))
        .h(px(h * zoom))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(4.0 * zoom))
        .border_1()
        .border_color(if selected { rgb(0x38bdf8) } else { border });

    col = col.child(
        div()
            .h(px(TITLE_H * zoom))
            .w_full()
            .flex()
            .items_center()
            .px(px(8.0 * zoom))
            .bg(rgb(t.header_bg))
            .text_size(px(13.0 * zoom))
            .font_weight(FontWeight::NORMAL)
            .text_color(rgb(0xf8fafc))
            .child(t.title.clone()),
    );
    col = col.child(
        div()
            .flex()
            .flex_row()
            .children(t.headers.iter().enumerate().map(|(i, hd)| {
                cell(
                    hd.clone(),
                    t.col_widths.get(i).copied().unwrap_or(100.0),
                    HEADER_H,
                    0x0b1220,
                    true,
                )
            })),
    );
    for (ri, row) in t.rows.iter().enumerate() {
        let bg = if ri % 2 == 0 {
            t.row_stripe_bg
        } else {
            0x111827
        };
        col = col.child(div().flex().flex_row().children(row.iter().enumerate().map(
            move |(i, c)| {
                cell(
                    c.clone(),
                    t.col_widths.get(i).copied().unwrap_or(100.0),
                    ROW_H,
                    bg,
                    false,
                )
            },
        )));
    }
    col
}

trait WhenBold {
    fn when_bold(self, bold: bool) -> Self;
}

impl WhenBold for Div {
    fn when_bold(self, bold: bool) -> Self {
        if bold {
            self.font_weight(FontWeight::NORMAL)
        } else {
            self
        }
    }
}

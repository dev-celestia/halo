//! VT grid state and the host-drawn cell renderer.
//!
//! `alacritty_terminal` parses the byte stream; this module owns the grid and
//! turns it into GPUI text runs. No widget chrome lives here — the surface adds
//! padding, focus, selection gestures, and links.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as CellPoint};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use gpui::*;

use crate::color::resolve_cell_colors;
use crate::link::find_links;

/// Horizontal padding (each side) for the grid inset.
pub const GRID_PAD_X: f32 = 8.;
/// Vertical padding (each side) for the grid inset.
pub const GRID_PAD_Y: f32 = 4.;

/// Selection tints — solid enough to read as a highlight, pale enough to leave
/// the ANSI colours underneath legible.
pub const SELECTION_BG_DARK: (u8, u8, u8) = (56, 78, 112);
pub const SELECTION_BG_LIGHT: (u8, u8, u8) = (176, 208, 248);

/// Approximate monospace cell metrics from font size (px).
/// Line height must stay ≥ font size so glyphs are never vertically clipped.
pub fn cell_metrics(font_size: f32) -> (f32, f32) {
    let cell_h = (font_size * 1.4).ceil().max(font_size + 2.);
    let cell_w = (font_size * 0.6).max(6.);
    (cell_w, cell_h)
}

#[derive(Clone, Copy, Debug)]
pub struct TermSize {
    columns: usize,
    screen_lines: usize,
}

impl TermSize {
    pub fn new(columns: usize, screen_lines: usize) -> Self {
        Self {
            columns: columns.max(1),
            screen_lines: screen_lines.max(1),
        }
    }
}

impl Dimensions for TermSize {
    /// Screen lines only, deliberately: `Term::new` passes the visible line count
    /// and `config.scrolling_history` to the grid separately, and `Term`'s own
    /// `history_size()` reads the grid's impl. Adding scrollback here would
    /// double-count it (this is only consulted by `Line::grid_clamp`).
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Parsed VT state for one PTY session.
pub struct SessionVt {
    pub term: Term<VoidListener>,
    parser: Processor,
    pub cols: u16,
    pub rows: u16,
}

impl SessionVt {
    pub fn new(cols: u16, rows: u16, scrollback: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let size = TermSize::new(cols as usize, rows as usize);
        let config = Config {
            scrolling_history: scrollback.max(100),
            ..Config::default()
        };
        Self {
            term: Term::new(config, &size, VoidListener),
            parser: Processor::new(),
            cols,
            rows,
        }
    }

    /// Throw away the grid (epoch change / truncation) and keep the same size.
    pub fn reset(&mut self, scrollback: usize) {
        let size = TermSize::new(self.cols as usize, self.rows as usize);
        let config = Config {
            scrolling_history: scrollback.max(100),
            ..Config::default()
        };
        self.term = Term::new(config, &size, VoidListener);
        self.parser = Processor::new();
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if self.cols == cols && self.rows == rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term
            .resize(TermSize::new(cols as usize, rows as usize));
    }

    pub fn feed(&mut self, data: &str) {
        if data.is_empty() {
            return;
        }
        self.parser.advance(&mut self.term, data.as_bytes());
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }
}

/// One URL from the last painted frame. Spans are kept in grid coordinates,
/// not viewport rows, so a click still resolves after the user scrolls back.
#[derive(Clone, Debug)]
pub struct RenderedLink {
    pub line: i32,
    pub start: usize,
    pub end: usize,
    pub url: String,
}

/// Visual attributes for one coalesced run of terminal cells.
#[derive(Clone, Copy, PartialEq)]
pub struct CellStyle {
    fg: Hsla,
    bg: Option<Hsla>,
    bold: bool,
    italic: bool,
    dim: bool,
    underline: bool,
    strike: bool,
}

/// Paint the visible grid.
///
/// `links` is replaced with this frame's spans, in grid coordinates.
#[allow(clippy::too_many_arguments)]
pub fn render_grid(
    term: &Term<VoidListener>,
    mono: &SharedString,
    font_size: f32,
    default_fg: Hsla,
    default_bg: Hsla,
    cursor_fg: Hsla,
    selection_bg: Hsla,
    dark: bool,
    links: &mut Vec<RenderedLink>,
) -> AnyElement {
    let (_, line_h) = cell_metrics(font_size);
    let grid = term.grid();
    let cols = grid.columns();
    let rows = grid.screen_lines();
    let display_offset = grid.display_offset() as i32;
    let cursor = grid.cursor.point;
    // DECTCEM: applications hide the cursor while they draw.
    let show_cursor = display_offset == 0 && term.mode().contains(TermMode::SHOW_CURSOR);
    // Resolved once: the per-cell test is a plain range check.
    let selected = term.selection.as_ref().and_then(|s| s.to_range(term));
    let mut found_links: Vec<RenderedLink> = Vec::new();

    let mut line_els: Vec<AnyElement> = Vec::with_capacity(rows);
    for row in 0..rows {
        let line_idx = Line(row as i32 - display_offset);
        let mut line_text = String::with_capacity(cols);
        let mut text_runs: Vec<TextRun> = Vec::new();
        let mut cur_style = CellStyle {
            fg: default_fg,
            bg: None,
            bold: false,
            italic: false,
            dim: false,
            underline: false,
            strike: false,
        };
        let mut run_start_bytes = 0usize;

        let flush_run = |runs: &mut Vec<TextRun>,
                         start: usize,
                         end: usize,
                         style: CellStyle,
                         font_size: f32| {
            if end <= start {
                return;
            }
            let mut fg = style.fg;
            if style.dim {
                fg = fg.opacity(0.55);
            }
            runs.push(TextRun {
                len: end - start,
                font: Font {
                    family: mono.clone(),
                    features: FontFeatures::default(),
                    fallbacks: None,
                    weight: if style.bold {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    },
                    style: if style.italic {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    },
                },
                color: fg,
                background_color: style.bg,
                underline: if style.underline {
                    Some(UnderlineStyle {
                        thickness: px((font_size * 0.08).max(1.)),
                        color: Some(fg),
                        wavy: false,
                    })
                } else {
                    None
                },
                strikethrough: if style.strike {
                    Some(StrikethroughStyle {
                        thickness: px((font_size * 0.08).max(1.)),
                        color: Some(fg),
                    })
                } else {
                    None
                },
            });
        };

        // Link pass: one char per column, so a span's indices are columns.
        // Wide-char spacers stand in for a space to keep that 1:1 mapping,
        // and URLs are ASCII throughout, so nothing is lost.
        let mut link_cols: Vec<bool> = Vec::new();
        let mut col_chars: Vec<char> = Vec::with_capacity(cols);
        for col in 0..cols {
            let cell = &grid[line_idx][Column(col)];
            let mut ch = cell.c;
            if ch == '\0'
                || cell.flags.contains(Flags::HIDDEN)
                || cell.flags.contains(Flags::WIDE_CHAR_SPACER)
            {
                ch = ' ';
            }
            col_chars.push(ch);
        }
        // Cheap gate: a row without `://` never has a link in it.
        if col_chars
            .windows(3)
            .any(|window| window[0] == ':' && window[1] == '/' && window[2] == '/')
        {
            link_cols = vec![false; cols];
            for span in find_links(&col_chars) {
                let end = span.end.min(cols);
                link_cols[span.start..end].fill(true);
                found_links.push(RenderedLink {
                    line: line_idx.0,
                    start: span.start,
                    end,
                    url: span.url,
                });
            }
        }

        for col in 0..cols {
            let cell = &grid[line_idx][Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let mut ch = cell.c;
            if ch == '\0' || cell.flags.contains(Flags::HIDDEN) {
                ch = ' ';
            }
            let bold = cell.flags.contains(Flags::BOLD);
            let (mut fg, mut bg_opt) =
                resolve_cell_colors(cell.fg, cell.bg, default_fg, default_bg, dark, bold);
            if cell.flags.contains(Flags::INVERSE) {
                let tmp = fg;
                fg = bg_opt.unwrap_or(default_bg);
                bg_opt = Some(tmp);
            }
            let is_cursor = show_cursor
                && cursor
                    == CellPoint {
                        line: Line(row as i32),
                        column: Column(col),
                    };
            if is_cursor {
                bg_opt = Some(cursor_fg);
                fg = default_bg;
            }
            // The highlight wins over the cursor block: the selection is what
            // the user is reading, and the caret is not needed while it shows.
            let is_selected = selected.as_ref().is_some_and(|range| {
                range.contains(CellPoint {
                    line: line_idx,
                    column: Column(col),
                })
            });
            let is_link = link_cols.get(col).copied().unwrap_or(false);

            let style = CellStyle {
                fg,
                bg: if is_selected {
                    Some(selection_bg)
                } else {
                    bg_opt
                },
                bold,
                italic: cell.flags.contains(Flags::ITALIC),
                dim: cell.flags.contains(Flags::DIM),
                // Links keep their own colour and gain an underline, which is
                // what makes them findable without repainting ANSI output.
                underline: is_link || cell.flags.intersects(Flags::ALL_UNDERLINES),
                strike: cell.flags.contains(Flags::STRIKEOUT),
            };

            // Close the previous run before appending this cell's text, so
            // the base char and its combining marks always land together.
            if line_text.is_empty() {
                cur_style = style;
                run_start_bytes = 0;
            } else if style != cur_style {
                flush_run(
                    &mut text_runs,
                    run_start_bytes,
                    line_text.len(),
                    cur_style,
                    font_size,
                );
                run_start_bytes = line_text.len();
                cur_style = style;
            }
            line_text.push(ch);
            // Combining marks live beside the base char, not in a cell of
            // their own — dropping them loses every NFD accent.
            if let Some(extra) = cell.zerowidth() {
                line_text.extend(extra.iter());
            }
        }
        flush_run(
            &mut text_runs,
            run_start_bytes,
            line_text.len(),
            cur_style,
            font_size,
        );

        if line_text.is_empty() {
            line_text.push(' ');
            text_runs.push(TextRun {
                len: 1,
                font: Font {
                    family: mono.clone(),
                    features: FontFeatures::default(),
                    fallbacks: None,
                    weight: FontWeight::NORMAL,
                    style: FontStyle::Normal,
                },
                color: default_fg,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }

        // Fixed row height + flex_none: never let the column compress lines
        // below the font metrics (that caused mid-glyph horizontal clipping).
        let styled = StyledText::new(line_text).with_runs(text_runs);
        let row_el = div()
            .flex_none()
            .w_full()
            .h(px(line_h))
            .overflow_hidden()
            .whitespace_nowrap()
            .line_height(px(line_h))
            .child(styled);
        line_els.push(row_el.into_any_element());
    }

    // Publish this frame's spans so a click can resolve to a URL. They are in
    // grid coordinates, so they stay correct while scrolled back.
    *links = found_links;

    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .overflow_hidden()
        .children(line_els)
        .into_any_element()
}

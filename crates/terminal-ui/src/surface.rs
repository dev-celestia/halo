//! One focusable terminal surface bound to one PTY session.
//!
//! Keystrokes go to the session's PTY; its output is parsed into a cell grid and
//! rendered in place. Pointer gestures:
//!
//! - Drag to select (double-click word, triple-click line; ⌥-drag = block).
//! - ⌘C copies the selection (Ctrl+Shift+C off macOS).
//! - Click (or ⌥/⌘-click) an `http(s)://` link to open it; a drag on the same
//!   span selects instead.

use std::sync::Arc;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::Point as CellPoint;
use alacritty_terminal::index::{Column, Line, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;

use crate::color::rgb_to_hsla;
use crate::grid::{
    GRID_PAD_X, GRID_PAD_Y, RenderedLink, SELECTION_BG_DARK, SELECTION_BG_LIGHT, SessionVt,
    cell_metrics, render_grid,
};
use crate::host::{TerminalHost, TerminalRead};
use crate::keys::encode_keystroke;

/// Grid used before the first layout pass measures the real size.
const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 24;

/// One terminal pane: one PTY session, one VT grid, one selection.
pub struct TerminalSurface {
    focus_handle: FocusHandle,
    host: Arc<dyn TerminalHost>,
    session: Option<String>,
    vt: Option<SessionVt>,
    /// Absolute byte offset of the next chunk this surface has not parsed.
    cursor: u64,
    /// Last session epoch applied to `vt`; a change forces a grid reset.
    epoch: u64,
    alive: bool,
    font_size: f32,
    scrollback: usize,
    last_bounds: Option<Size<Pixels>>,
    /// Full surface bounds in window coordinates — used by the paint-time
    /// window mouse listeners (hitbox hover is too flaky for drag-select).
    surface_bounds: Option<Bounds<Pixels>>,
    /// Top-left of the cell grid in window coordinates (surface origin + inset).
    grid_origin: Option<Point<Pixels>>,
    /// A drag selection is in flight.
    selecting: bool,
    /// Cell the current press began on — a release on the same cell can open a link.
    press_cell: Option<CellPoint>,
    /// URL under the press cell; opened on release if the pointer never moved.
    press_url: Option<String>,
    /// URL under the pointer (from the last painted link map).
    hover_url: Option<String>,
    /// Clickable spans from the last render.
    links: Vec<RenderedLink>,
    /// Palette from the window theme (light/dark).
    default_fg: Hsla,
    default_bg: Hsla,
    cursor_fg: Hsla,
    selection_bg: Hsla,
    dark: bool,
}

impl TerminalSurface {
    pub fn new(host: Arc<dyn TerminalHost>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            session: None,
            vt: None,
            cursor: 0,
            epoch: 0,
            alive: false,
            font_size: 13.,
            scrollback: 5_000,
            last_bounds: None,
            surface_bounds: None,
            grid_origin: None,
            selecting: false,
            press_cell: None,
            press_url: None,
            hover_url: None,
            links: Vec::new(),
            default_fg: rgb_to_hsla(232, 234, 237),
            default_bg: rgb_to_hsla(28, 30, 33),
            cursor_fg: rgb_to_hsla(88, 196, 120),
            selection_bg: rgb_to_hsla(
                SELECTION_BG_DARK.0,
                SELECTION_BG_DARK.1,
                SELECTION_BG_DARK.2,
            ),
            dark: true,
        }
    }

    /// Point this surface at a session. Passing the same id is a no-op.
    pub fn bind(&mut self, session: Option<String>) {
        if self.session == session {
            return;
        }
        self.session = session;
        self.cursor = 0;
        self.epoch = 0;
        self.alive = false;
        self.last_bounds = None;
        self.selecting = false;
        self.press_cell = None;
        self.press_url = None;
        self.hover_url = None;
        self.links.clear();
        self.vt = self
            .session
            .as_ref()
            .map(|_| SessionVt::new(DEFAULT_COLS, DEFAULT_ROWS, self.scrollback));
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub fn alive(&self) -> bool {
        self.alive
    }

    pub fn has_selection(&self) -> bool {
        self.vt
            .as_ref()
            .and_then(|vt| vt.term.selection.as_ref())
            .is_some_and(|selection| !selection.is_empty())
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle.focus(window, cx);
        if let Some(id) = self.session.as_deref() {
            self.host.focused(id, cx);
        }
    }

    pub fn set_palette(&mut self, fg: Hsla, bg: Hsla, cursor: Hsla, dark: bool) {
        self.default_fg = fg;
        self.default_bg = bg;
        self.cursor_fg = cursor;
        self.dark = dark;
        self.selection_bg = if dark {
            rgb_to_hsla(
                SELECTION_BG_DARK.0,
                SELECTION_BG_DARK.1,
                SELECTION_BG_DARK.2,
            )
        } else {
            rgb_to_hsla(
                SELECTION_BG_LIGHT.0,
                SELECTION_BG_LIGHT.1,
                SELECTION_BG_LIGHT.2,
            )
        };
    }

    pub fn set_settings(&mut self, font_size: u16, scrollback_lines: u32) {
        let font_size = font_size.max(8) as f32;
        // Cell metrics drive the row/column count, so a font change invalidates
        // the measurement cache — otherwise the grid keeps the old row count and
        // the new line height clips it until the pane is resized.
        if font_size != self.font_size {
            self.last_bounds = None;
        }
        self.font_size = font_size;
        self.scrollback = scrollback_lines.max(100) as usize;
    }

    /// Apply everything the session produced since this surface's last read.
    pub fn sync(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.session.clone() else {
            return;
        };
        let Some(read) = self.host.read(&id, self.cursor, cx) else {
            self.alive = false;
            return;
        };
        self.apply_read(read);
    }

    fn apply_read(&mut self, read: TerminalRead) {
        let vt = self
            .vt
            .get_or_insert_with(|| SessionVt::new(DEFAULT_COLS, DEFAULT_ROWS, self.scrollback));

        // Epoch change or a truncated tail means the grid no longer matches the
        // stream: rebuild it before feeding the retained bytes.
        if read.epoch != self.epoch || read.truncated {
            self.epoch = read.epoch;
            vt.reset(self.scrollback);
        }
        if !read.data.is_empty() {
            vt.feed(&read.data);
        }
        self.cursor = read.seq;
        self.alive = read.alive;
    }

    fn ensure_size(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let size = bounds.size;
        self.surface_bounds = Some(bounds);
        // Mouse hit-testing needs the painted origin too, and it is valid even
        // when the box is too small to size a grid from.
        self.grid_origin = Some(point(
            bounds.origin.x + px(GRID_PAD_X),
            bounds.origin.y + px(GRID_PAD_Y),
        ));
        let Some(id) = self.session.clone() else {
            return;
        };
        if self.last_bounds == Some(size) {
            return;
        }

        let (cell_w, cell_h) = cell_metrics(self.font_size);
        // Subtract grid padding so row count matches the painted area (avoids
        // flex-shrinking lines and clipping glyphs through the middle).
        let inner_w = (f32::from(size.width) - GRID_PAD_X * 2.).max(cell_w);
        let inner_h = (f32::from(size.height) - GRID_PAD_Y * 2.).max(cell_h);
        let cols = (inner_w / cell_w).floor() as u16;
        let rows = (inner_h / cell_h).floor() as u16;

        // A collapsed or not-yet-laid-out pane reports a tiny box. Resizing the
        // PTY to 1×1 would SIGWINCH a full-screen app into a single cell, so skip
        // it — and do not cache, so the next real layout measures again.
        if cols < 2 || rows < 2 {
            self.last_bounds = None;
            return;
        }

        self.last_bounds = Some(size);

        let scrollback = self.scrollback;
        let vt = self
            .vt
            .get_or_insert_with(|| SessionVt::new(cols, rows, scrollback));
        if vt.cols != cols || vt.rows != rows {
            vt.resize(cols, rows);
            self.host.resize(&id, rows, cols, cx);
            cx.notify();
        }
    }

    fn active_mode(&self) -> TermMode {
        self.vt
            .as_ref()
            .map(SessionVt::mode)
            .unwrap_or_else(TermMode::empty)
    }

    /// Writing input always returns the viewport to the live prompt — otherwise
    /// typing after scrolling back goes to a screen the user cannot see.
    fn write_bytes(&mut self, data: &[u8], cx: &mut Context<Self>) {
        let Some(id) = self.session.clone() else {
            return;
        };
        if let Some(vt) = self.vt.as_mut() {
            vt.term.scroll_display(Scroll::Bottom);
        }
        self.host.write(&id, data, cx);
    }

    /// Paste the clipboard, wrapped in bracketed-paste markers when the shell
    /// asked for them — without them a multi-line paste is executed line by line.
    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let mut bytes = Vec::with_capacity(text.len() + 12);
        if self.active_mode().contains(TermMode::BRACKETED_PASTE) {
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(text.as_bytes());
            bytes.extend_from_slice(b"\x1b[201~");
        } else {
            bytes.extend_from_slice(text.as_bytes());
        }
        self.write_bytes(&bytes, cx);
        cx.stop_propagation();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let mods = &event.keystroke.modifiers;
        // Copy the selection. On macOS that is ⌘C; off macOS the platform
        // modifier is Ctrl and Ctrl+C must keep meaning SIGINT, so there it is
        // Ctrl+Shift+C. Either way a chord with nothing selected falls through.
        let copy_chord = mods.platform
            && (cfg!(target_os = "macos") || mods.shift)
            && event.keystroke.key.eq_ignore_ascii_case("c");
        if copy_chord && self.copy_selection(cx) {
            cx.stop_propagation();
            return;
        }

        // Platform chords belong to the app (⌘S, ⌘W …); paste is the one the
        // terminal implements itself.
        if mods.platform {
            if event.keystroke.key.eq_ignore_ascii_case("v") {
                self.paste_clipboard(cx);
            }
            return;
        }

        let mode = self.active_mode();
        if let Some(bytes) = encode_keystroke(&event.keystroke, mode) {
            self.write_bytes(&bytes, cx);
            cx.stop_propagation();
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(id) = self.session.clone() else {
            return;
        };
        let Some(vt) = self.vt.as_mut() else {
            return;
        };

        let (_, cell_h) = cell_metrics(self.font_size);
        let delta = event.delta.pixel_delta(px(cell_h));
        let lines = (f32::from(delta.y) / cell_h).round() as i32;
        if lines == 0 {
            return;
        }

        let mode = *vt.term.mode();
        if mode.contains(TermMode::ALT_SCREEN) {
            // The alternate screen has no scrollback. When the application opted
            // into alternate scroll (`\e[?1007h`) the wheel is forwarded as
            // cursor keys, which is what makes `less` and `man` scroll.
            if !mode.contains(TermMode::ALTERNATE_SCROLL) {
                return;
            }
            let step: &[u8] = if lines > 0 { b"\x1b[A" } else { b"\x1b[B" };
            let count = lines.unsigned_abs().min(10) as usize;
            let mut bytes = Vec::with_capacity(step.len() * count);
            for _ in 0..count {
                bytes.extend_from_slice(step);
            }
            self.host.write(&id, &bytes, cx);
            cx.stop_propagation();
            return;
        }

        // Wheel up → positive display offset (show history).
        vt.term.scroll_display(Scroll::Delta(lines));
        cx.notify();
        cx.stop_propagation();
    }

    /// Whether a window position sits on this surface (padding included).
    fn contains_pointer(&self, pos: Point<Pixels>) -> bool {
        self.surface_bounds
            .is_some_and(|bounds| bounds.contains(&pos))
    }

    /// Cell under a window position, in grid coordinates (`Line` already folds
    /// in the scrollback offset) plus which half of the cell was hit.
    ///
    /// Positions outside the grid are clamped to the nearest edge so a drag that
    /// leaves the pane still extends the selection to the last visible cell.
    fn cell_of(&self, pos: Point<Pixels>) -> Option<(CellPoint, Side)> {
        let origin = self.grid_origin?;
        let vt = self.vt.as_ref()?;
        let grid = vt.term.grid();
        let cols = grid.columns();
        let rows = grid.screen_lines();
        if cols == 0 || rows == 0 {
            return None;
        }
        let (cell_w, cell_h) = cell_metrics(self.font_size);
        let dx = f32::from(pos.x) - f32::from(origin.x);
        let dy = f32::from(pos.y) - f32::from(origin.y);
        let col = (dx / cell_w).floor().clamp(0., (cols as f32) - 1.) as usize;
        let row = (dy / cell_h).floor().clamp(0., (rows as f32) - 1.) as usize;
        let local_x = dx - col as f32 * cell_w;
        let side = if local_x < cell_w / 2. {
            Side::Left
        } else {
            Side::Right
        };
        let line = Line(row as i32 - grid.display_offset() as i32);
        Some((
            CellPoint {
                line,
                column: Column(col),
            },
            side,
        ))
    }

    /// URL covering a grid cell, per the last painted frame.
    fn link_at(&self, point: CellPoint) -> Option<&str> {
        self.links
            .iter()
            .find(|link| {
                link.line == point.line.0
                    && point.column.0 >= link.start
                    && point.column.0 < link.end
            })
            .map(|link| link.url.as_str())
    }

    /// ⌥-click (macOS Option) or ⌘/Win/Super-click opens a link immediately.
    fn link_open_modifiers(mods: &Modifiers) -> bool {
        mods.alt || mods.platform
    }

    fn open_link(&mut self, url: &str, cx: &mut Context<Self>) {
        if let Some(vt) = self.vt.as_mut() {
            vt.term.selection = None;
        }
        self.selecting = false;
        self.press_cell = None;
        self.press_url = None;
        cx.open_url(url);
        cx.stop_propagation();
        cx.notify();
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left || !self.contains_pointer(event.position) {
            return;
        }
        self.focus_handle.focus(window, cx);
        let Some(id) = self.session.clone() else {
            return;
        };
        self.host.focused(&id, cx);
        let Some((point, side)) = self.cell_of(event.position) else {
            return;
        };
        let url = self.link_at(point).map(str::to_string);

        // ⌥/⌘-click opens immediately. Plain click may still open on release if
        // the pointer never left this cell (see mouse_up).
        if let Some(url) = url
            .as_ref()
            .filter(|_| Self::link_open_modifiers(&event.modifiers))
        {
            self.open_link(url, cx);
            return;
        }

        let mode = self.active_mode();
        if mode.intersects(TermMode::MOUSE_MODE) && !event.modifiers.shift {
            // The app asked for the pointer (vim, htop). Painting a selection
            // over its screen would be a lie; ⇧+drag opts back in, as in xterm.
            self.selecting = false;
            self.press_cell = None;
            self.press_url = None;
            if let Some(vt) = self.vt.as_mut() {
                vt.term.selection = None;
            }
            cx.notify();
            return;
        }

        // ⌥-drag is block select (Terminal.app). ⌥-click on a link is handled
        // above, so a plain ⌥-press on text still gets the rectangular mode.
        let ty = match event.click_count {
            0 | 1 => {
                if event.modifiers.alt {
                    SelectionType::Block
                } else {
                    SelectionType::Simple
                }
            }
            2 => SelectionType::Semantic,
            _ => SelectionType::Lines,
        };
        if let Some(vt) = self.vt.as_mut() {
            let mut selection = Selection::new(ty, point, side);
            // Word / line clicks expand immediately; without this a double-click
            // that never moves looks empty until the next drag sample.
            if matches!(ty, SelectionType::Semantic | SelectionType::Lines) {
                selection.include_all();
            }
            vt.term.selection = Some(selection);
        }
        self.selecting = true;
        self.press_cell = Some(point);
        // Only a single click may turn into a link trip: a double-click means to
        // select a word, and its second release would otherwise open a second tab.
        self.press_url = if event.click_count <= 1 { url } else { None };
        cx.stop_propagation();
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.session.is_none() {
            return;
        }

        if self.selecting {
            cx.stop_propagation();
            // Moving off the press cell cancels a pending link click — this is a
            // drag-select, even when it started on top of a URL.
            if let (Some(press), Some(_)) = (self.press_cell, self.press_url.as_ref())
                && self.cell_of(event.position).map(|(p, _)| p) != Some(press)
            {
                self.press_url = None;
            }
            let Some((point, side)) = self.cell_of(event.position) else {
                return;
            };
            if let Some(vt) = self.vt.as_mut()
                && let Some(selection) = vt.term.selection.as_mut()
            {
                selection.update(point, side);
            }
            cx.notify();
            return;
        }

        if !self.contains_pointer(event.position) {
            if self.hover_url.take().is_some() {
                cx.notify();
            }
            return;
        }

        let hover = self
            .cell_of(event.position)
            .and_then(|(point, _)| self.link_at(point).map(str::to_string));
        if hover != self.hover_url {
            self.hover_url = hover;
            cx.notify();
        }
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        if event.button != MouseButton::Left || !self.selecting {
            return;
        }
        self.selecting = false;
        let press = self.press_cell.take();
        let url = self.press_url.take();
        if self.session.is_none() {
            return;
        }
        let released = self.cell_of(event.position).map(|(point, _)| point);

        // Click (no drag) on a URL opens it — same cell on press and release.
        if let (Some(url), Some(press)) = (url, press)
            && released == Some(press)
        {
            self.open_link(&url, cx);
            return;
        }

        // A plain click leaves a zero-width selection; drop it so no stray cell
        // stays highlighted, and ⌘C cannot copy nothing.
        let empty = self
            .vt
            .as_ref()
            .and_then(|vt| vt.term.selection.as_ref())
            .is_some_and(Selection::is_empty);
        if empty && let Some(vt) = self.vt.as_mut() {
            vt.term.selection = None;
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Copy the live selection. Returns false when there is nothing selected, so
    /// the chord can still reach the rest of the app.
    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(vt) = self.vt.as_ref() else {
            return false;
        };
        if vt.term.selection.as_ref().is_none_or(Selection::is_empty) {
            return false;
        }
        let text = vt.term.selection_to_string().unwrap_or_default();
        if text.is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }
}

impl Focusable for TerminalSurface {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle.clone();
        let font_size = self.font_size;
        let mono = cx.theme().mono_font_family.clone();
        let default_fg = self.default_fg;
        let default_bg = self.default_bg;
        let cursor_fg = self.cursor_fg;
        let dark = self.dark;
        let selection_bg = self.selection_bg;
        let session = self.session.clone().unwrap_or_default();

        let lines_el = match self.vt.as_ref() {
            Some(vt) => render_grid(
                &vt.term,
                &mono,
                font_size,
                default_fg,
                default_bg,
                cursor_fg,
                selection_bg,
                dark,
                &mut self.links,
            ),
            None => div()
                .text_color(default_fg.opacity(0.55))
                .child("No terminal session.")
                .into_any_element(),
        };
        // Hand cursor over a link so it reads as clickable before the click.
        let link_hover = self.hover_url.is_some();

        let entity = cx.entity().clone();
        let paint_entity = entity.clone();
        let (_, line_h) = cell_metrics(font_size);

        let surface = div()
            .id(SharedString::from(format!("term-surface-{session}")))
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .relative()
            .track_focus(&focus.clone().tab_index(0).tab_stop(true))
            .key_context("TerminalSurface")
            .font_family(mono)
            .text_size(px(font_size))
            .line_height(px(line_h))
            .text_color(default_fg)
            .bg(default_bg)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.handle_key(event, cx);
            }))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                this.scroll_wheel(event, cx);
            }))
            .child(
                div()
                    .id(SharedString::from(format!("term-grid-{session}")))
                    .size_full()
                    .overflow_hidden()
                    .px(px(GRID_PAD_X))
                    .py(px(GRID_PAD_Y))
                    .child(lines_el),
            )
            .child(
                // Invisible canvas: measure the PTY grid, and register
                // window-level capture listeners so drag-select does not depend
                // on hitbox hover (which drops events after keyboard focus).
                // `top_0` + `left_0` are required. `absolute` + `size_full` alone
                // uses the static position, which sits *after* the full-height
                // grid sibling — so this layer is the right size but entirely
                // below the visible pane, and every drag misses it.
                canvas(
                    move |bounds, _window, cx| {
                        entity.update(cx, |this, cx| {
                            this.ensure_size(bounds, cx);
                        });
                        bounds
                    },
                    move |bounds, _meta, window, _cx| {
                        let down_entity = paint_entity.clone();
                        let move_entity = paint_entity.clone();
                        let up_entity = paint_entity.clone();
                        let hit = bounds;

                        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            if !hit.contains(&event.position) {
                                return;
                            }
                            down_entity.update(cx, |this, cx| {
                                this.mouse_down(event, window, cx);
                            });
                        });

                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            move_entity.update(cx, |this, cx| {
                                // Always route moves while dragging; otherwise
                                // only while the pointer is over the surface.
                                if this.selecting || hit.contains(&event.position) {
                                    this.mouse_move(event, cx);
                                }
                            });
                        });

                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            up_entity.update(cx, |this, cx| {
                                if this.selecting {
                                    this.mouse_up(event, cx);
                                }
                            });
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .opacity(0.),
            );

        if link_hover {
            surface.cursor_pointer()
        } else {
            surface.cursor_text()
        }
    }
}

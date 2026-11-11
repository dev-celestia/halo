//! Traffic table with a pinned column header and virtualized selectable rows.
//!
//! Rows are virtualized via [`celestia_ui::components::composite::virtual_list`]
//! so thousands of proxy captures stay cheap to paint.

use celestia_ui::components::composite::virtual_list::{
    VirtualListBuilder, VirtualListScrollHandle,
};
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::table::{
    Table, TableHead, TableHeader, TablePart, TableRow,
};
use celestia_ui::components::traits::Size;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::{Scrollbar, ScrollbarAxis};
use halo_bridge::{Bridge, HttpLogRow};

use super::history_row::{HISTORY_ROW_HEIGHT, render_history_row};
use crate::pages::chrome::empty_state;

/// Gutter kept clear of the overlay scrollbar so the last column stays
/// readable. The header and the body share it, so their columns stay aligned.
const SCROLLBAR_GUTTER: Pixels = px(12.);

/// Height of the pinned header band.
///
/// `TableHead` hardcodes `h = max(row_height, 40px)` (web `h-10`), which leaves
/// ~14px of dead space above and below a single 12px label — too much for a
/// dense traffic table. `TableHead` refines with the caller's style *after* it
/// paints that height, so a call-site `.h()` wins without touching celestia-ui.
const HEADER_HEIGHT: Pixels = px(28.);

/// A header cell: 12px muted label, pinned width, [`HEADER_HEIGHT`] band.
fn head(label: &'static str, width: Option<Pixels>, muted: Hsla) -> TableHead {
    let cell = TableHead::new()
        .h(HEADER_HEIGHT)
        // The height is fixed and the label is centred, so the component's
        // vertical padding only eats into the content box.
        .py(px(0.))
        .text_color(muted);
    match width {
        Some(w) => cell.width(w).child(label),
        None => cell.child(label),
    }
}

/// Virtualized traffic rows for the HTTP history table.
///
/// Owned by [`crate::pages::editors::ToolEditors`] so [`VirtualListBuilder`]
/// has a stable [`Render`] entity across frames; [`Self::sync`] refreshes the
/// snapshot whenever the page re-renders from bridge state.
pub struct HttpHistoryList {
    scroll_handle: VirtualListScrollHandle,
    rows: Vec<HttpLogRow>,
    selected_id: Option<String>,
}

impl HttpHistoryList {
    pub fn new(_cx: &mut App) -> Self {
        Self {
            scroll_handle: VirtualListScrollHandle::new(),
            rows: Vec::new(),
            selected_id: None,
        }
    }

    /// Replace the visible snapshot from the current bridge history.
    pub fn sync(&mut self, history: &[HttpLogRow], selected_id: Option<&str>) {
        self.rows = history.to_vec();
        self.selected_id = selected_id.map(str::to_owned);
    }

    fn render_item(&self, ix: usize, cx: &App) -> AnyElement {
        let Some(row) = self.rows.get(ix) else {
            return div().h(HISTORY_ROW_HEIGHT).into_any_element();
        };
        let selected = self.selected_id.as_deref() == Some(row.id.as_str());
        render_history_row(row, selected, ix, cx).into_any_element()
    }
}

impl Render for HttpHistoryList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.rows.len();

        div()
            .id("http-history-virtual")
            .size_full()
            .relative()
            .child(
                VirtualListBuilder::vertical(cx.entity(), "http-history-rows")
                    .uniform(count, HISTORY_ROW_HEIGHT)
                    .track_scroll(&self.scroll_handle)
                    .render_item(|this, ix, _window, cx| this.render_item(ix, cx))
                    .size_full(),
            )
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::new(&self.scroll_handle)
                        .id("http-history-scrollbar")
                        .axis(ScrollbarAxis::Vertical)
                        .viewport_from_layout(),
                ),
            )
    }
}

/// Flex child under the pinned header that hosts the virtualized row list.
///
/// GPUI has no `position: sticky`, so a pinned header has to sit outside the
/// scroll container: the table frame becomes a flex column with the header as
/// its first child and this region as its second.
#[derive(IntoElement)]
struct VirtualHistoryBody {
    ix: usize,
    list: Entity<HttpHistoryList>,
}

impl VirtualHistoryBody {
    fn new(list: Entity<HttpHistoryList>) -> Self {
        Self { ix: 0, list }
    }
}

impl TablePart for VirtualHistoryBody {
    fn with_ix(mut self, ix: usize) -> Self {
        self.ix = ix;
        self
    }

    fn with_size(self, _size: Size) -> Self {
        self
    }
}

impl RenderOnce for VirtualHistoryBody {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(("http-history-body", self.ix))
            .w_full()
            .flex_1()
            .min_h_0()
            .pr(SCROLLBAR_GUTTER)
            .overflow_hidden()
            .child(self.list)
    }
}

pub fn render_history_list(
    history: &[HttpLogRow],
    selected_id: Option<&str>,
    pending_count: u32,
    list: &Entity<HttpHistoryList>,
    cx: &mut App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let mono = theme.mono_font_family.clone();
    let radius = theme.radius;
    let transparent = theme.transparent;

    if history.is_empty() {
        list.update(cx, |list, _cx| list.sync(&[], None));
        return div()
            .w_full()
            .h_full()
            .min_h_0()
            .child(empty_state(
                "No requests yet",
                "Start the proxy in Settings, then browse through it to fill this list.",
                cx,
            ))
            .into_any_element();
    }

    list.update(cx, |list, _cx| list.sync(history, selected_id));

    let header = TableHeader::new()
        .flex_none()
        // Match the page frame's top corners so a header fill (border band,
        // future muted wash, …) cannot paint a square past the radius.
        .rounded_t(radius)
        .pr(SCROLLBAR_GUTTER)
        .child(
            TableRow::new()
                .child(head("Time", Some(px(80.)), muted))
                .child(head("Method", Some(px(64.)), muted))
                .child(head("Status", Some(px(52.)), muted))
                // Wider than the old 160 to pay for the client mark that now leads
                // the host text, so the hostname itself keeps its width.
                .child(head("Host", Some(px(176.)), muted))
                .child(head("Path", None, muted))
                .child(head("Type", Some(px(88.)), muted))
                .child(head("Size", Some(px(80.)), muted)),
        );

    let pending_banner = (pending_count > 0).then(|| {
        let label = if pending_count == 1 {
            "1 new request — Click to refresh".to_string()
        } else {
            format!("{pending_count} new requests — Click to refresh")
        };
        Button::new("http-history-pending")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Small)
            .label(label)
            .on_click(|_, _, cx| {
                let _ = Bridge::refresh_history(cx);
            })
    });

    div()
        .w_full()
        .h_full()
        .min_h_0()
        .id("http-history-list")
        .flex()
        .flex_col()
        .children(pending_banner.map(|banner| {
            div()
                .w_full()
                .flex_none()
                .flex()
                .justify_center()
                .px_2()
                .py_1()
                .child(banner)
        }))
        .child(
            Table::new()
                .id("http-history-table")
                .accessibility_label("HTTP history")
                // The page wraps this table and the dock in one rounded frame
                // (`super::render_http`), so the table's own card border and
                // fill are dropped here — a square `table` fill was bleeding
                // past the frame's corner radius.
                .border_0()
                .rounded(px(0.))
                .bg(transparent)
                .font_family(mono)
                // Flex column so the header can hold its own line and the
                // body takes the rest of the frame.
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(header)
                .child(VirtualHistoryBody::new(list.clone())),
        )
        .into_any_element()
}

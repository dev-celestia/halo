//! Single row in the HTTP traffic table.

use celestia_ui::components::primitive::table::{TableCell, TablePart, TableRow};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, HttpLogRow};

use crate::components::confirm_delete;
use crate::pages::chrome::radius_xs;

use super::styles::{
    browser_mark, build_curl_command, format_bytes, format_time, host_and_path, method_color,
    short_mime, status_color,
};

/// Size of the client mark drawn inline in the `Host` cell. A touch larger than
/// the 14px explorer icons: these are colour marks read at a glance.
const HOST_MARK_SIZE: f32 = 16.;

/// Uniform row pitch for the virtualized traffic list.
///
/// Matches [`celestia_ui::components::traits::Size::Medium`]'s table row
/// height so cell padding and the 16px host mark fit without clipping.
pub(super) const HISTORY_ROW_HEIGHT: Pixels = px(32.);

pub fn render_history_row(row: &HttpLogRow, selected: bool, ix: usize, cx: &App) -> TableRow {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let id = row.id.clone();
    let (host, path) = host_and_path(row);
    let status = row
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into());
    let status_col = status_color(row.status);
    let method_col = method_color(&row.method);
    let mime = short_mime(row.content_type.as_deref());
    let size = format_bytes(row.response_size);
    let time = format_time(&row.timestamp);
    // The client's mark rides inside the host cell, so the table has no
    // separate `Client` column any more.
    let mark = browser_mark(row).map(host_mark);

    let select_id = id.clone();
    let menu_id = id.clone();
    let menu_host = host.clone();
    let menu_target = format!("{} {}{}", row.method, host, path);

    let mono = theme.mono_font_family.clone();
    let row = TableRow::new()
        .id(SharedString::from(format!("hist-row-{id}")))
        .selected(selected)
        .h(HISTORY_ROW_HEIGHT)
        .flex_none()
        .overflow_hidden()
        .font_family(mono)
        // Selecting is all it takes to open the dock: the page derives the
        // detail panel's presence from `selected_detail`.
        .on_click(move |_, _, cx| {
            let _ = Bridge::select_history_row(Some(select_id.clone()), cx);
        })
        .context_menu(move |menu, window, cx| {
            // Freeze the live stream while the menu is open so new rows do not
            // shove the target away under the cursor.
            Bridge::set_history_context_menu_open(true, cx);
            let entity = cx.entity();
            window
                .subscribe(&entity, cx, move |_, _: &DismissEvent, _, cx| {
                    Bridge::set_history_context_menu_open(false, cx);
                })
                .detach();

            // Doc §3.3 — Row Context Menu Actions
            let id = menu_id.clone();
            let host = menu_host.clone();
            menu.item({
                let id = id.clone();
                PopupMenuItem::new("Send to Repeater").on_click(move |_, _, cx| {
                    let _ = Bridge::select_history_row(Some(id.clone()), cx);
                    let _ = Bridge::send_selected_to_repeater(cx);
                })
            })
            .item({
                let id = id.clone();
                PopupMenuItem::new("Send to Intruder").on_click(move |_, _, cx| {
                    let _ = Bridge::select_history_row(Some(id.clone()), cx);
                    let _ = Bridge::send_selected_to_intruder(cx);
                })
            })
            .item({
                let id = id.clone();
                PopupMenuItem::new("Send to Intercept").on_click(move |_, _, cx| {
                    let _ = Bridge::select_history_row(Some(id.clone()), cx);
                    let _ = Bridge::send_selected_to_intercept(cx);
                })
            })
            .item({
                let host = host.clone();
                PopupMenuItem::new("Add to Scope").on_click(move |_, _, cx| {
                    let _ = Bridge::add_history_host_to_scope(&host, cx);
                })
            })
            .separator()
            .item({
                let id = id.clone();
                PopupMenuItem::new("Copy as cURL (bash)").on_click(move |_, _, cx| {
                    match Bridge::history_detail(&id, cx) {
                        Ok(detail) => {
                            let curl = build_curl_command(&detail);
                            cx.write_to_clipboard(ClipboardItem::new_string(curl));
                        }
                        Err(_) => {}
                    }
                })
            })
            .separator()
            .item({
                let target = menu_target.clone();
                PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                    confirm_delete(
                        window,
                        cx,
                        "Delete request?",
                        format!("{target} will be removed from history. This cannot be undone."),
                        {
                            let id = id.clone();
                            move |_, cx| {
                                let _ = Bridge::delete_history_row(&id, cx);
                            }
                        },
                    );
                })
            })
        })
        .child(
            // Time leads the row: it is the one column you scan down, and the
            // row number that used to sit here carried no information.
            TableCell::new()
                .width(px(80.))
                .child(div().text_color(muted).child(time)),
        )
        .child(
            TableCell::new().width(px(64.)).child(
                div()
                    .px_1()
                    .py(px(1.))
                    .rounded(radius_xs(theme))
                    .bg(method_col.opacity(0.18))
                    .font_weight(FontWeight::NORMAL)
                    .text_xs()
                    .text_color(method_col)
                    .child(row.method.clone()),
            ),
        )
        .child(
            TableCell::new().width(px(52.)).child(
                div()
                    .px_1()
                    .py(px(1.))
                    .rounded(radius_xs(theme))
                    .bg(status_col.opacity(0.18))
                    .font_weight(FontWeight::NORMAL)
                    .text_xs()
                    .text_color(status_col)
                    .child(status),
            ),
        )
        .child(
            // `[mark] host` — the client's logo leads the host, so one column
            // answers both "where" and "who sent it".
            TableCell::new().width(px(176.)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .w_full()
                    .min_w_0()
                    .children(mark)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(host),
                    ),
            ),
        )
        .child(
            TableCell::new().child(
                div()
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(path),
            ),
        )
        .child(
            TableCell::new().width(px(88.)).child(
                div()
                    .text_color(muted)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(mime),
            ),
        )
        .child(
            TableCell::new()
                .width(px(80.))
                .child(div().text_color(muted).child(size)),
        );

    // Real list index so top borders and a11y row numbers stay correct when
    // only a viewport slice is mounted.
    TablePart::with_ix(row, ix)
}

/// Client mark at the device-pixel size of the host cell, so a 128px logo is
/// not minified by the sprite atlas.
fn host_mark(path: &'static str) -> AnyElement {
    if let Some(image) = crate::raster::cached_png(path, HOST_MARK_SIZE) {
        img(image)
            .size(px(HOST_MARK_SIZE))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain)
            .into_any_element()
    } else {
        img(SharedString::from(path))
            .size(px(HOST_MARK_SIZE))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain)
            .into_any_element()
    }
}

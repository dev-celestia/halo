//! Wordlists — bundled catalog plus the remote GitHub manifest.
//!
//! Tag sidebar, searchable table, per-item download, bundle download, and a
//! 100-line preview pane. Files land in `Documents/Halo Files/Wordlists`.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, WordlistItem, WordlistStatus, WordlistsView};

use crate::pages::chrome::{PAGE_GAP, empty_state, format_bytes, panel};
use crate::pages::editors::ToolEditors;

pub fn render_wordlists(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let wl = &state.wordlists;
    let (installed, total) = wl.totals();

    div()
        .id("wordlists-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .gap(px(PAGE_GAP))
        .overflow_hidden()
        .child(render_tag_sidebar(wl, installed, total, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(px(8.))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .child(
                    div()
                        .w_full()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().muted.opacity(0.4))
                        .child(
                            div()
                                .flex_1()
                                .max_w(px(280.))
                                .child(editors.wl_search.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_family("Menlo")
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{} of {} wordlists",
                                    wl.catalog().len(),
                                    wl.items.len()
                                )),
                        )
                        .child(
                            Button::new("wordlists-refresh")
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::ArrowClockwise)
                                .label("Refresh")
                                .on_click(|_, _, cx| {
                                    let _ = Bridge::wordlists_fetch_manifest(true, cx);
                                }),
                        ),
                )
                .child(render_wordlist_table(state, cx)),
        )
        .children(
            wl.selected
                .clone()
                .map(|href| render_wordlist_preview(state, &href, cx).into_any_element()),
        )
}

fn render_tag_sidebar(wl: &WordlistsView, installed: usize, total: usize, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w(px(210.))
        .flex_none()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .px_2p5()
                .py_1p5()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.4))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child("Categories"),
                )
                .child(
                    div()
                        .px_1p5()
                        .rounded(theme.radius_sm())
                        .border_1()
                        .border_color(theme.border)
                        .text_xs()
                        .font_family("Menlo")
                        .child(format!("{installed}/{total}")),
                ),
        )
        .child(
            div()
                .id("wordlists-tags")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .flex()
                .flex_col()
                .gap_0p5()
                .p_1p5()
                .child(tag_row("all", "All Wordlists", total, wl, cx))
                .children(
                    wl.tags
                        .iter()
                        .map(|tag| tag_row(&tag.tag, &tag.tag, tag.count, wl, cx)),
                ),
        )
        .child(
            div().p_2().border_t_1().border_color(theme.border).child(
                div().w_full().child(
                    Button::new("wordlists-download-bundle")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::DownloadSimple)
                        .label(if wl.selected_tag == "all" {
                            "Download All Missing".into()
                        } else {
                            format!("Download \"{}\"", wl.selected_tag)
                        })
                        .on_click(|_, _, cx| {
                            let _ = Bridge::wordlists_download_bundle(cx);
                        }),
                ),
            ),
        )
        .into_any_element()
}

fn tag_row(tag: &str, label: &str, count: usize, wl: &WordlistsView, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let selected = wl.selected_tag == tag;
    div()
        .id(SharedString::from(format!("wordlists-tag-{tag}")))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .bg(if selected {
            theme.primary.opacity(0.1)
        } else {
            theme.background
        })
        .hover(|s| s.bg(theme.muted.opacity(0.4)))
        .on_click({
            let tag = tag.to_string();
            move |_, _, cx| Bridge::wordlists_set_tag(&tag, cx)
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_xs()
                .font_family("Menlo")
                .text_color(theme.muted_foreground)
                .child(count.to_string()),
        )
        .into_any_element()
}

fn render_wordlist_table(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let items = state.wordlists.catalog();
    let selected = state.wordlists.selected.clone();

    div()
        .id("wordlists-table")
        .w_full()
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .flex()
        .flex_col()
        .children(if items.is_empty() {
            vec![
                empty_state(
                    "No Wordlists Found",
                    "Refresh the catalog or download wordlists.",
                    cx,
                )
                .into_any_element(),
            ]
        } else {
            vec![
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.muted_foreground)
                    .bg(theme.muted.opacity(0.5))
                    .child(div().flex_1().child("WORDLIST"))
                    .child(div().w(px(80.)).child("TAGS"))
                    .child(div().w(px(70.)).child("LINES"))
                    .child(div().w(px(90.)).child("STATUS"))
                    .into_any_element(),
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .children(
                        items
                            .iter()
                            .map(|item| wordlist_row(item, selected.as_deref(), cx))
                            .collect::<Vec<_>>(),
                    )
                    .into_any_element(),
            ]
        })
        .into_any_element()
}

fn wordlist_status_pill(status: WordlistStatus, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (color, label, icon) = match status {
        WordlistStatus::Bundled => (theme.primary, "Bundled", PhosphorIcon::Package),
        WordlistStatus::Ready => (
            hsla(150. / 360., 0.65, 0.4, 1.),
            "Ready",
            PhosphorIcon::CheckCircle,
        ),
        WordlistStatus::Downloading => (theme.primary, "Downloading…", PhosphorIcon::CircleNotch),
        WordlistStatus::Error => (theme.danger, "Error", PhosphorIcon::Warning),
        WordlistStatus::Cloud => (
            theme.muted_foreground,
            "Cloud",
            PhosphorIcon::CloudArrowDown,
        ),
    };
    div()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(color.opacity(0.3))
        .bg(color.opacity(0.1))
        .text_color(color)
        .text_xs()
        .child(icon)
        .child(label)
        .into_any_element()
}

fn wordlist_row(item: &WordlistItem, selected: Option<&str>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(item.href.as_str());
    let href = item.href.clone();
    let href_for_download = item.href.clone();

    div()
        .id(SharedString::from(format!("wordlist-row-{}", item.href)))
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .font_family("Menlo")
        .text_xs()
        .border_b_1()
        .border_color(theme.border.opacity(0.4))
        .bg(if is_selected {
            theme.primary.opacity(0.1)
        } else {
            theme.background
        })
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted.opacity(0.3)))
        .on_click({
            let href = href.clone();
            move |_, _, cx| Bridge::wordlists_select(Some(href.clone()), cx)
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_color(theme.foreground)
                        .child(item.name.clone()),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(item.href.clone()),
                ),
        )
        .child(
            div()
                .w(px(80.))
                .flex()
                .flex_wrap()
                .gap_0p5()
                .children(item.tags.iter().map(|tag| {
                    div()
                        .px_1()
                        .rounded(theme.radius_sm())
                        .bg(theme.muted.opacity(0.5))
                        .text_xs()
                        .child(tag.clone())
                        .into_any_element()
                })),
        )
        .child(
            div().w(px(70.)).text_color(theme.muted_foreground).child(
                item.lines
                    .map(|lines| lines.to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
        )
        .child(
            div()
                .w(px(90.))
                .child(wordlist_status_pill(item.status, cx)),
        )
        .child(
            div()
                .w(px(90.))
                .flex()
                .items_center()
                .justify_end()
                .gap_1()
                .children(match item.status {
                    WordlistStatus::Cloud => {
                        vec![
                            Button::new(SharedString::from(format!("wordlist-dl-{}", item.href)))
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::CloudArrowDown)
                                .label("Download")
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::wordlists_download(&href_for_download, cx);
                                })
                                .into_any_element(),
                        ]
                    }
                    WordlistStatus::Ready => {
                        vec![
                            Button::new(SharedString::from(format!("wordlist-open-{}", item.href)))
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::FolderOpen)
                                .label("Reveal")
                                .on_click({
                                    let href = href.clone();
                                    move |_, _, cx| Bridge::wordlists_reveal(&href, cx)
                                })
                                .into_any_element(),
                        ]
                    }
                    _ => vec![],
                }),
        )
        .into_any_element()
}

fn render_wordlist_preview(state: &BridgeState, href: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let item = state
        .wordlists
        .items
        .iter()
        .find(|item| item.href == href)
        .cloned();
    let Some(item) = item else {
        return div().into_any_element();
    };
    let preview = state.wordlists.preview.clone();

    panel(
        "Wordlist Preview",
        None,
        div()
            .id("wordlists-preview")
            .w(px(320.))
            .h_full()
            .flex_none()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .child(item.name.clone()),
                    )
                    .child(wordlist_status_pill(item.status, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{} lines",
                        item.lines
                            .map(|l| l.to_string())
                            .unwrap_or_else(|| "—".into())
                    ))
                    .child(
                        item.file_size
                            .map(format_bytes)
                            .unwrap_or_else(|| "Built-in".into()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(item.tags.iter().map(|tag| {
                        div()
                            .px_1p5()
                            .rounded(theme.radius_sm())
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .child(tag.clone())
                            .into_any_element()
                    })),
            )
            .child(
                div()
                    .p_2p5()
                    .rounded(theme.radius_sm())
                    .border_1()
                    .border_color(theme.border.opacity(0.6))
                    .bg(theme.muted.opacity(0.2))
                    .text_xs()
                    .font_family("Menlo")
                    .whitespace_normal()
                    .min_h(px(120.))
                    .child(preview.unwrap_or_else(|| "Click a wordlist to load preview.".into())),
            ),
        cx,
    )
    .into_any_element()
}

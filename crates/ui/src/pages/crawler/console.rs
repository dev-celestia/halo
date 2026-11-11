//! Crawl activity console — full-bleed log surface under the view chrome.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_destructive;
use crate::pages::chrome::{SECTION_GAP, empty_state, radius_xs};
use crate::pages::editors::ToolEditors;

pub fn render_crawl_console(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let filter = state.crawler.log_filter.to_ascii_lowercase();
    let logs: Vec<_> = state
        .crawler
        .logs
        .iter()
        .filter(|log| {
            if filter.is_empty() {
                return true;
            }
            log.message.to_ascii_lowercase().contains(&filter)
                || log
                    .url
                    .as_ref()
                    .map(|u| u.to_ascii_lowercase().contains(&filter))
                    .unwrap_or(false)
                || log.log_type.to_ascii_lowercase().contains(&filter)
                || log.level.to_ascii_lowercase().contains(&filter)
        })
        .cloned()
        .collect();
    let log_count = logs.len();
    let total_logs = state.crawler.logs.len();
    let error_count = state
        .crawler
        .logs
        .iter()
        .filter(|l| l.level == "error" || l.log_type == "error")
        .count();

    let target = state
        .crawler
        .session
        .as_ref()
        .map(|s| s.target_url.as_str())
        .unwrap_or(state.crawler.config.target_url.as_str());

    div()
        .id("crawler-console")
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .overflow_hidden()
        .child(
            // Console toolbar — controls float inside the surface margins.
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(SECTION_GAP))
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_none()
                        .child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child(PhosphorIcon::Code),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.foreground)
                                .child(if filter.is_empty() {
                                    format!("{log_count} events")
                                } else {
                                    format!("{log_count} of {total_logs}")
                                }),
                        )
                        .when(error_count > 0, |el| {
                            el.child(
                                div()
                                    .px_1p5()
                                    .rounded(radius_xs(theme))
                                    .bg(theme.danger.opacity(0.18))
                                    .text_xs()
                                    .text_color(theme.danger)
                                    .child(format!("{error_count} err")),
                            )
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .child(Input::new(&editors.crawler_log_filter)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_none()
                        .child(
                            Button::new("crawler-apply-log-filter")
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::MagnifyingGlass)
                                .label("Filter")
                                .on_click({
                                    let filter_ed = editors.crawler_log_filter.clone();
                                    move |_, _, cx| {
                                        let value = filter_ed.read(cx).value().to_string();
                                        Bridge::set_crawler_log_filter(value, cx);
                                    }
                                }),
                        )
                        .child(
                            Button::icon("crawler-clear-console", PhosphorIcon::Trash)
                                .variant(ButtonVariant::Destructive)
                                .size(ButtonSize::Small)
                                .accessibility_label("Clear console")
                                .tooltip("Clear console")
                                .disabled(total_logs == 0)
                                .on_click(move |_, window, cx| {
                                    confirm_destructive(
                                        window,
                                        cx,
                                        "Clear console?",
                                        format!(
                                            "{total_logs} console event(s) will be removed. This cannot be undone."
                                        ),
                                        "Clear",
                                        |_, cx| {
                                            let _ = Bridge::clear_crawler_console(cx);
                                        },
                                    );
                                }),
                        ),
                ),
        )
        .child(
            // Quiet target strip — identifying context, not a second status bar.
            div()
                .w_full()
                .px_3()
                .py_1p5()
                .bg(theme.muted.opacity(0.14))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .font_family("Menlo")
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("$ crawl {target}")),
                ),
        )
        .child(
            div()
                .id("crawler-console-scroll")
                .w_full()
                .flex_1()
                .min_h(px(200.))
                .overflow_y_scrollbar()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_1p5()
                .when(logs.is_empty(), |el| {
                    el.child(empty_state(
                        if filter.is_empty() {
                            "No crawl events yet"
                        } else {
                            "No matching events"
                        },
                        if filter.is_empty() {
                            "Start a crawl to stream navigation, extraction, and policy events here."
                        } else {
                            "Try a different filter, or clear it to show the full activity log."
                        },
                        cx,
                    ))
                })
                .children(logs.into_iter().map(|log| render_log_row(log, cx))),
        )
}

fn render_log_row(log: halo_bridge::CrawlerLogDto, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let time = log
        .created_at
        .get(11..19)
        .unwrap_or(&log.created_at)
        .to_string();
    let (pill_bg, pill_fg) = log_type_colors(&log.log_type, &log.level, cx);

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_0p5()
        .font_family("Menlo")
        .text_xs()
        .child(
            div()
                .flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .w(px(64.))
                        .text_color(theme.muted_foreground)
                        .child(format!("[{time}]")),
                )
                .child(
                    div()
                        .flex_none()
                        .px_1p5()
                        .rounded(radius_xs(theme))
                        .bg(pill_bg)
                        .text_color(pill_fg)
                        .child(log.log_type.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(theme.foreground)
                        .child(log.message.clone()),
                ),
        )
        .when_some(log.url.clone(), |el, url| {
            el.child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_none().w(px(64.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(theme.muted_foreground)
                            .child(format!("↳ {url}")),
                    ),
            )
        })
}

fn log_type_colors(log_type: &str, level: &str, cx: &App) -> (Hsla, Hsla) {
    let theme = cx.theme();
    if level == "error" || log_type == "error" {
        return (theme.danger.opacity(0.22), theme.danger);
    }
    match log_type {
        "navigation" => (theme.primary.opacity(0.18), theme.primary),
        "extraction" => (theme.success.opacity(0.18), theme.success),
        "ai" => (theme.accent.opacity(0.35), theme.foreground),
        "human" => (theme.warning.opacity(0.18), theme.warning),
        "policy" | "skipped" | "queue" => (theme.muted.opacity(0.55), theme.muted_foreground),
        _ => (theme.muted.opacity(0.45), theme.muted_foreground),
    }
}

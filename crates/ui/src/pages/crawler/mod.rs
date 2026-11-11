//! Browser automation page — crawl console + recon findings.

mod config;
mod console;
mod insights;
mod toolbar;

use celestia_ui::components::composite::banner::{Banner, BannerTone};
use celestia_ui::components::composite::tab_bar::TabBarItem;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, CrawlerViewTab};

use super::editors::ToolEditors;
use crate::pages::chrome::{PAGE_GAP, StatusTone, content_tabs, status_pill};
use config::render_crawler_config;
use console::render_crawl_console;
use insights::render_insights_panel;
use toolbar::render_crawler_toolbar;

pub fn render_crawler(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let proxy_ok = state.proxy.running;

    div()
        .id("crawler-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(render_alerts(proxy_ok))
        .child(render_crawler_toolbar(state, editors, cx))
        .when(state.crawler.config_open, |el| {
            el.child(render_crawler_config(state, editors, cx))
        })
        .child(render_view_chrome(state.crawler.view_tab, state, cx))
        .child(div().w_full().flex_1().min_h_0().overflow_hidden().child(
            match state.crawler.view_tab {
                CrawlerViewTab::Activity => {
                    render_crawl_console(state, editors, cx).into_any_element()
                }
                CrawlerViewTab::Findings => render_insights_panel(state, cx).into_any_element(),
            },
        ))
}

fn render_alerts(proxy_ok: bool) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .when(!proxy_ok, |el| {
            el.child(
                Banner::new("crawler-proxy-offline", "Proxy is offline")
                    .tone(BannerTone::Warning)
                    .body("Start it before crawling so traffic is captured in HTTP History.")
                    .action("Start Proxy", |_, _, cx| {
                        let http = Bridge::state(cx).read(cx).preferred_http_port;
                        let tls = Bridge::state(cx).read(cx).preferred_tls_port;
                        let _ = Bridge::start_proxy(http, tls, cx);
                    }),
            )
        })
}

/// Tabs + live crawl stats in one chrome strip — single place for status.
fn render_view_chrome(active: CrawlerViewTab, state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let status = state.crawler.status();
    let tone = match status {
        "running" => StatusTone::Active,
        "paused" | "failed" => StatusTone::Busy,
        _ => StatusTone::Idle,
    };

    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(PAGE_GAP))
        .child(
            div().flex_1().min_w_0().child(
                content_tabs("crawler-view-tabs")
                    .item(TabBarItem::new("activity", "Activity").closable(false))
                    .item(TabBarItem::new("findings", "Findings").closable(false))
                    .active(match active {
                        CrawlerViewTab::Activity => "activity",
                        CrawlerViewTab::Findings => "findings",
                    })
                    .on_select(|id, _, cx| {
                        let tab = match id.as_ref() {
                            "findings" => CrawlerViewTab::Findings,
                            _ => CrawlerViewTab::Activity,
                        };
                        Bridge::set_crawler_view_tab(tab, cx);
                    }),
            ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .flex_none()
                .child(status_pill(format!("Crawl · {status}"), tone, cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .font_family("Menlo")
                        .text_color(theme.muted_foreground)
                        .child(format!("{} visited", state.crawler.pages_visited()))
                        .child(
                            div()
                                .size(px(3.))
                                .rounded_full()
                                .bg(theme.muted_foreground.opacity(0.45)),
                        )
                        .child(format!("{} findings", state.crawler.insights.len()))
                        .child(
                            div()
                                .size(px(3.))
                                .rounded_full()
                                .bg(theme.muted_foreground.opacity(0.45)),
                        )
                        .child(format!("{} logs", state.crawler.logs.len())),
                ),
        )
}

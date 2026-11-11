//! Command bar — target URL, crawl controls, config disclosure.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::SECTION_GAP;
use crate::pages::editors::ToolEditors;

pub fn render_crawler_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let can_start = state.crawler.can_start();
    let running = state.crawler.is_running();
    let paused = state.crawler.is_paused();
    let target = editors.crawler_target.clone();
    let exclude = editors.crawler_exclude.clone();
    let config = state.crawler.config.clone();
    let config_open = state.crawler.config_open;

    div()
        .id("crawler-toolbar")
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            // Control zone: target leads, actions trail. 12px between the
            // input and the action cluster; 8px within the cluster.
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(SECTION_GAP))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.))
                        .child(Input::new(&editors.crawler_target)),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .flex_none()
                        .when(can_start, |el| {
                            let target = target.clone();
                            let exclude = exclude.clone();
                            let config = config.clone();
                            el.child(
                                Button::new("crawler-start")
                                    .variant(ButtonVariant::Primary)
                                    .size(ButtonSize::Small)
                                    .leading_icon(PhosphorIcon::Play)
                                    .label("Start")
                                    .on_click(move |_, _, cx| {
                                        let mut crawl = config.clone();
                                        crawl.target_url = target.read(cx).value().to_string();
                                        crawl.exclude_paths = exclude.read(cx).value().to_string();
                                        let _ = Bridge::start_crawler_crawl(crawl, cx);
                                    }),
                            )
                        })
                        .when(running, |el| {
                            el.child(
                                Button::new("crawler-pause")
                                    .variant(ButtonVariant::Secondary)
                                    .size(ButtonSize::Small)
                                    .leading_icon(PhosphorIcon::Pause)
                                    .label("Pause")
                                    .on_click(|_, _, cx| {
                                        let _ = Bridge::pause_crawler_crawl(cx);
                                    }),
                            )
                            .child(
                                Button::new("crawler-stop")
                                    .variant(ButtonVariant::Destructive)
                                    .size(ButtonSize::Small)
                                    .leading_icon(PhosphorIcon::Stop)
                                    .label("Stop")
                                    .on_click(|_, _, cx| {
                                        let _ = Bridge::stop_crawler_crawl(cx);
                                    }),
                            )
                        })
                        .when(paused, |el| {
                            el.child(
                                Button::new("crawler-resume")
                                    .variant(ButtonVariant::Primary)
                                    .size(ButtonSize::Small)
                                    .leading_icon(PhosphorIcon::ArrowCounterClockwise)
                                    .label("Resume")
                                    .on_click(|_, _, cx| {
                                        let _ = Bridge::resume_crawler_crawl(cx);
                                    }),
                            )
                            .child(
                                Button::new("crawler-stop-paused")
                                    .variant(ButtonVariant::Destructive)
                                    .size(ButtonSize::Small)
                                    .leading_icon(PhosphorIcon::Stop)
                                    .label("Stop")
                                    .on_click(|_, _, cx| {
                                        let _ = Bridge::stop_crawler_crawl(cx);
                                    }),
                            )
                        })
                        .child(
                            Button::new("crawler-config-toggle")
                                .variant(ButtonVariant::Secondary)
                                .size(ButtonSize::Small)
                                .leading_icon(PhosphorIcon::SlidersHorizontal)
                                .trailing_icon(if config_open {
                                    PhosphorIcon::CaretUp
                                } else {
                                    PhosphorIcon::CaretDown
                                })
                                .label("Config")
                                .on_click(move |_, _, cx| {
                                    let _ = Bridge::set_crawler_config_open(!config_open, cx);
                                }),
                        ),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Authorized targets only. Crawl systems you have permission to test."),
        )
}

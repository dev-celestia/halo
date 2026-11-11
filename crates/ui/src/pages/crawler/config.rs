//! Inline crawl configuration — collapsible panel under the command bar.
//!
//! Sections group with space (PAGE_GAP between, gap_2 within). Toggle and
//! stepper chips sit on bordered control surfaces so they read as controls,
//! not labels. Active toggles stay Secondary so Start remains the sole primary.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::pages::chrome::{PAGE_GAP, radius_xs};
use crate::pages::editors::ToolEditors;

/// Crawl configuration panel body — rendered under the toolbar while open.
pub fn render_crawler_config(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let cfg = &state.crawler.config;

    div()
        .id("crawler-config")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.12))
        .px_3()
        .py_3()
        .child(config_section(
            "General",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(row_field(
                    "Exclude paths",
                    "Comma-separated paths to skip (/logout, /delete)",
                    Input::new(&editors.crawler_exclude),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(toggle_chip(
                            "crawler-same-domain",
                            "Same domain only",
                            cfg.same_domain_only,
                            |c| c.same_domain_only = !c.same_domain_only,
                        ))
                        .child(toggle_chip(
                            "crawler-headless",
                            "Headless",
                            cfg.headless,
                            |c| c.headless = !c.headless,
                        )),
                ),
            cx,
        ))
        .child(config_section(
            "Limits & timing",
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(stepper_chip(
                    "crawler-depth",
                    "Max depth",
                    cfg.max_depth,
                    1,
                    20,
                    |c, v| c.max_depth = v,
                    cx,
                ))
                .child(stepper_chip(
                    "crawler-pages",
                    "Max pages",
                    cfg.max_pages,
                    1,
                    10_000,
                    |c, v| c.max_pages = v,
                    cx,
                ))
                .child(stepper_chip_u64(
                    "crawler-delay",
                    "Delay ms",
                    cfg.request_delay_ms,
                    0,
                    30_000,
                    100,
                    |c, v| c.request_delay_ms = v,
                    cx,
                ))
                .child(stepper_chip_u64(
                    "crawler-timeout",
                    "Timeout ms",
                    cfg.timeout_ms,
                    1_000,
                    120_000,
                    1_000,
                    |c, v| c.timeout_ms = v,
                    cx,
                )),
            cx,
        ))
        .child(config_section(
            "Artifacts & AI",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(toggle_chip(
                            "crawler-ai",
                            "Heuristic insights",
                            cfg.enable_ai_insights,
                            |c| c.enable_ai_insights = !c.enable_ai_insights,
                        ))
                        .child(toggle_chip(
                            "crawler-screenshots",
                            "Screenshots",
                            cfg.capture_screenshots,
                            |c| c.capture_screenshots = !c.capture_screenshots,
                        ))
                        .child(toggle_chip(
                            "crawler-html",
                            "Rendered HTML",
                            cfg.capture_rendered_html,
                            |c| c.capture_rendered_html = !c.capture_rendered_html,
                        )),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(
                            "LLM page analysis ships with AI settings; heuristic findings run locally today.",
                        ),
                ),
            cx,
        ))
}

fn config_section(title: &str, body: impl IntoElement, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(cx.theme().muted_foreground)
                .child(title.to_uppercase()),
        )
        .child(body)
}

fn row_field(label: &str, hint: &str, input: impl IntoElement, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.foreground)
                .child(label.to_string()),
        )
        .child(input)
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(hint.to_string()),
        )
}

fn toggle_chip(
    id: &'static str,
    label: &'static str,
    on: bool,
    mutate: impl Fn(&mut halo_bridge::CrawlerCrawlConfig) + 'static + Clone,
) -> impl IntoElement {
    // Secondary when on — keeps Start as the page's only primary action.
    Button::new(id)
        .variant(if on {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        })
        .size(ButtonSize::Small)
        .label(label)
        .on_click(move |_, _, cx| {
            let mut config = Bridge::state(cx).read(cx).crawler.config.clone();
            mutate(&mut config);
            Bridge::set_crawler_config(config, cx);
        })
}

fn stepper_chip(
    id: &'static str,
    label: &'static str,
    value: u32,
    min: u32,
    max: u32,
    mutate: impl Fn(&mut halo_bridge::CrawlerCrawlConfig, u32) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let dec_id = SharedString::from(format!("{id}-dec"));
    let inc_id = SharedString::from(format!("{id}-inc"));
    let mutate_dec = mutate.clone();
    let mutate_inc = mutate;
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(
            div()
                .min_w(px(28.))
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .font_family("Menlo")
                .text_color(theme.foreground)
                .child(format!("{value}")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .rounded(radius_xs(theme))
                .overflow_hidden()
                .child(
                    Button::new(dec_id)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("−")
                        .on_click(move |_, _, cx| {
                            let mut config = Bridge::state(cx).read(cx).crawler.config.clone();
                            let next = value.saturating_sub(1).max(min);
                            mutate_dec(&mut config, next);
                            Bridge::set_crawler_config(config, cx);
                        }),
                )
                .child(
                    Button::new(inc_id)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("+")
                        .on_click(move |_, _, cx| {
                            let mut config = Bridge::state(cx).read(cx).crawler.config.clone();
                            let next = (value + 1).min(max);
                            mutate_inc(&mut config, next);
                            Bridge::set_crawler_config(config, cx);
                        }),
                ),
        )
}

fn stepper_chip_u64(
    id: &'static str,
    label: &'static str,
    value: u64,
    min: u64,
    max: u64,
    step: u64,
    mutate: impl Fn(&mut halo_bridge::CrawlerCrawlConfig, u64) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let dec_id = SharedString::from(format!("{id}-dec"));
    let inc_id = SharedString::from(format!("{id}-inc"));
    let mutate_dec = mutate.clone();
    let mutate_inc = mutate;
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(
            div()
                .min_w(px(40.))
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .font_family("Menlo")
                .text_color(theme.foreground)
                .child(format!("{value}")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .rounded(radius_xs(theme))
                .overflow_hidden()
                .child(
                    Button::new(dec_id)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("−")
                        .on_click(move |_, _, cx| {
                            let mut config = Bridge::state(cx).read(cx).crawler.config.clone();
                            let next = value.saturating_sub(step).max(min);
                            mutate_dec(&mut config, next);
                            Bridge::set_crawler_config(config, cx);
                        }),
                )
                .child(
                    Button::new(inc_id)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .label("+")
                        .on_click(move |_, _, cx| {
                            let mut config = Bridge::state(cx).read(cx).crawler.config.clone();
                            let next = (value + step).min(max);
                            mutate_inc(&mut config, next);
                            Bridge::set_crawler_config(config, cx);
                        }),
                ),
        )
}

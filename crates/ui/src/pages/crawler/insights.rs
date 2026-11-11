//! Recon findings — master list + detail, severity filter, interesting pages.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, CrawlerInsightDto};

use crate::pages::chrome::{PAGE_GAP, SECTION_GAP, empty_state, radius_xs};

pub fn render_insights_panel(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let filter = state.crawler.severity_filter.as_str();
    let mut insights: Vec<_> = state
        .crawler
        .insights
        .iter()
        .filter(|i| filter == "all" || i.severity.eq_ignore_ascii_case(filter))
        .cloned()
        .collect();
    insights.sort_by_key(|i| severity_rank(&i.severity));

    let selected = state
        .crawler
        .selected_insight_id
        .as_ref()
        .and_then(|id| insights.iter().find(|i| &i.id == id).cloned());
    let has_selection = selected.is_some();

    div()
        .id("crawler-insights")
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(
            // Filter chrome — severity controls trail identifying copy.
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(SECTION_GAP))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Recon observations from crawl evidence"),
                )
                .child(render_severity_filters(filter)),
        )
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h(px(200.))
                .flex()
                .flex_row()
                .gap(px(PAGE_GAP))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(240.))
                        .min_h_0()
                        .rounded(theme.radius)
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.background)
                        .overflow_hidden()
                        .child(
                            div()
                                .id("crawler-insights-list")
                                .size_full()
                                .overflow_y_scrollbar()
                                .px_2()
                                .py_2()
                                .flex()
                                .flex_col()
                                .gap_1p5()
                                .when(insights.is_empty(), |el| {
                                    el.child(empty_state(
                                        "No findings yet",
                                        "Start a crawl with heuristic insights enabled to surface interesting pages.",
                                        cx,
                                    ))
                                })
                                .children(
                                    insights
                                        .into_iter()
                                        .map(|insight| render_insight_row(insight, state, cx)),
                                ),
                        ),
                )
                .when_some(selected, |el, insight| {
                    el.child(
                        div()
                            .w(px(340.))
                            .flex_none()
                            .min_h_0()
                            .rounded(theme.radius)
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.muted.opacity(0.1))
                            .overflow_hidden()
                            .child(
                                div()
                                    .id("crawler-insight-detail")
                                    .size_full()
                                    .overflow_y_scrollbar()
                                    .px_3()
                                    .py_3()
                                    .child(render_insight_detail(insight, cx)),
                            ),
                    )
                })
                .when(!has_selection && !state.crawler.insights.is_empty(), |el| {
                    // Empty detail affordance so the master-detail layout stays stable.
                    el.child(
                        div()
                            .w(px(340.))
                            .flex_none()
                            .min_h_0()
                            .rounded(theme.radius)
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.muted.opacity(0.08))
                            .flex()
                            .items_center()
                            .justify_center()
                            .px_3()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Select a finding to inspect"),
                            ),
                    )
                }),
        )
        .child(render_interesting_pages(state, cx))
}

fn render_severity_filters(active: &str) -> impl IntoElement {
    // 8px between bordered/ghost chips — breathing room without losing density.
    div().flex().flex_wrap().items_center().gap_2().children(
        ["all", "critical", "high", "medium", "low", "info"]
            .into_iter()
            .map(|sev| {
                let selected = active.eq_ignore_ascii_case(sev);
                Button::new(SharedString::from(format!("sev-{sev}")))
                    .variant(if selected {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ButtonSize::Small)
                    .label(sev)
                    .on_click(move |_, _, cx| {
                        Bridge::set_crawler_severity_filter(sev.to_string(), cx);
                    })
            }),
    )
}

fn render_insight_row(
    insight: CrawlerInsightDto,
    state: &BridgeState,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let selected = state.crawler.selected_insight_id.as_deref() == Some(insight.id.as_str());
    let id = insight.id.clone();
    let (sev_bg, sev_fg) = severity_colors(&insight.severity, cx);

    div()
        .id(SharedString::from(format!("insight-{}", insight.id)))
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(if selected {
            theme.primary.opacity(0.45)
        } else {
            theme.border.opacity(0.0)
        })
        .bg(if selected {
            theme.accent.opacity(0.12)
        } else {
            gpui::transparent_black()
        })
        .when(!selected, |el| {
            el.hover(|s| s.bg(theme.muted.opacity(0.22)))
        })
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            Bridge::select_crawler_insight(Some(id.clone()), cx);
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .px_1p5()
                        .rounded(radius_xs(theme))
                        .bg(sev_bg)
                        .text_color(sev_fg)
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child(insight.severity.clone()),
                )
                .child(
                    div()
                        .px_1p5()
                        .rounded(radius_xs(theme))
                        .bg(if insight.ai_used {
                            theme.primary.opacity(0.16)
                        } else {
                            theme.muted.opacity(0.45)
                        })
                        .text_xs()
                        .text_color(if insight.ai_used {
                            theme.primary
                        } else {
                            theme.muted_foreground
                        })
                        .child(if insight.ai_used { "AI" } else { "Heuristic" }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(insight.finding_type.clone()),
                ),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.foreground)
                .child(insight.title.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(truncate(&insight.description, 140)),
        )
}

fn render_insight_detail(insight: CrawlerInsightDto, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let id = insight.id.clone();
    let (sev_bg, sev_fg) = severity_colors(&insight.severity, cx);

    div()
        .flex()
        .flex_col()
        .gap(px(SECTION_GAP))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .px_1p5()
                        .rounded(radius_xs(theme))
                        .bg(sev_bg)
                        .text_color(sev_fg)
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child(insight.severity.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(insight.finding_type.clone()),
                ),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.foreground)
                .child(insight.title.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(insight.description.clone()),
        )
        .when_some(insight.url.clone(), |el, url| {
            el.child(
                div()
                    .px_2()
                    .py_1p5()
                    .rounded(theme.radius_sm())
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .font_family("Menlo")
                    .text_color(theme.foreground)
                    .child(url),
            )
        })
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "{} · {}",
                    insight
                        .analysis_source
                        .clone()
                        .unwrap_or_else(|| "heuristic".into()),
                    insight.created_at
                )),
        )
        .child(
            Button::new("crawler-toggle-reviewed")
                .variant(if insight.reviewed {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Primary
                })
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Check)
                .label(if insight.reviewed {
                    "Reviewed"
                } else {
                    "Mark reviewed"
                })
                .on_click(move |_, _, cx| {
                    Bridge::toggle_crawler_insight_reviewed(&id, cx);
                }),
        )
}

fn render_interesting_pages(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let pages = state.crawler.interesting_pages();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.foreground)
                .child(format!("Interesting pages ({})", pages.len())),
        )
        .when(pages.is_empty(), |el| {
            el.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Nothing flagged yet for this crawl."),
            )
        })
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_1p5()
                .children(pages.into_iter().take(12).map(|page| {
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .px_2()
                        .py_1p5()
                        .rounded(theme.radius_sm())
                        .bg(theme.muted.opacity(0.15))
                        .text_xs()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(theme.foreground)
                                .child(page.title.clone().unwrap_or_else(|| page.url.clone())),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_family("Menlo")
                                .text_color(theme.muted_foreground)
                                .child(format!(
                                    "{} · {} links · {} forms",
                                    page.http_status
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| "—".into()),
                                    page.links_found,
                                    page.forms_found
                                )),
                        )
                })),
        )
}

fn severity_rank(sev: &str) -> u8 {
    match sev.to_ascii_lowercase().as_str() {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

fn severity_colors(sev: &str, cx: &App) -> (Hsla, Hsla) {
    let theme = cx.theme();
    match sev.to_ascii_lowercase().as_str() {
        "critical" | "high" => (theme.danger.opacity(0.22), theme.danger),
        "medium" => (theme.warning.opacity(0.22), theme.warning),
        "low" => (theme.primary.opacity(0.16), theme.primary),
        _ => (theme.muted.opacity(0.55), theme.muted_foreground),
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let truncated: String = value.chars().take(max).collect();
        format!("{truncated}…")
    }
}

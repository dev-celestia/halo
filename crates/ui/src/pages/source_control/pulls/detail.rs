//! Pull-request detail view: PR title, badges, author, and description.

use celestia_ui::components::traits::Size;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::BridgeState;

use crate::pages::source_control::author_avatar;

pub(crate) fn detail_pane(state: &BridgeState, cx: &App) -> Div {
    let theme = cx.theme();
    let Some(number) = state.pulls.selected else {
        return div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_6()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.foreground)
                    .child("Select a pull request"),
            )
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(theme.muted_foreground)
                    .child("The description shows here once a row is selected."),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("↑ ↓ to move · Enter to open"),
            );
    };

    let Some(pr) = state.pulls.items.iter().find(|pr| pr.number == number) else {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("That pull request is no longer in the list.");
    };

    let body = pr.body.trim();
    let body_text = if body.is_empty() {
        "No description.".to_string()
    } else {
        body.to_string()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .px_5()
                .pt_4()
                .pb_3()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(pr.title.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("#{}", pr.number))
                        .child(state_pill(&pr.state, cx))
                        .child(author_avatar(
                            if pr.author.is_empty() {
                                "Unknown"
                            } else {
                                pr.author.as_str()
                            },
                            "",
                            Size::Size(px(18.)),
                        ))
                        .child(if pr.author.is_empty() {
                            "Unknown author".to_string()
                        } else {
                            pr.author.clone()
                        }),
                ),
        )
        .child(
            div()
                .id("sc-pr-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .px_5()
                .pb_4()
                .text_sm()
                .text_color(if body.is_empty() {
                    theme.muted_foreground
                } else {
                    theme.foreground
                })
                .child(body_text),
        )
}

fn state_pill(state: &str, cx: &App) -> Div {
    let theme = cx.theme();
    let color = match state.to_ascii_uppercase().as_str() {
        "OPEN" => theme.accent,
        "MERGED" => theme.success,
        _ => theme.muted_foreground,
    };
    div()
        .px_1p5()
        .rounded(theme.radius_sm())
        .bg(color.opacity(0.16))
        .text_color(color)
        .child(state_label(state))
}

fn state_label(state: &str) -> String {
    let lower = state.to_ascii_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

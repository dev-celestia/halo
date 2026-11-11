//! Pull-request list column: search, filter tabs, keyboard navigation, and PR rows.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::components::primitive::tabs::{TabsList, TabsSize, TabsTrigger};
use celestia_ui::components::traits::Size;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{Bridge, BridgeState, PullRequest, PullRequestFilter};

use crate::pages::editors::ToolEditors;
use crate::pages::source_control::author_avatar;
use crate::pages::source_control::projects::render_project_switcher;

pub(crate) const LIST_WIDTH: Pixels = px(320.);
pub(crate) const LIST_MIN: Pixels = px(240.);
pub(crate) const LIST_MAX: Pixels = px(480.);

pub(crate) fn list_column(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let pulls = &state.pulls;
    let open_count = pulls.open_count();
    let loading = pulls.loading;

    div()
        .id("sc-pr-list")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .w_full()
                .flex_none()
                .px_3()
                .py_1p5()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("Pull Requests"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(render_project_switcher(state, cx)),
                )
                .child(
                    Button::new("sc-pr-refresh")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Icon)
                        .leading_icon(PhosphorIcon::ArrowClockwise)
                        .tooltip("Refresh")
                        .disabled(loading || state.coder.root_path.is_none())
                        .on_click(|_, _, cx| {
                            let _ = Bridge::git_pulls_refresh(cx);
                        }),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap_4()
                .px_3()
                .pt_3()
                .pb_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_3()
                        .child(filter_row(pulls.filter, open_count))
                        .child(div().w_full().child(Input::new(&editors.sc_pr_search))),
                )
                .child(list_body(state, editors, cx)),
        )
}

fn filter_row(selected: PullRequestFilter, open_count: usize) -> impl IntoElement {
    let chips = [
        (
            PullRequestFilter::Open,
            "sc-pr-filter-open",
            format!("Open ({open_count})"),
        ),
        (
            PullRequestFilter::Merged,
            "sc-pr-filter-merged",
            "Merged".into(),
        ),
        (
            PullRequestFilter::Closed,
            "sc-pr-filter-closed",
            "Closed".into(),
        ),
        (PullRequestFilter::All, "sc-pr-filter-all", "All".into()),
    ];

    TabsList::new("sc-pr-filters")
        .size(TabsSize::Small)
        .children(chips.into_iter().map(|(filter, id, label)| {
            TabsTrigger::new(id)
                .label(label)
                .selected(filter == selected)
                .on_click(move |_, _, cx| {
                    Bridge::git_pulls_set_filter(filter, cx);
                })
        }))
}

fn list_body(state: &BridgeState, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let pulls = &state.pulls;

    if state.coder.root_path.is_none() {
        return centered_note(cx, "Add a project to load pull requests.").into_any_element();
    }
    if pulls.loading && pulls.items.is_empty() {
        return centered_note(cx, "Loading pull requests…").into_any_element();
    }
    if let Some(err) = pulls.error.clone() {
        return div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.foreground)
                    .child("Unable to load pull requests"),
            )
            .child(
                div()
                    .text_xs()
                    .text_center()
                    .text_color(theme.muted_foreground)
                    .child(err),
            )
            .child(
                Button::new("sc-pr-try-again")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Small)
                    .label("Try again")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::git_pulls_refresh(cx);
                    }),
            )
            .into_any_element();
    }

    let visible = pulls.visible();
    if visible.is_empty() {
        return centered_note(cx, pulls.filter.empty_label()).into_any_element();
    }

    let selected = pulls.selected;
    let focus = editors.sc_pr_focus.clone();
    let mut list = div()
        .id("sc-pr-rows")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap_0p5()
        .overflow_y_scrollbar()
        .track_focus(&focus)
        .on_key_down(|event: &KeyDownEvent, _, cx| {
            let keystroke = &event.keystroke;
            if keystroke.modifiers.alt
                || keystroke.modifiers.control
                || keystroke.modifiers.shift
                || keystroke.modifiers.platform
            {
                return;
            }
            match keystroke.key.as_str() {
                "down" => Bridge::git_pulls_move(1, cx),
                "up" => Bridge::git_pulls_move(-1, cx),
                "enter" => {
                    let selected = Bridge::state(cx).read(cx).pulls.selected;
                    if selected.is_none() {
                        Bridge::git_pulls_move(1, cx);
                    }
                }
                _ => return,
            }
            cx.stop_propagation();
        });

    for pr in visible {
        list = list.child(pr_row(pr, selected == Some(pr.number), &focus, cx));
    }
    list.into_any_element()
}

fn pr_row(pr: &PullRequest, selected: bool, focus: &FocusHandle, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let number = pr.number;
    let author = if pr.author.is_empty() {
        "Unknown".to_string()
    } else {
        pr.author.clone()
    };

    div()
        .id(SharedString::from(format!("sc-pr-row-{number}")))
        .w_full()
        .flex()
        .flex_row()
        .items_start()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .cursor_pointer()
        .when(selected, |s| s.bg(theme.accent.opacity(0.16)))
        .hover(|s| s.bg(theme.accent.opacity(if selected { 0.2 } else { 0.08 })))
        .on_click({
            let focus = focus.clone();
            move |_, window, cx| {
                window.focus(&focus, cx);
                Bridge::git_pulls_select(number, cx);
            }
        })
        .child(author_avatar(author.clone(), "", Size::Size(px(22.))))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .truncate()
                        .child(pr.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(format!("#{number} · {author}")),
                ),
        )
}

fn centered_note(cx: &App, message: &str) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .px_3()
        .text_sm()
        .text_center()
        .text_color(cx.theme().muted_foreground)
        .child(message.to_string())
}

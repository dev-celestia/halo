//! Toolbar: search, actions, and multi-select filter dropdowns.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::Input;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, BridgeState, FilterChip, HistoryFilter};

use crate::components::confirm_destructive;
use crate::pages::editors::ToolEditors;

const METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];
const STATUS_CLASSES: &[(u16, &str)] = &[(2, "2xx"), (3, "3xx"), (4, "4xx"), (5, "5xx")];

pub fn render_http_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let count = state.history.len();
    let filter = &state.history_filter;
    let search = editors.http_search.clone();

    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        // Filters lead the row; the search cluster trails it, so the field sits
        // against the trailing edge instead of stretching away from its actions.
        .child(filter_group(filter))
        .child(
            Button::new("http-clear-filters")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .label("Reset filters")
                .on_click(|_, _, cx| {
                    let _ = Bridge::clear_history_filters(cx);
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .text_xs()
                .text_color(muted)
                .child(format!("{count} entries")),
        )
        // Spacer: eats the slack so the divider + search cluster ride the right
        // edge. A fixed `min_w` keeps it from collapsing to nothing on wrap.
        .child(div().flex_1().min_w(px(8.)))
        // Explicit divider: the filters end here, the search cluster begins.
        .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
        .child(
            div()
                .flex_1()
                .min_w(px(180.))
                .max_w(px(360.))
                .child(Input::new(&editors.http_search)),
        )
        // Extra breathing room between the field and its actions: the row gap
        // alone leaves them looking welded together.
        .child(div().w(px(8.)))
        // The action cluster is icon-only: the search field submits on Enter,
        // so the glyph is enough for the rest too.
        .child(
            Button::icon("http-search-apply", PhosphorIcon::MagnifyingGlass)
                .variant(ButtonVariant::Secondary)
                .accessibility_label("Apply search")
                .tooltip("Apply search (Enter)")
                .on_click(move |_, _, cx| {
                    let q = search.read(cx).value().to_string();
                    let _ = Bridge::set_history_search(q, cx);
                }),
        )
        .child(
            Button::icon("http-refresh", PhosphorIcon::ArrowClockwise)
                .variant(ButtonVariant::Secondary)
                .accessibility_label("Refresh history")
                .tooltip("Refresh captured requests")
                .on_click(|_, _, cx| {
                    let _ = Bridge::refresh_history(cx);
                }),
        )
        .child(
            Button::icon("http-clear", PhosphorIcon::Trash)
                .variant(ButtonVariant::Destructive)
                .accessibility_label("Clear history")
                .tooltip("Clear captured requests")
                .disabled(count == 0)
                .on_click(move |_, window, cx| {
                    confirm_destructive(
                        window,
                        cx,
                        "Clear history?",
                        format!(
                            "{count} captured request(s) will be removed from this tab. This cannot be undone."
                        ),
                        "Clear",
                        |_, cx| {
                            let _ = Bridge::clear_history(cx);
                        },
                    );
                }),
        )
}

/// The Method / Status / Options dropdowns as one group, so they read as a
/// single control cluster instead of three loose buttons.
fn filter_group(filter: &HistoryFilter) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(method_filter_dropdown(filter))
        .child(status_filter_dropdown(filter))
        .child(options_filter_dropdown(filter))
}

fn method_filter_dropdown(filter: &HistoryFilter) -> impl IntoElement {
    let selected: Vec<String> = METHODS
        .iter()
        .filter(|method| {
            filter
                .methods
                .iter()
                .any(|m| m.eq_ignore_ascii_case(method))
        })
        .map(|m| (*m).to_string())
        .collect();
    let active = !selected.is_empty();
    let label = if selected.is_empty() {
        "Method".to_string()
    } else if selected.len() <= 2 {
        selected.join(", ")
    } else {
        format!("Method · {}", selected.len())
    };

    Button::new("http-filter-method")
        .variant(if active {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        })
        .size(ButtonSize::Small)
        .label(label)
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu;
            for method in METHODS {
                let checked = selected.iter().any(|m| m.eq_ignore_ascii_case(method));
                let method = (*method).to_string();
                menu = menu.item(
                    PopupMenuItem::new(method.clone())
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::toggle_method_filter(&method, cx);
                        }),
                );
            }
            menu
        })
}

fn status_filter_dropdown(filter: &HistoryFilter) -> impl IntoElement {
    let selected: Vec<(u16, &'static str)> = STATUS_CLASSES
        .iter()
        .copied()
        .filter(|(class, _)| filter.status_classes.contains(class))
        .collect();
    let active = !selected.is_empty();
    let label = if selected.is_empty() {
        "Status".to_string()
    } else {
        selected
            .iter()
            .map(|(_, label)| *label)
            .collect::<Vec<_>>()
            .join(", ")
    };

    Button::new("http-filter-status")
        .variant(if active {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        })
        .size(ButtonSize::Small)
        .label(label)
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu;
            for (class, label) in STATUS_CLASSES {
                let checked = selected.iter().any(|(c, _)| c == class);
                let class = *class;
                menu = menu.item(PopupMenuItem::new(*label).checked(checked).on_click(
                    move |_, _, cx| {
                        let _ = Bridge::toggle_status_class(class, cx);
                    },
                ));
            }
            menu
        })
}

fn options_filter_dropdown(filter: &HistoryFilter) -> impl IntoElement {
    let chips: [(&'static str, bool, FilterChip); 4] = [
        ("Hide 304", filter.hide_304, FilterChip::Hide304),
        (
            "Hide Images/Fonts",
            filter.hide_images,
            FilterChip::HideImages,
        ),
        ("Only API (JSON)", filter.only_api, FilterChip::OnlyApi),
        ("Errors Only", filter.errors_only, FilterChip::ErrorsOnly),
    ];
    let active_count = chips.iter().filter(|(_, on, _)| *on).count();
    let active = active_count > 0;
    let label = if active_count == 0 {
        "Options".to_string()
    } else if active_count == 1 {
        chips
            .iter()
            .find(|(_, on, _)| *on)
            .map(|(label, _, _)| (*label).to_string())
            .unwrap_or_else(|| "Options".to_string())
    } else {
        format!("Options · {active_count}")
    };

    Button::new("http-filter-options")
        .variant(if active {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Ghost
        })
        .size(ButtonSize::Small)
        .label(label)
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu;
            for (label, checked, chip) in chips {
                menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                    move |_, _, cx| {
                        let _ = Bridge::toggle_filter_chip(chip, cx);
                    },
                ));
            }
            menu
        })
}

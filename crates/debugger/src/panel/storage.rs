//! Storage tab — disk footprint + prefs JSON tree + clear actions.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState};

use crate::confirm::confirm_destructive;
use crate::state::DebuggerState;
use crate::storage::{self, PrefsNode, format_bytes};

pub fn render(
    state: &DebuggerState,
    bridge: &BridgeState,
    on_select_prefs: impl Fn(Option<String>, &mut App) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let busy = bridge.storage_busy;

    let prefs = bridge
        .storage
        .as_ref()
        .map(|s| storage::load_prefs_tree(&s.app_data_dir))
        .unwrap_or_default();

    div()
        .id("debugger-storage")
        .size_full()
        .flex()
        .min_h_0()
        .child(
            div()
                .id("debugger-storage-summary")
                .w(px(300.))
                .flex_none()
                .h_full()
                .min_h_0()
                .border_r_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .px_2()
                .py_2()
                .gap_2()
                .overflow_y_scrollbar()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .child("Disk footprint"),
                )
                .child(match bridge.storage.as_ref() {
                    Some(info) => div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(size_row(
                            "Database & payloads",
                            info.database_size_bytes,
                            cx,
                        ))
                        .child(size_row("Log file", info.log_file_size_bytes, cx))
                        .child(size_row(
                            "Crawler artifacts",
                            info.crawler_artifacts_size_bytes,
                            cx,
                        ))
                        .child(size_row(
                            "Regression artifacts",
                            info.regression_artifacts_size_bytes,
                            cx,
                        ))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(info.app_data_dir.clone()),
                        )
                        .into_any_element(),
                    None => div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Loading storage info…")
                        .into_any_element(),
                })
                .child(actions_row(busy, cx)),
        )
        .child(
            div()
                .id("debugger-storage-tree")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Prefs & settings (read-only)"),
                )
                .child(
                    div()
                        .id("debugger-storage-tree-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .px_2()
                        .py_1()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .children(prefs.roots.into_iter().map(|node| {
                                    let on_select = on_select_prefs.clone();
                                    let selected = state.selected_prefs_path.as_deref();
                                    prefs_node_view(&node, 0, selected, on_select, cx)
                                }))
                                .children(prefs.errors.into_iter().map(|err| {
                                    div()
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(theme.danger)
                                        .child(err)
                                })),
                        ),
                )
                .child(
                    div()
                        .id("debugger-storage-detail-scroll")
                        .flex_none()
                        .border_t_1()
                        .border_color(theme.border)
                        .px_2()
                        .py_2()
                        .h(px(100.))
                        .overflow_y_scrollbar()
                        .child(prefs_detail(state, &bridge, cx)),
                ),
        )
}

fn size_row(label: &str, bytes: u64, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex_shrink_0()
        .flex()
        .justify_between()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(theme.foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format_bytes(bytes)),
        )
}

fn actions_row(busy: bool, cx: &App) -> impl IntoElement {
    let _ = cx;
    div()
        .flex()
        .flex_wrap()
        .gap_1()
        .pt_1()
        .child(
            Button::new("dbg-storage-refresh")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .label("Refresh")
                .disabled(busy)
                .on_click(|_, _, cx| {
                    let _ = Bridge::refresh_storage(cx);
                }),
        )
        .child(clear_btn(
            "database",
            "Clear DB",
            "Clear database?",
            "The database file, WAL/SHM, and payload segments will be deleted and an empty schema recreated. This cannot be undone.",
            busy,
        ))
        .child(clear_btn(
            "log_file",
            "Clear log",
            "Clear log file?",
            "logs/halo.log will be truncated. This cannot be undone.",
            busy,
        ))
        .child(clear_btn(
            "crawler_artifacts",
            "Clear crawler",
            "Clear crawler artifacts?",
            "Screenshots and DOM snapshots under crawler-artifacts will be removed. This cannot be undone.",
            busy,
        ))
}

fn clear_btn(
    artifact: &'static str,
    label: &'static str,
    title: &'static str,
    body: &'static str,
    busy: bool,
) -> Button {
    Button::icon(
        SharedString::from(format!("dbg-clear-{artifact}")),
        PhosphorIcon::Trash,
    )
    .variant(ButtonVariant::Destructive)
    .size(ButtonSize::Small)
    .accessibility_label(label)
    .tooltip(label)
    .disabled(busy)
    .on_click(move |_, window, cx| {
        confirm_destructive(window, cx, title, body, "Clear", |_, cx| {
            let _ = Bridge::delete_storage_artifact(artifact, cx);
        });
    })
}

fn prefs_node_view(
    node: &PrefsNode,
    depth: usize,
    selected: Option<&str>,
    on_select: impl Fn(Option<String>, &mut App) + 'static + Clone,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let is_selected = selected == Some(node.path.as_str());
    let path = node.path.clone();
    let on_select_click = on_select.clone();

    let row = div()
        .w_full()
        .flex_shrink_0()
        .pl(px(8. * depth as f32))
        .py_0p5()
        .child(
            div()
                .id(SharedString::from(format!("prefs-{}", node.path)))
                .w_full()
                .flex()
                .items_center()
                .gap_1()
                .px_1p5()
                .py_0p5()
                .rounded(px(6.))
                .cursor_pointer()
                .when(is_selected, |this| {
                    this.bg(theme.accent.opacity(0.22))
                        .border_1()
                        .border_color(theme.accent.opacity(0.45))
                })
                .when(!is_selected, |this| {
                    this.border_1().border_color(gpui::transparent_black())
                })
                .hover(|this| {
                    if is_selected {
                        this
                    } else {
                        this.bg(theme.accent.opacity(0.10))
                    }
                })
                .on_click(move |_, _, cx| on_select_click(Some(path.clone()), cx))
                .child(
                    div()
                        .text_xs()
                        .font_family("monospace")
                        .text_color(theme.foreground)
                        .child(node.key.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .overflow_hidden()
                        .child(node.value_preview.clone()),
                ),
        )
        .into_any_element();

    if node.children.is_empty() {
        return row;
    }

    // Wrap parent + children
    let mut col = div().flex().flex_col().child(row);
    for child in &node.children {
        col = col.child(prefs_node_view(
            child,
            depth + 1,
            selected,
            on_select.clone(),
            cx,
        ));
    }
    col.into_any_element()
}

fn prefs_detail(state: &DebuggerState, bridge: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let Some(path) = state.selected_prefs_path.as_deref() else {
        return div()
            .w_full()
            .py_2()
            .flex()
            .justify_center()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .text_center()
                    .child("Select a prefs key to see its path."),
            )
            .into_any_element();
    };

    let app_dir = bridge
        .storage
        .as_ref()
        .map(|s| s.app_data_dir.as_str())
        .unwrap_or("");

    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .child(path.to_string()),
        )
        .child(
            div()
                .text_xs()
                .font_family("monospace")
                .text_color(theme.muted_foreground)
                .child(format!("{app_dir}/{path}")),
        )
        .into_any_element()
}

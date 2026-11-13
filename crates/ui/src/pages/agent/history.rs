//! Agent Sessions rail — left column.
//!
//! Header: New chat + Add projects. Body: projects as accordion sections with
//! chats nested underneath (Cursor Agents pattern). Each chat row trails a
//! compact relative last-activity time (`4h`, `6d`); hover swaps it for
//! archive/delete. Archived shelf at the bottom.

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::accordion::Accordion;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::loader::Loader;
use celestia_ui::theme::ActiveTheme as _;
use chrono::{Local, TimeZone};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::{Sizable as _, Size};
use halo_bridge::{Bridge, BridgeState, HaloAgentSession, HaloAgentSessionStatus};

use super::META_LINE_HEIGHT;
use crate::components::{confirm_delete, confirm_destructive};
use crate::pages::coder::project_display_name;

/// Width of the sessions rail when open.
pub(super) const RAIL_WIDTH: Pixels = px(260.);

/// Accordion key for sessions with no `root_path`.
const NO_PROJECT_KEY: &str = "";

pub(super) fn render_sessions_rail(state: &BridgeState, cx: &App) -> AnyElement {
    if !state.coder.agent_history_open {
        return Empty.into_any_element();
    }

    let theme = cx.theme();
    let border = theme.border;
    let surface = theme.background;
    let accent = theme.accent;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let muted_bg = theme.muted;
    let skeleton = theme.skeleton;
    let row_radius = theme.radius_sm();
    let active_id = state.coder.agent_active_session_id.clone();
    let current_root = state.coder.root_path.clone();
    let loading_root = state.coder.agent_project_loading.clone();

    let active: Vec<_> = state
        .coder
        .agent_sessions_sorted()
        .into_iter()
        .filter(|s| s.id == active_id || !s.messages.is_empty())
        .collect();
    let archived = state.coder.agent_archived_sessions_sorted();
    let groups = project_groups(&active, &state.coder_projects, current_root.as_deref());
    let group_keys: Vec<String> = groups.iter().map(|(k, _)| k.clone()).collect();

    let success = theme.success;
    let danger = theme.danger;

    // No horizontal padding on the list — accordion item `border_b` separators
    // must run edge-to-edge. Content insets live on titles / session rows.
    let mut list = div()
        .id("coder-agent-sessions-list")
        .w_full()
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .flex()
        .flex_col()
        .gap(px(2.))
        .pb(px(12.));

    if groups.is_empty() && archived.is_empty() {
        list = list.child(
            div()
                .w_full()
                .px(px(14.))
                .py(px(16.))
                .text_xs()
                .text_color(muted)
                .child("No projects or chats yet"),
        );
    } else if !groups.is_empty() {
        let mut accordion = Accordion::new("agent-rail-projects")
            .multiple(true)
            .bordered(false)
            .with_size(Size::XSmall)
            // Default accordion is height 100% with each item set to grow, so
            // leftover rail height opens as an empty band under every project.
            .h_auto()
            .flex_none()
            .w_full()
            .on_toggle_click({
                let keys = group_keys.clone();
                move |open_ixs, _, cx| {
                    let _ = Bridge::halo_agent_set_rail_open_sections(open_ixs, &keys, cx);
                }
            });

        for (key, sessions) in &groups {
            let open = state.coder.agent_rail_section_open(key);
            let is_loading = loading_root.as_deref() == Some(key.as_str());
            let is_current = current_root.as_deref() == Some(key.as_str())
                || (key.is_empty() && current_root.is_none())
                || is_loading;
            let title = project_section_title(
                key, open, is_current, is_loading, foreground, muted, muted_bg, accent,
            );
            let body = project_section_body(
                key, sessions, &active_id, is_loading, foreground, muted, muted_bg, accent,
                success, danger, skeleton, row_radius,
            );
            // Keep the rail flat like the charcoal canvas — only chat rows
            // pick up a muted chip; whole-section fills read as gray slabs.
            // Zero Accordion's size-based side padding so separators (item
            // border_b) and content share the rail width; titles/rows pad.
            accordion = accordion.item(move |item| {
                item.title(title)
                    .show_chevron(false)
                    .open(open)
                    .title_style(StyleRefinement::default().px(px(0.)).py(px(0.)))
                    .content_style(StyleRefinement::default().px(px(0.)).pb(px(0.)))
                    .bg(surface)
                    .hover(move |s| s.bg(muted_bg.opacity(0.35)))
                    .child(body)
            });
        }

        list = list.child(accordion);
    }

    if !archived.is_empty() {
        list = list.child(
            div()
                .w_full()
                .px(px(14.))
                .pt(px(14.))
                .pb(px(4.))
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted)
                .child(format!("Archived · {}", archived.len())),
        );
        for session in &archived {
            list = list.child(session_row(
                session,
                session.id == active_id,
                true,
                foreground,
                muted,
                muted_bg,
                accent,
                success,
                danger,
                row_radius,
            ));
        }
    }

    celestia_ui::motion::fade_in(
        "coder-agent-sessions-rail-in",
        div()
            .id("coder-agent-sessions-rail")
            .h_full()
            .w(RAIL_WIDTH)
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(border)
            .bg(surface)
            .child(rail_header(state, border, muted))
            .child(list),
    )
    .into_any_element()
}

/// Icon and label for a rail action. Small buttons only leave 4px between a
/// leading icon and its word; these rows use the same 10px the chat titles use,
/// at the chat title size.
fn rail_action_label(icon: PhosphorIcon, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .text_sm()
        .line_height(px(20.))
        .child(Phosphor::new(icon).size(px(16.)))
        .child(label)
}

fn rail_header(state: &BridgeState, border: Hsla, muted: Hsla) -> impl IntoElement {
    let recent: Vec<String> = state.coder_projects.clone();
    let current = state.coder.root_path.clone();

    div()
        .w_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(10.))
        .pt(px(8.))
        .pb(px(6.))
        .border_b_1()
        .border_color(border)
        .child(
            div().w_full().flex().justify_start().child(
                Button::new("coder-agent-rail-new-chat")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .child(rail_action_label(PhosphorIcon::Plus, "New chat"))
                    .accessibility_label("New chat")
                    .tooltip("Start a new agent session")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::halo_agent_new_chat(cx);
                    }),
            ),
        )
        .child(
            div().w_full().flex().justify_start().child(
                Button::new("coder-agent-rail-add-project")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .child(rail_action_label(PhosphorIcon::FolderPlus, "Add projects"))
                    .dropdown_caret(true)
                    .accessibility_label("Add projects")
                    .tooltip("Open a folder or pick a recent project")
                    .dropdown_menu({
                        let recent = recent.clone();
                        let current = current.clone();
                        move |menu, _, _| {
                            let mut menu = menu.item(
                                PopupMenuItem::new("Open Folder…")
                                    .icon(PhosphorIcon::FolderOpen)
                                    .on_click(|_, _, cx| {
                                        let _ = Bridge::coder_open_folder(cx);
                                    }),
                            );
                            if !recent.is_empty() {
                                menu = menu.separator();
                                for path in &recent {
                                    let label = project_display_name(path);
                                    let checked = current.as_deref() == Some(path.as_str());
                                    let open_path = path.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(label).checked(checked).on_click(
                                            move |_, _, cx| {
                                                let _ = Bridge::halo_agent_focus_project(
                                                    &open_path, cx,
                                                );
                                            },
                                        ),
                                    );
                                }
                            }
                            menu
                        }
                    }),
            ),
        )
        .child(
            div()
                .w_full()
                .pt(px(4.))
                .px(px(4.))
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .font_weight(FontWeight::NORMAL)
                .text_color(muted)
                .child("Projects"),
        )
}

fn project_section_title(
    key: &str,
    is_open: bool,
    is_current: bool,
    is_loading: bool,
    foreground: Hsla,
    muted: Hsla,
    muted_bg: Hsla,
    accent: Hsla,
) -> impl IntoElement {
    let label = if key.is_empty() {
        "No project".to_string()
    } else {
        project_display_name(key)
    };
    let add_tooltip = if key.is_empty() {
        "New chat".to_string()
    } else {
        format!("New chat in {label}")
    };
    let caret = if is_open {
        PhosphorIcon::CaretDown
    } else {
        PhosphorIcon::CaretRight
    };
    let icon = if key.is_empty() {
        PhosphorIcon::FolderSimple
    } else if is_open {
        PhosphorIcon::FolderOpen
    } else {
        PhosphorIcon::Folder
    };
    let switch_path = key.to_string();
    let new_chat_path = key.to_string();
    let remove_path = key.to_string();
    let remove_label = label.clone();
    let can_switch = !key.is_empty() && !is_current;
    let can_remove = !key.is_empty();
    let tooltip_path = key.to_string();
    let tone = if is_current { foreground } else { muted };
    let add_id = SharedString::from(format!(
        "agent-rail-project-add-{}",
        if key.is_empty() { "_" } else { key }
    ));
    let more_id = SharedString::from(format!(
        "agent-rail-project-more-{}",
        if key.is_empty() { "_" } else { key }
    ));

    // Layout: [caret] [folder] [name] …… [+] [⋯]
    // `context_menu` wraps the element type, so it cannot sit inside `.when`.
    let title = div()
        .id(SharedString::from(format!(
            "agent-rail-project-title-{}",
            if key.is_empty() { "_" } else { key }
        )))
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .py(px(8.))
        .min_w_0()
        .when(is_current, |el| el.rounded(px(4.)).bg(muted_bg.opacity(0.55)))
        .when(can_switch, |el| {
            // Stop accordion toggle — switch expands via focus_project so the
            // section never flash-collapses on an already-open project.
            el.cursor_pointer()
                .rounded(px(4.))
                .hover(move |s| s.bg(accent.opacity(0.55)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = Bridge::halo_agent_focus_project(&switch_path, cx);
                })
        })
        .when(!key.is_empty(), |el| {
            el.tooltip(move |window, cx| {
                let tip = if can_switch {
                    format!("Switch to {tooltip_path}")
                } else {
                    tooltip_path.clone()
                };
                Tooltip::new(tip).build(window, cx)
            })
        })
        .child(Phosphor::new(caret).size(px(14.)).color(muted))
        .child(
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(Phosphor::new(icon).size(px(16.)).color(tone))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .line_height(px(20.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(tone)
                        .child(label),
                ),
        )
        .when(is_loading, |el| {
            el.child(
                Loader::grid_shift()
                    .size(px(12.))
                    .color(muted.opacity(0.85)),
            )
        })
        .child(div().flex_1())
        .child(
            // Isolate from AccordionTrigger's header click so + / ⋯ never fold.
            div()
                .id(SharedString::from(format!(
                    "agent-rail-project-actions-{}",
                    if key.is_empty() { "_" } else { key }
                )))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(1.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    Button::icon(add_id, PhosphorIcon::Plus)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Icon)
                        .accessibility_label("New chat in project")
                        .tooltip(add_tooltip)
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            let _ = Bridge::halo_agent_new_chat_for_project(&new_chat_path, cx);
                        }),
                )
                .when(can_remove, |el| {
                    let path = remove_path.clone();
                    let name = remove_label.clone();
                    el.child(
                        Button::icon(more_id, PhosphorIcon::DotsThreeVertical)
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Icon)
                            .accessibility_label("Project actions")
                            .tooltip("Project actions")
                            .dropdown_menu(move |menu, _, _| {
                                let path = path.clone();
                                let name = name.clone();
                                menu.item(
                                    PopupMenuItem::new("Remove")
                                        .icon(PhosphorIcon::Trash)
                                        .on_click(move |_, window, cx| {
                                            let path = path.clone();
                                            let name = name.clone();
                                            confirm_destructive(
                                                window,
                                                cx,
                                                "Remove project?",
                                                format!(
                                                    "\"{name}\" and all of its agent chats will be removed from the sessions list. Files on disk are not deleted."
                                                ),
                                                "Remove",
                                                move |_, cx| {
                                                    let _ =
                                                        Bridge::halo_agent_remove_project(&path, cx);
                                                },
                                            );
                                        }),
                                )
                            }),
                    )
                }),
        );

    if !can_remove {
        return title.into_any_element();
    }

    title
        .context_menu(move |menu, _, _| {
            let path = remove_path.clone();
            let name = remove_label.clone();
            menu.item(
                PopupMenuItem::new("Remove")
                    .icon(PhosphorIcon::Trash)
                    .on_click(move |_, window, cx| {
                        let path = path.clone();
                        let name = name.clone();
                        confirm_destructive(
                            window,
                            cx,
                            "Remove project?",
                            format!(
                                "\"{name}\" and all of its agent chats will be removed from the sessions list. Files on disk are not deleted."
                            ),
                            "Remove",
                            move |_, cx| {
                                let _ = Bridge::halo_agent_remove_project(&path, cx);
                            },
                        );
                    }),
            )
        })
        .into_any_element()
}

fn project_section_body(
    project_key: &str,
    sessions: &[HaloAgentSession],
    active_id: &str,
    is_loading: bool,
    foreground: Hsla,
    muted: Hsla,
    muted_bg: Hsla,
    accent: Hsla,
    success: Hsla,
    danger: Hsla,
    skeleton: Hsla,
    row_radius: Pixels,
) -> impl IntoElement {
    let body_id = if project_key.is_empty() {
        "agent-rail-project-body-_".to_string()
    } else {
        format!("agent-rail-project-body-{project_key}")
    };
    let mut body = div()
        .id(SharedString::from(body_id))
        .w_full()
        .flex()
        .flex_col()
        .gap(px(1.));

    // While the workspace hydrates and this project has no chats yet, keep the
    // expanded body stable with skeleton rows instead of an empty flash.
    if is_loading && sessions.is_empty() {
        body = body.child(project_sessions_skeleton(skeleton, row_radius));
    } else if sessions.is_empty() {
        let switch_path = project_key.to_string();
        let can_switch = !project_key.is_empty();
        body = body.child(
            div()
                .id(SharedString::from(format!(
                    "agent-rail-project-empty-{}",
                    if project_key.is_empty() {
                        "_"
                    } else {
                        project_key
                    }
                )))
                .w_full()
                .px(px(10.))
                .py(px(6.))
                .rounded(row_radius)
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted)
                .when(can_switch, |el| {
                    el.cursor_pointer()
                        .hover(move |s| s.bg(accent))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            let _ = Bridge::halo_agent_focus_project(&switch_path, cx);
                        })
                        .child("No chats yet — click to open")
                })
                .when(!can_switch, |el| el.child("No chats yet")),
        );
    } else {
        for session in sessions {
            body = body.child(session_row(
                session,
                session.id == active_id,
                false,
                foreground,
                muted,
                muted_bg,
                accent,
                success,
                danger,
                row_radius,
            ));
        }
    }

    body
}

/// Placeholder session rows so an expanding empty project keeps height while
/// the workspace hydrate finishes.
fn project_sessions_skeleton(skeleton: Hsla, row_radius: Pixels) -> impl IntoElement {
    const WIDTHS: &[f32] = &[148., 112., 136.];
    let mut block = div()
        .id("coder-agent-project-sessions-skeleton")
        .w_full()
        .flex()
        .flex_col()
        .gap(px(1.))
        .aria_label("Loading project");

    for (i, w) in WIDTHS.iter().enumerate() {
        block = block.child(
            div()
                .id(SharedString::from(format!("coder-agent-project-skel-{i}")))
                .w_full()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(10.))
                .py(px(6.))
                .rounded(row_radius)
                .child(
                    div()
                        .flex_none()
                        .size(px(8.))
                        .rounded_full()
                        .bg(skeleton.opacity(0.75)),
                )
                .child(
                    div()
                        .h(px(12.))
                        .w(px(*w))
                        .max_w_full()
                        .rounded_full()
                        .bg(skeleton),
                ),
        );
    }

    block
}

/// Build accordion groups in stable list order: recent projects as stored,
/// then session-only roots, then optional "No project". Selecting a project
/// does not reshuffle the list.
///
/// Missing folders are pruned when the rail opens — do **not** `is_dir()` here;
/// that filesystem hit on every paint made project switches feel laggy.
fn project_groups(
    sessions: &[HaloAgentSession],
    recent: &[String],
    current_root: Option<&str>,
) -> Vec<(String, Vec<HaloAgentSession>)> {
    let mut keys: Vec<String> = Vec::new();
    let mut push_key = |key: String| {
        if key.is_empty() {
            return;
        }
        if !keys.iter().any(|k| k == &key) {
            keys.push(key);
        }
    };

    for path in recent {
        push_key(path.clone());
    }
    // Current root may be missing from recents (e.g. closed prefs); keep it
    // in place at the end rather than jumping it to the top.
    if let Some(root) = current_root {
        push_key(root.to_string());
    }
    for session in sessions {
        if let Some(root) = session.root_path.as_ref() {
            push_key(root.clone());
        }
    }

    let mut groups: Vec<(String, Vec<HaloAgentSession>)> = keys
        .into_iter()
        .map(|key| {
            let kids: Vec<_> = sessions
                .iter()
                .filter(|s| s.root_path.as_deref() == Some(key.as_str()))
                .cloned()
                .collect();
            (key, kids)
        })
        .collect();

    let orphans: Vec<_> = sessions
        .iter()
        .filter(|s| s.root_path.is_none())
        .cloned()
        .collect();
    if !orphans.is_empty() {
        groups.push((NO_PROJECT_KEY.to_string(), orphans));
    }

    groups
}

fn session_row(
    session: &HaloAgentSession,
    is_active: bool,
    archived: bool,
    foreground: Hsla,
    muted: Hsla,
    muted_bg: Hsla,
    accent: Hsla,
    success: Hsla,
    danger: Hsla,
    row_radius: Pixels,
) -> impl IntoElement {
    let id = session.id.clone();
    let title = session.title_or_default().to_string();
    let title_for_delete = title.clone();
    let relative = format_relative_time_ms(session.updated_at);
    let hover_group = SharedString::from(format!("agent-session-row-{id}"));
    // Working → grid_shift; Errored → danger dot; Idle → muted/foreground dot.
    let working = session.status == HaloAgentSessionStatus::Working;
    let errored = session.status == HaloAgentSessionStatus::Errored;
    let selected_bg = muted_bg.opacity(0.85);
    let selected_hover_bg = muted_bg;
    let title_color = if errored { danger } else { foreground };
    let idle_dot = if is_active {
        foreground.opacity(0.7)
    } else {
        muted.opacity(0.35)
    };

    div()
        .id(SharedString::from(format!("coder-agent-session-{id}")))
        .group(hover_group.clone())
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(10.))
        .py(px(6.))
        .rounded(row_radius)
        .cursor_pointer()
        .when(is_active, |el| el.bg(selected_bg))
        .hover(move |s| {
            if is_active {
                s.bg(selected_hover_bg)
            } else {
                s.bg(accent)
            }
        })
        .on_click({
            let id = id.clone();
            move |_, _, cx| {
                let _ = Bridge::halo_agent_select_session(&id, cx);
            }
        })
        .child(
            div()
                .flex_none()
                .w(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .when(working, |el| {
                    el.child(Loader::grid_shift().size(px(14.)).color(success))
                })
                .when(errored && !working, |el| {
                    el.child(div().size(px(8.)).rounded_full().bg(danger.opacity(0.75)))
                })
                .when(!working && !errored, |el| {
                    el.child(div().size(px(8.)).rounded_full().bg(idle_dot))
                }),
        )
        .when_some(session.effective_harness(), |row, harness| {
            let (mark, tint) = super::model_picker::harness_mark(harness);
            row.child(
                div()
                    .flex_none()
                    .opacity(if archived { 0.72 } else { 1. })
                    .child(super::model_picker::render_mark(
                        mark,
                        tint,
                        px(14.),
                        PhosphorIcon::Cpu,
                        muted,
                    )),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .opacity(if archived { 0.72 } else { 1. })
                .text_sm()
                .line_height(px(20.))
                .font_weight(FontWeight::NORMAL)
                .text_color(title_color)
                .child(title),
        )
        // Idle trail: compact last-activity time. Hover swaps it for row actions
        // (Cursor Agents pattern) so the rail stays quiet until needed.
        .child({
            let hover_group = hover_group.clone();
            div()
                .flex_none()
                .overflow_hidden()
                .when(!relative.is_empty(), |el| {
                    el.group_hover(hover_group, |style| style.w(px(0.)).opacity(0.))
                })
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted.opacity(0.85))
                .child(relative)
        })
        .child({
            let actions_w = if archived { px(56.) } else { px(28.) };
            let hover_group = hover_group.clone();
            let mut actions = div()
                .flex_none()
                .w(px(0.))
                .h(px(20.))
                .overflow_hidden()
                .flex()
                .items_center()
                .gap(px(1.))
                .group_hover(hover_group, move |style| style.w(actions_w));

            if archived {
                actions = actions
                    .child(
                        Button::icon(
                            SharedString::from(format!("coder-agent-session-unarchive-{id}")),
                            PhosphorIcon::ArrowUUpLeft,
                        )
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .accessibility_label("Unarchive session")
                        .tooltip("Unarchive")
                        .on_click({
                            let id = id.clone();
                            move |_, _, cx| {
                                let _ = Bridge::halo_agent_unarchive_session(&id, cx);
                            }
                        }),
                    )
                    .child(
                        Button::icon(
                            SharedString::from(format!("coder-agent-session-rm-{id}")),
                            PhosphorIcon::Trash,
                        )
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .accessibility_label("Delete session")
                        .tooltip("Delete")
                        .on_click({
                            let id = id.clone();
                            let title = title_for_delete.clone();
                            move |_, window, cx| {
                                let id = id.clone();
                                confirm_delete(
                                    window,
                                    cx,
                                    "Delete session?",
                                    format!(
                                        "\"{title}\" and its transcript will be permanently deleted. This cannot be undone."
                                    ),
                                    move |_, cx| {
                                        let _ = Bridge::halo_agent_delete_session(&id, cx);
                                    },
                                );
                            }
                        }),
                    );
            } else {
                actions = actions.child(
                    Button::icon(
                        SharedString::from(format!("coder-agent-session-archive-{id}")),
                        PhosphorIcon::Archive,
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .accessibility_label("Archive session")
                    .tooltip("Archive")
                    .on_click({
                        let id = id.clone();
                        move |_, _, cx| {
                            let _ = Bridge::halo_agent_archive_session(&id, cx);
                        }
                    }),
                );
            }

            actions
        })
}

/// Compact relative last-activity label from Unix millis (`now`, `5m`, `3h`,
/// `2d`, `5w`, or a short date past ~8 weeks). Empty for a zero timestamp.
pub(super) fn format_relative_time_ms(ms: u64) -> String {
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    format_relative_time_ms_at(ms, now_ms)
}

fn format_relative_time_ms_at(ms: u64, now_ms: u64) -> String {
    if ms == 0 {
        return String::new();
    }

    // Future timestamps (clock skew) clamp to "now" rather than negative units.
    let seconds = now_ms.saturating_sub(ms) / 1000;
    if seconds < 60 {
        return "now".to_string();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    let weeks = days / 7;
    if weeks < 8 {
        return format!("{weeks}w");
    }

    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|time| time.format("%b %-d, %Y").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::format_relative_time_ms_at;

    const NOW: u64 = 1_700_000_000_000;

    #[test]
    fn relative_time_buckets() {
        assert_eq!(format_relative_time_ms_at(0, NOW), "");
        assert_eq!(format_relative_time_ms_at(NOW, NOW), "now");
        assert_eq!(format_relative_time_ms_at(NOW + 5_000, NOW), "now");
        assert_eq!(format_relative_time_ms_at(NOW - 5 * 60_000, NOW), "5m");
        assert_eq!(format_relative_time_ms_at(NOW - 4 * 3_600_000, NOW), "4h");
        assert_eq!(format_relative_time_ms_at(NOW - 3 * 86_400_000, NOW), "3d");
        assert_eq!(format_relative_time_ms_at(NOW - 14 * 86_400_000, NOW), "2w");
    }
}
